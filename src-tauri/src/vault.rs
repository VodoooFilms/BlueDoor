//! Vault cifrado local: un usuario, un equipo, sin sincronización.
//!
//! Jerarquía de claves
//! -------------------
//! La clave maestra (VMK) es aleatoria y nunca toca el disco en claro. Se
//! guarda envuelta dos veces en `vault.key`: una con la contraseña del usuario
//! y otra con la frase de recuperación, derivadas con Argon2id. Cada entrada
//! usa además su propia DEK, envuelta por la VMK, de modo que eliminar una
//! entrada es destruir 32 bytes: borrado criptográfico inmediato.
//!
//! Formato en disco
//! ----------------
//! ```text
//! vault/
//!   vault.key          documento JSON con los huecos de clave envueltos
//!   index.enc          manifiesto cifrado (títulos, tamaños, hashes)
//!   entries/<uuid>.bde cabecera + criptograma de una entrada
//! ```
//!
//! El nombre de fichero es un UUID: ni el título ni el tipo son legibles en un
//! listado de directorio. Se filtra, inevitablemente, el número de entradas y
//! el tamaño total.
//!
//! Todo lo que se escribe pasa por [`crate::store::atomic_write`]: un fallo a
//! mitad nunca deja una entrada parcial en su sitio definitivo.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use chacha20poly1305::aead::{rand_core::RngCore, Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

use crate::store::atomic_write;

const MAGIC_ENTRY: &[u8; 8] = b"BDORENT1";
const MAGIC_INDEX: &[u8; 8] = b"BDORIDX1";
const MAGIC_BACKUP: &[u8; 8] = b"BDORBAK1";
const SUITE_XCHACHA20_POLY1305: u8 = 1;
const FORMAT_VERSION: u8 = 1;
const MASTER_KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const KEY_FILE: &str = "vault.key";
const INDEX_FILE: &str = "index.enc";
const ENTRIES_DIR: &str = "entries";
const AUTO_LOCK: Duration = Duration::from_secs(300);
/// Tope por entrada. Un archivo mayor se rechaza antes de leerlo para no
/// agotar la memoria; el Vault guarda originales, no bibliotecas de vídeo.
const MAX_ENTRY_BYTES: u64 = 1024 * 1024 * 1024;

/// Techos de los parámetros que llegan desde `vault.key`: un fichero alterado
/// no debe poder pedir una derivación de varios GB al desbloquear.
const MAX_KDF_MEMORY_KIB: u32 = 1024 * 1024;
const MAX_KDF_ITERATIONS: u32 = 16;
const MAX_KDF_PARALLELISM: u32 = 8;

const AAD_PASSWORD: &[u8] = b"bluedoor/vault/master/password/v1";
const AAD_RECOVERY: &[u8] = b"bluedoor/vault/master/recovery/v1";

type MasterKey = Zeroizing<[u8; MASTER_KEY_LEN]>;

// ---------------------------------------------------------------------------
// KDF
// ---------------------------------------------------------------------------

/// Parámetros Argon2id del hueco de clave. Se guardan en `vault.key` para que
/// un vault creado hoy siga abriéndose si mañana cambian los valores por
/// defecto.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KdfParams {
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self::new(64 * 1024, 3, 1)
    }
}

impl KdfParams {
    fn new(memory_kib: u32, iterations: u32, parallelism: u32) -> Self {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        Self {
            memory_kib,
            iterations,
            parallelism,
            salt: encode(&salt),
        }
    }

    /// Perfil rápido para pruebas: la seguridad real depende de que en
    /// producción se use [`KdfParams::default`].
    #[cfg(test)]
    fn fast() -> Self {
        Self::new(8 * 1024, 1, 1)
    }

    fn derive(&self, secret: &str) -> Result<MasterKey, String> {
        if self.memory_kib == 0
            || self.memory_kib > MAX_KDF_MEMORY_KIB
            || self.iterations == 0
            || self.iterations > MAX_KDF_ITERATIONS
            || self.parallelism == 0
            || self.parallelism > MAX_KDF_PARALLELISM
        {
            return Err("Los parámetros de derivación no son válidos.".to_string());
        }
        let salt = decode(&self.salt)?;
        if salt.len() < 8 {
            return Err("Los parámetros de derivación no son válidos.".to_string());
        }
        let params = argon2::Params::new(
            self.memory_kib,
            self.iterations,
            self.parallelism,
            Some(MASTER_KEY_LEN),
        )
        .map_err(|_| "Los parámetros de derivación no son válidos.".to_string())?;
        let argon =
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let mut key = Zeroizing::new([0u8; MASTER_KEY_LEN]);
        argon
            .hash_password_into(secret.as_bytes(), &salt, &mut key[..])
            .map_err(|_| "No se pudo derivar la clave.".to_string())?;
        Ok(key)
    }
}

// ---------------------------------------------------------------------------
// Primitivas AEAD
// ---------------------------------------------------------------------------

fn cipher(key: &[u8]) -> Result<XChaCha20Poly1305, String> {
    XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| "La clave no tiene el tamaño esperado.".to_string())
}

fn seal(key: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<([u8; NONCE_LEN], Vec<u8>), String> {
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher(key)?
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| "No se pudo cifrar el contenido.".to_string())?;
    let mut bytes = [0u8; NONCE_LEN];
    bytes.copy_from_slice(&nonce);
    Ok((bytes, ciphertext))
}

fn open(key: &[u8], aad: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    if nonce.len() != NONCE_LEN {
        return Err("El fichero del Vault está dañado.".to_string());
    }
    cipher(key)?
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| {
            "No se pudo descifrar el contenido: la clave no coincide o el archivo se ha alterado."
                .to_string()
        })
}

fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn decode(text: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|_| "El fichero del Vault está dañado.".to_string())
}

fn random_master_key() -> MasterKey {
    let mut key = Zeroizing::new([0u8; MASTER_KEY_LEN]);
    OsRng.fill_bytes(&mut key[..]);
    key
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ---------------------------------------------------------------------------
// Documentos en disco
// ---------------------------------------------------------------------------

/// Un hueco de clave: parámetros KDF + clave maestra envuelta.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KdfSlot {
    kdf: KdfParams,
    nonce: String,
    wrapped: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyFile {
    version: u8,
    created_at: String,
    label: String,
    password: KdfSlot,
    recovery: KdfSlot,
    /// Reservado para el hueco ligado al hardware (Secure Enclave / TPM).
    /// Se serializa como `null` mientras no exista, de modo que añadirlo más
    /// adelante no cambia el formato.
    #[serde(default)]
    hardware: Option<serde_json::Value>,
}

/// Una entrada tal y como la ve el frontend. Nunca incluye la ruta en disco.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultEntry {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub mime: String,
    pub size: u64,
    pub created_at: String,
    pub description: String,
    pub sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    version: u8,
    entries: Vec<VaultEntry>,
}

impl Manifest {
    fn empty() -> Self {
        Self {
            version: FORMAT_VERSION,
            entries: Vec::new(),
        }
    }
}

fn entry_aad(id: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(16 + id.len());
    aad.extend_from_slice(MAGIC_ENTRY);
    aad.push(FORMAT_VERSION);
    aad.push(SUITE_XCHACHA20_POLY1305);
    aad.extend_from_slice(id.as_bytes());
    aad
}

fn body_aad(id: &str, dek_nonce: &[u8], wrapped_dek: &[u8]) -> Vec<u8> {
    let mut aad = entry_aad(id);
    aad.extend_from_slice(dek_nonce);
    aad.extend_from_slice(wrapped_dek);
    aad
}

fn index_aad() -> Vec<u8> {
    let mut aad = Vec::with_capacity(10);
    aad.extend_from_slice(MAGIC_INDEX);
    aad.push(FORMAT_VERSION);
    aad.push(SUITE_XCHACHA20_POLY1305);
    aad
}

fn ensure_layout(dir: &Path) -> Result<(), String> {
    let entries = dir.join(ENTRIES_DIR);
    std::fs::create_dir_all(&entries).map_err(|e| format!("No se pudo preparar el Vault: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::set_permissions(&entries, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn write_sealed(path: &Path, key: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<(), String> {
    let (nonce, ciphertext) = seal(key, aad, plaintext)?;
    atomic_write(path, |file| {
        file.write_all(&nonce)
            .and_then(|_| file.write_all(&ciphertext))
            .map_err(|e| format!("No se pudo escribir en el Vault: {e}"))
    })
}

fn read_sealed(path: &Path, key: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|_| "El Vault está incompleto.".to_string())?;
    if bytes.len() < NONCE_LEN + TAG_LEN {
        return Err("El archivo del Vault está dañado.".to_string());
    }
    open(key, aad, &bytes[..NONCE_LEN], &bytes[NONCE_LEN..])
}

fn save_manifest(dir: &Path, master: &MasterKey, manifest: &Manifest) -> Result<(), String> {
    let bytes =
        serde_json::to_vec(manifest).map_err(|e| format!("No se pudo preparar el índice: {e}"))?;
    write_sealed(&dir.join(INDEX_FILE), &master[..], &index_aad(), &bytes)
}

fn load_manifest(dir: &Path, master: &MasterKey) -> Result<Manifest, String> {
    let bytes = read_sealed(&dir.join(INDEX_FILE), &master[..], &index_aad())?;
    serde_json::from_slice(&bytes).map_err(|_| "El índice del Vault está dañado.".to_string())
}

fn write_entry_file(
    dir: &Path,
    master: &MasterKey,
    entry: &VaultEntry,
    plaintext: &[u8],
) -> Result<(), String> {
    let dek = random_master_key();
    let dek_nonce = {
        let mut bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut bytes);
        bytes
    };
    let wrapped_dek = cipher(&master[..])?
        .encrypt(
            XNonce::from_slice(&dek_nonce),
            Payload {
                msg: &dek[..],
                aad: &entry_aad(&entry.id),
            },
        )
        .map_err(|_| "No se pudo proteger la entrada.".to_string())?;
    let aad = body_aad(&entry.id, &dek_nonce, &wrapped_dek);
    let (body_nonce, body) = seal(&dek[..], &aad, plaintext)?;

    let mut header = Vec::with_capacity(128);
    header.extend_from_slice(MAGIC_ENTRY);
    header.push(FORMAT_VERSION);
    header.push(SUITE_XCHACHA20_POLY1305);
    header.extend_from_slice(entry.id.as_bytes());
    header.extend_from_slice(&dek_nonce);
    header.extend_from_slice(&wrapped_dek);
    header.extend_from_slice(&body_nonce);
    header.extend_from_slice(&(plaintext.len() as u64).to_le_bytes());

    let path = dir.join(ENTRIES_DIR).join(format!("{}.bde", entry.id));
    atomic_write(&path, |file| {
        file.write_all(&header)
            .and_then(|_| file.write_all(&body))
            .map_err(|e| format!("No se pudo guardar la entrada: {e}"))
    })
}

fn read_entry_file(dir: &Path, master: &MasterKey, id: &str) -> Result<Vec<u8>, String> {
    if !is_valid_id(id) {
        return Err("La entrada no existe.".to_string());
    }
    let path = dir.join(ENTRIES_DIR).join(format!("{id}.bde"));
    let bytes = std::fs::read(&path).map_err(|_| "La entrada no existe.".to_string())?;
    parse_entry(&bytes, master, id)
}

fn parse_entry(bytes: &[u8], master: &MasterKey, id: &str) -> Result<Vec<u8>, String> {
    let damaged = || "La entrada del Vault está dañada.".to_string();
    let head =
        MAGIC_ENTRY.len() + 2 + id.len() + NONCE_LEN + MASTER_KEY_LEN + TAG_LEN + NONCE_LEN + 8;
    if bytes.len() < head {
        return Err(damaged());
    }
    if &bytes[..MAGIC_ENTRY.len()] != MAGIC_ENTRY.as_slice()
        || bytes[MAGIC_ENTRY.len()] != FORMAT_VERSION
        || bytes[MAGIC_ENTRY.len() + 1] != SUITE_XCHACHA20_POLY1305
    {
        return Err(damaged());
    }
    let mut offset = MAGIC_ENTRY.len() + 2;
    if &bytes[offset..offset + id.len()] != id.as_bytes() {
        // La cabecera pertenece a otra entrada: no se intenta descifrar.
        return Err("La entrada del Vault está dañada.".to_string());
    }
    offset += id.len();
    let dek_nonce = &bytes[offset..offset + NONCE_LEN];
    offset += NONCE_LEN;
    let wrapped_dek = &bytes[offset..offset + MASTER_KEY_LEN + TAG_LEN];
    offset += MASTER_KEY_LEN + TAG_LEN;
    let body_nonce = &bytes[offset..offset + NONCE_LEN];
    offset += NONCE_LEN;
    let mut length = [0u8; 8];
    length.copy_from_slice(&bytes[offset..offset + 8]);
    offset += 8;
    let expected = u64::from_le_bytes(length) as usize;
    if bytes.len() != offset + expected + TAG_LEN {
        return Err(damaged());
    }

    let dek = open(&master[..], &entry_aad(id), dek_nonce, wrapped_dek)?;
    let dek = Zeroizing::new(dek);
    let aad = body_aad(id, dek_nonce, wrapped_dek);
    open(&dek[..], &aad, body_nonce, &bytes[offset..])
}

fn is_valid_id(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

// ---------------------------------------------------------------------------
// Creación y desbloqueo
// ---------------------------------------------------------------------------

fn create_vault(
    dir: &Path,
    password: &str,
    kdf: KdfParams,
) -> Result<(KeyFile, MasterKey, String), String> {
    ensure_layout(dir)?;
    let master = random_master_key();
    let mnemonic = bip39::Mnemonic::generate_in(bip39::Language::Spanish, 12)
        .map_err(|_| "No se pudo generar la frase de recuperación.".to_string())?;
    let phrase = mnemonic.to_string();

    let password_slot = wrap_slot(&master, password, AAD_PASSWORD, &kdf)?;
    let recovery_slot = wrap_slot(&master, &phrase, AAD_RECOVERY, &kdf)?;
    let key_file = KeyFile {
        version: FORMAT_VERSION,
        created_at: now_iso(),
        label: "Mi Vault".to_string(),
        password: password_slot,
        recovery: recovery_slot,
        hardware: None,
    };
    write_key_file(dir, &key_file)?;
    save_manifest(dir, &master, &Manifest::empty())?;
    Ok((key_file, master, phrase))
}

fn wrap_slot(
    master: &MasterKey,
    secret: &str,
    aad: &[u8],
    kdf: &KdfParams,
) -> Result<KdfSlot, String> {
    let kek = kdf.derive(secret)?;
    let (nonce, wrapped) = seal(&kek[..], aad, &master[..])?;
    Ok(KdfSlot {
        kdf: kdf.clone(),
        nonce: encode(&nonce),
        wrapped: encode(&wrapped),
    })
}

fn unwrap_slot(slot: &KdfSlot, secret: &str, aad: &[u8]) -> Result<MasterKey, String> {
    let kek = slot.kdf.derive(secret)?;
    let nonce = decode(&slot.nonce)?;
    let wrapped = decode(&slot.wrapped)?;
    let bytes = open(&kek[..], aad, &nonce, &wrapped)?;
    if bytes.len() != MASTER_KEY_LEN {
        return Err("La clave del Vault está dañada.".to_string());
    }
    let mut key = Zeroizing::new([0u8; MASTER_KEY_LEN]);
    key.copy_from_slice(&bytes);
    Ok(key)
}

fn write_key_file(dir: &Path, file: &KeyFile) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(file).map_err(|e| format!("No se pudo guardar la clave: {e}"))?;
    atomic_write(&dir.join(KEY_FILE), |handle| {
        handle
            .write_all(&bytes)
            .map_err(|e| format!("No se pudo guardar la clave: {e}"))
    })
}

fn read_key_file(dir: &Path) -> Result<KeyFile, String> {
    let bytes = std::fs::read(dir.join(KEY_FILE))
        .map_err(|_| "Todavía no existe un Vault en este equipo.".to_string())?;
    serde_json::from_slice(&bytes).map_err(|_| "La clave del Vault está dañada.".to_string())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ---------------------------------------------------------------------------
// Estado en ejecución
// ---------------------------------------------------------------------------

struct Inner {
    master: Option<MasterKey>,
    last_activity: Instant,
}

/// Estado del Vault dentro de `AppState`. La clave solo vive aquí mientras el
/// Vault está desbloqueado; `AUTO_LOCK` la borra al no usarse.
pub struct VaultState {
    inner: Mutex<Inner>,
    /// Serializa las escrituras: dos importaciones simultáneas leerían el
    /// mismo índice y la segunda borraría la entrada de la primera.
    writer: Arc<Mutex<()>>,
}

impl Default for VaultState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                master: None,
                last_activity: Instant::now(),
            }),
            writer: Arc::new(Mutex::new(())),
        }
    }
}

impl VaultState {
    fn writer(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.writer)
    }

    fn unlocked(&self) -> Option<MasterKey> {
        let mut inner = self.inner.lock().ok()?;
        inner.master.as_ref()?;
        if inner.last_activity.elapsed() > AUTO_LOCK {
            inner.master = None;
            return None;
        }
        inner.last_activity = Instant::now();
        inner.master.as_ref().map(|key| {
            let mut copy = Zeroizing::new([0u8; MASTER_KEY_LEN]);
            copy.copy_from_slice(&key[..]);
            copy
        })
    }

    fn require(&self) -> Result<MasterKey, String> {
        self.unlocked()
            .ok_or_else(|| "El Vault está bloqueado.".to_string())
    }

    fn store(&self, master: MasterKey) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "El Vault no está disponible.".to_string())?;
        inner.master = Some(master);
        inner.last_activity = Instant::now();
        Ok(())
    }

    fn lock(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.master = None;
        }
    }

    fn is_unlocked(&self) -> bool {
        self.unlocked().is_some()
    }
}

// ---------------------------------------------------------------------------
// Respuestas para el renderer
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    initialized: bool,
    unlocked: bool,
    entry_count: usize,
    total_bytes: u64,
    auto_lock_seconds: u64,
    created_at: Option<String>,
    file_vault: Option<bool>,
    hardware_protection: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedVault {
    recovery_phrase: String,
    status: VaultStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    copied: usize,
    remaining: usize,
}

// ---------------------------------------------------------------------------
// Órdenes IPC
// ---------------------------------------------------------------------------

fn dir_for(app: &tauri::AppHandle) -> PathBuf {
    crate::paths::vault_dir(app)
}

#[cfg(target_os = "macos")]
fn file_vault_enabled() -> Option<bool> {
    static CACHE: OnceLock<Option<bool>> = OnceLock::new();
    *CACHE.get_or_init(|| {
        let output = std::process::Command::new("/usr/bin/fdesetup")
            .arg("status")
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        Some(text.contains("FileVault is On"))
    })
}

#[cfg(not(target_os = "macos"))]
fn file_vault_enabled() -> Option<bool> {
    None
}

fn build_status(app: &tauri::AppHandle, state: &VaultState) -> VaultStatus {
    let dir = dir_for(app);
    let key_file = read_key_file(&dir).ok();
    let initialized = key_file.is_some();
    let unlocked = state.is_unlocked();
    let (mut count, mut total) = (0usize, 0u64);
    let manifest = state
        .unlocked()
        .and_then(|master| load_manifest(&dir, &master).ok());
    if let Some(manifest) = manifest {
        count = manifest.entries.len();
        total = manifest.entries.iter().map(|entry| entry.size).sum();
    }
    VaultStatus {
        initialized,
        unlocked,
        entry_count: count,
        total_bytes: total,
        auto_lock_seconds: AUTO_LOCK.as_secs(),
        created_at: key_file.map(|file| file.created_at),
        file_vault: file_vault_enabled(),
        // El hueco ligado al hardware todavía no está implementado; se informa
        // como `false` para que la interfaz no prometa una protección que no
        // existe.
        hardware_protection: false,
    }
}

#[tauri::command]
pub(crate) fn vault_status(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
) -> Result<VaultStatus, String> {
    Ok(build_status(&app, &state))
}

#[tauri::command]
pub(crate) async fn vault_create(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
    password: String,
) -> Result<CreatedVault, String> {
    if password.chars().count() < 8 {
        return Err("La contraseña debe tener al menos 8 caracteres.".to_string());
    }
    let dir = dir_for(&app);
    if dir.join(KEY_FILE).exists() {
        return Err("Ya existe un Vault en este equipo.".to_string());
    }
    let handle = app.clone();
    let (_, master, phrase) = tauri::async_runtime::spawn_blocking(move || {
        create_vault(&dir, &password, KdfParams::default())
    })
    .await
    .map_err(|e| format!("No se pudo crear el Vault: {e}"))??;
    state.store(master)?;
    Ok(CreatedVault {
        recovery_phrase: phrase,
        status: build_status(&handle, &state),
    })
}

#[tauri::command]
pub(crate) async fn vault_unlock(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
    password: String,
) -> Result<VaultStatus, String> {
    let dir = dir_for(&app);
    let handle = app.clone();
    let master = tauri::async_runtime::spawn_blocking(move || {
        let key_file = read_key_file(&dir)?;
        unwrap_slot(&key_file.password, &password, AAD_PASSWORD)
            .map_err(|_| "La contraseña no es correcta.".to_string())
    })
    .await
    .map_err(|e| format!("No se pudo abrir el Vault: {e}"))??;
    state.store(master)?;
    Ok(build_status(&handle, &state))
}

#[tauri::command]
pub(crate) async fn vault_unlock_recovery(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
    phrase: String,
) -> Result<VaultStatus, String> {
    let dir = dir_for(&app);
    let handle = app.clone();
    let master = tauri::async_runtime::spawn_blocking(move || {
        let normalized = phrase.split_whitespace().collect::<Vec<_>>().join(" ");
        let mnemonic = bip39::Mnemonic::parse_in(bip39::Language::Spanish, &normalized)
            .or_else(|_| bip39::Mnemonic::parse_in(bip39::Language::English, &normalized))
            .map_err(|_| "La frase de recuperación no es válida.".to_string())?;
        let key_file = read_key_file(&dir)?;
        unwrap_slot(&key_file.recovery, &mnemonic.to_string(), AAD_RECOVERY)
            .map_err(|_| "La frase de recuperación no es válida.".to_string())
    })
    .await
    .map_err(|e| format!("No se pudo abrir el Vault: {e}"))??;
    state.store(master)?;
    Ok(build_status(&handle, &state))
}

#[tauri::command]
pub(crate) fn vault_lock(state: State<'_, VaultState>) {
    state.lock();
}

#[tauri::command]
pub(crate) fn vault_list(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
) -> Result<Vec<VaultEntry>, String> {
    let master = state.require()?;
    let manifest = load_manifest(&dir_for(&app), &master)?;
    let mut entries = manifest.entries;
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(entries)
}

#[tauri::command]
pub(crate) async fn vault_import(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
    title: Option<String>,
) -> Result<Option<VaultEntry>, String> {
    let master = state.require()?;
    let handle = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Añadir al Vault")
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("No se pudo abrir el selector: {e}"))?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|_| "El archivo elegido no es una ruta local.".to_string())?;
    let dir = dir_for(&handle);
    let writer = state.writer();
    let entry = tauri::async_runtime::spawn_blocking(move || {
        let _guard = writer
            .lock()
            .map_err(|_| "El Vault no está disponible.".to_string())?;
        store_entry_from_path(&dir, &master, &path, title)
    })
    .await
    .map_err(|e| format!("No se pudo guardar la entrada: {e}"))??;
    Ok(Some(entry))
}

/// Comprueba el archivo **antes** de leerlo entero: un vídeo de varios GB no
/// debe agotar la memoria del equipo ni dejar el Vault a medias.
fn store_entry_from_path(
    dir: &Path,
    master: &MasterKey,
    path: &Path,
    title: Option<String>,
) -> Result<VaultEntry, String> {
    let metadata =
        std::fs::metadata(path).map_err(|e| format!("No se pudo leer el archivo: {e}"))?;
    if !metadata.is_file() {
        return Err("Solo se pueden añadir archivos, no carpetas.".to_string());
    }
    if metadata.len() == 0 {
        return Err("El archivo está vacío.".to_string());
    }
    if metadata.len() > MAX_ENTRY_BYTES {
        return Err(format!(
            "El archivo supera el máximo por entrada ({} GB).",
            MAX_ENTRY_BYTES / (1024 * 1024 * 1024)
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("No se pudo leer el archivo: {e}"))?;
    store_entry(dir, master, path, title, bytes)
}

fn store_entry(
    dir: &Path,
    master: &MasterKey,
    path: &Path,
    title: Option<String>,
    bytes: Vec<u8>,
) -> Result<VaultEntry, String> {
    if bytes.is_empty() {
        return Err("El archivo está vacío.".to_string());
    }
    ensure_layout(dir)?;
    let mut manifest = load_manifest(dir, master)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "Sin título".to_string());
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // Un JSON importado se trata como mapa de alias; las transcripciones
    // llegan por `vault_migrate_transcripts` con su propio tipo.
    let kind = if is_audio_extension(&extension) {
        "audio"
    } else if extension == "json" {
        "map"
    } else {
        "document"
    };
    let entry = VaultEntry {
        id: uuid::Uuid::new_v4().to_string(),
        title: title
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or(name),
        kind: kind.to_string(),
        mime: mime_for(&extension).to_string(),
        size: bytes.len() as u64,
        created_at: now_iso(),
        description: describe(kind),
        sha256: sha256_hex(&bytes),
    };
    write_entry_file(dir, master, &entry, &bytes)?;
    manifest.entries.push(entry.clone());
    if let Err(error) = save_manifest(dir, master, &manifest) {
        // Sin índice la entrada quedaría huérfana: se retira y se informa.
        let _ = std::fs::remove_file(dir.join(ENTRIES_DIR).join(format!("{}.bde", entry.id)));
        return Err(error);
    }
    Ok(entry)
}

fn is_audio_extension(extension: &str) -> bool {
    matches!(
        extension,
        "wav" | "mp3" | "m4a" | "aac" | "ogg" | "oga" | "flac" | "webm" | "mp4" | "m4v" | "mov"
    )
}

fn mime_for(extension: &str) -> &'static str {
    match extension {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "json" => "application/json",
        "txt" => "text/plain",
        "rtf" => "application/rtf",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "mp4" | "m4v" | "mov" => "video/mp4",
        _ => "application/octet-stream",
    }
}

fn describe(kind: &str) -> String {
    match kind {
        "audio" => "Transcripción y audio guardados en este equipo.".to_string(),
        "map" => "Mapa de alias guardado en este equipo, cifrado en reposo.".to_string(),
        _ => "Documento guardado en este equipo, cifrado en reposo.".to_string(),
    }
}

#[tauri::command]
pub(crate) async fn vault_export_entry(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
    id: String,
    default_name: String,
) -> Result<Option<String>, String> {
    let master = state.require()?;
    let dir = dir_for(&app);
    let bytes = read_entry_file(&dir, &master, &id)?;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Exportar desde el Vault")
            .set_file_name(sanitize_file_name(&default_name))
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("No se pudo abrir el selector: {e}"))?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let destination = picked
        .into_path()
        .map_err(|_| "El destino no es una ruta local válida.".to_string())?;
    atomic_write(&destination, |file| {
        file.write_all(&bytes)
            .map_err(|e| format!("No se pudo escribir el destino: {e}"))
    })?;
    Ok(Some(destination.to_string_lossy().to_string()))
}

fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "entrada".to_string()
    } else {
        trimmed.to_string()
    }
}

#[tauri::command]
pub(crate) fn vault_delete_entry(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<(), String> {
    let master = state.require()?;
    let _writer = state
        .writer
        .lock()
        .map_err(|_| "El Vault no está disponible.".to_string())?;
    let dir = dir_for(&app);
    let mut manifest = load_manifest(&dir, &master)?;
    let before = manifest.entries.len();
    manifest.entries.retain(|entry| entry.id != id);
    if manifest.entries.len() == before {
        return Err("La entrada ya no existe.".to_string());
    }
    // Primero el índice: si el borrado del criptograma fallase, quedaría un
    // fichero huérfano ilegible en vez de una entrada fantasma en la lista.
    save_manifest(&dir, &master, &manifest)?;
    let _ = std::fs::remove_file(dir.join(ENTRIES_DIR).join(format!("{id}.bde")));
    Ok(())
}

// ---------------------------------------------------------------------------
// Copia de seguridad
// ---------------------------------------------------------------------------

/// La copia es un contenedor de los ficheros **ya cifrados**. No se vuelve a
/// cifrar nada: `vault.key` viaja dentro, así que la copia se restaura con la
/// contraseña o con la frase de recuperación originales. Si el usuario cambia
/// la contraseña más adelante, las copias antiguas siguen abriéndose con la
/// anterior, que es el comportamiento esperado.
///
/// Se escribe y se lee en flujo: una entrada de cientos de MB se copia por
/// bloques y nunca ocupa la memoria del proceso entera.
const BACKUP_HEADER_LEN: usize = 13;
/// Tope para los dos documentos que se descifran en memoria (`vault.key` e
/// `index.enc`). Un índice real de 100 000 entradas no llega a 64 MiB.
const MAX_BACKUP_METADATA: u64 = 64 * 1024 * 1024;
const MAX_BACKUP_FILES: u32 = 100_000;

fn backup_damaged() -> String {
    "La copia no es válida o está dañada.".to_string()
}

fn backup_members(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    if !dir.join(KEY_FILE).exists() {
        return Err("Todavía no existe un Vault en este equipo.".to_string());
    }
    let index = dir.join(INDEX_FILE);
    if !index.exists() {
        return Err("El Vault está incompleto.".to_string());
    }
    let mut members = vec![
        (KEY_FILE.to_string(), dir.join(KEY_FILE)),
        (INDEX_FILE.to_string(), index),
    ];
    if let Ok(reader) = std::fs::read_dir(dir.join(ENTRIES_DIR)) {
        for item in reader.flatten() {
            let name = item.file_name().to_string_lossy().to_string();
            let id = name.strip_suffix(".bde").unwrap_or_default();
            if !is_valid_id(id) {
                continue;
            }
            members.push((format!("{ENTRIES_DIR}/{name}"), item.path()));
        }
    }
    Ok(members)
}

fn write_backup<W: Write>(dir: &Path, writer: &mut W) -> Result<(), String> {
    let members = backup_members(dir)?;
    if members.len() as u64 > MAX_BACKUP_FILES as u64 {
        return Err("El Vault tiene demasiadas entradas para una copia.".to_string());
    }
    writer
        .write_all(MAGIC_BACKUP)
        .and_then(|_| writer.write_all(&[FORMAT_VERSION]))
        .and_then(|_| writer.write_all(&(members.len() as u32).to_le_bytes()))
        .map_err(|e| format!("No se pudo escribir la copia: {e}"))?;
    for (name, path) in members {
        let name_bytes = name.as_bytes();
        if name_bytes.len() > u16::MAX as usize {
            return Err("El nombre de un fichero es demasiado largo.".to_string());
        }
        let mut source = std::fs::File::open(&path)
            .map_err(|_| "No se pudo leer una entrada del Vault.".to_string())?;
        let size = source
            .metadata()
            .map_err(|_| "No se pudo leer una entrada del Vault.".to_string())?
            .len();
        writer
            .write_all(&(name_bytes.len() as u16).to_le_bytes())
            .and_then(|_| writer.write_all(name_bytes))
            .and_then(|_| writer.write_all(&size.to_le_bytes()))
            .map_err(|e| format!("No se pudo escribir la copia: {e}"))?;
        let copied = std::io::copy(&mut source, writer)
            .map_err(|e| format!("No se pudo escribir la copia: {e}"))?;
        if copied != size {
            return Err("No se pudo leer una entrada del Vault.".to_string());
        }
    }
    Ok(())
}

/// Lector secuencial del contenedor. Valida cabecera y nombres antes de que
/// ningún byte llegue al disco.
struct BackupReader<R: Read> {
    inner: R,
    remaining: u32,
}

impl<R: Read> BackupReader<R> {
    fn open(mut inner: R) -> Result<Self, String> {
        let mut header = [0u8; BACKUP_HEADER_LEN];
        inner
            .read_exact(&mut header)
            .map_err(|_| "El archivo elegido no es una copia de BlueDoor.".to_string())?;
        if &header[..8] != MAGIC_BACKUP.as_slice() || header[8] != FORMAT_VERSION {
            return Err("El archivo elegido no es una copia de BlueDoor.".to_string());
        }
        let total = u32::from_le_bytes([header[9], header[10], header[11], header[12]]);
        if total > MAX_BACKUP_FILES {
            return Err(backup_damaged());
        }
        Ok(Self {
            inner,
            remaining: total,
        })
    }

    /// Devuelve `(nombre, tamaño)` del siguiente miembro. El cuerpo se lee
    /// con [`BackupReader::body`].
    fn next(&mut self) -> Result<Option<(String, u64)>, String> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let mut length = [0u8; 2];
        self.inner
            .read_exact(&mut length)
            .map_err(|_| backup_damaged())?;
        let name_len = u16::from_le_bytes(length) as usize;
        if name_len == 0 || name_len > 4096 {
            return Err(backup_damaged());
        }
        let mut name_bytes = vec![0u8; name_len];
        self.inner
            .read_exact(&mut name_bytes)
            .map_err(|_| backup_damaged())?;
        let name = String::from_utf8(name_bytes).map_err(|_| backup_damaged())?;
        if !safe_backup_name(&name) {
            return Err("La copia contiene rutas no permitidas.".to_string());
        }
        let mut size = [0u8; 8];
        self.inner
            .read_exact(&mut size)
            .map_err(|_| backup_damaged())?;
        self.remaining -= 1;
        Ok(Some((name, u64::from_le_bytes(size))))
    }

    fn body(&mut self) -> &mut R {
        &mut self.inner
    }

    /// El contenedor no debe tener bytes de más al final.
    fn finish(mut self) -> Result<(), String> {
        let mut extra = [0u8; 1];
        match self.inner.read(&mut extra) {
            Ok(0) => Ok(()),
            _ => Err(backup_damaged()),
        }
    }
}

fn safe_backup_name(name: &str) -> bool {
    if name.contains("..") || name.starts_with('/') || name.contains('\\') {
        return false;
    }
    name == KEY_FILE
        || name == INDEX_FILE
        || (name.starts_with("entries/") && name.ends_with(".bde"))
}

fn open_backup(archive: &Path) -> Result<BackupReader<std::io::BufReader<std::fs::File>>, String> {
    let file =
        std::fs::File::open(archive).map_err(|e| format!("No se pudo leer la copia: {e}"))?;
    BackupReader::open(std::io::BufReader::new(file))
}

fn read_member<R: Read>(reader: &mut BackupReader<R>, size: u64) -> Result<Vec<u8>, String> {
    if size > MAX_BACKUP_METADATA {
        return Err(backup_damaged());
    }
    let mut bytes = vec![0u8; size as usize];
    reader
        .body()
        .read_exact(&mut bytes)
        .map_err(|_| "La copia está incompleta.".to_string())?;
    Ok(bytes)
}

fn skip_member<R: Read>(reader: &mut BackupReader<R>, size: u64) -> Result<(), String> {
    let skipped = std::io::copy(&mut reader.body().take(size), &mut std::io::sink())
        .map_err(|_| "La copia está incompleta.".to_string())?;
    if skipped != size {
        return Err("La copia está incompleta.".to_string());
    }
    Ok(())
}

/// Primer pase: solo extrae `vault.key` e `index.enc`. Todavía no se escribe
/// nada en el Vault del usuario.
fn read_backup_metadata(archive: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut reader = open_backup(archive)?;
    let mut key = None;
    let mut index = None;
    while let Some((name, size)) = reader.next()? {
        if name == KEY_FILE {
            key = Some(read_member(&mut reader, size)?);
        } else if name == INDEX_FILE {
            index = Some(read_member(&mut reader, size)?);
        } else {
            skip_member(&mut reader, size)?;
        }
    }
    reader.finish()?;
    Ok((
        key.ok_or_else(|| "La copia no contiene la clave del Vault.".to_string())?,
        index.ok_or_else(|| "La copia no contiene el índice del Vault.".to_string())?,
    ))
}

fn unwrap_master_from_key(key_file: &KeyFile, secret: &str) -> Result<MasterKey, String> {
    let normalized = secret.split_whitespace().collect::<Vec<_>>().join(" ");
    let invalid = || "La contraseña o la frase no son válidas.".to_string();
    unwrap_slot(&key_file.password, secret, AAD_PASSWORD)
        .map_err(|_| invalid())
        .or_else(|_| {
            let mnemonic = bip39::Mnemonic::parse_in(bip39::Language::Spanish, &normalized)
                .or_else(|_| bip39::Mnemonic::parse_in(bip39::Language::English, &normalized))
                .map_err(|_| invalid())?;
            unwrap_slot(&key_file.recovery, &mnemonic.to_string(), AAD_RECOVERY)
                .map_err(|_| invalid())
        })
}

#[tauri::command]
pub(crate) async fn vault_backup(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
) -> Result<Option<String>, String> {
    state.require()?;
    let dir = dir_for(&app);
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Guardar copia cifrada del Vault")
            .set_file_name("Mi Vault.bluedoor")
            .add_filter("Copia del Vault", &["bluedoor"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("No se pudo abrir el selector: {e}"))?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let destination = picked
        .into_path()
        .map_err(|_| "El destino no es una ruta local válida.".to_string())?;
    let saved = destination.to_string_lossy().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        atomic_write(&destination, |file| write_backup(&dir, file))
    })
    .await
    .map_err(|e| format!("No se pudo guardar la copia: {e}"))??;
    Ok(Some(saved))
}

#[tauri::command]
pub(crate) async fn vault_restore(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
    secret: String,
    replace: bool,
) -> Result<VaultStatus, String> {
    let dir = dir_for(&app);
    if dir.join(KEY_FILE).exists() && !replace {
        return Err("Ya existe un Vault en este equipo.".to_string());
    }
    let handle = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Seleccionar copia del Vault")
            .add_filter("Copia del Vault", &["bluedoor"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("No se pudo abrir el selector: {e}"))?;
    let Some(picked) = picked else {
        return Err("No se eligió ninguna copia.".to_string());
    };
    let path = picked
        .into_path()
        .map_err(|_| "La copia elegida no es una ruta local.".to_string())?;
    let writer = state.writer();
    let master = tauri::async_runtime::spawn_blocking(move || {
        let _guard = writer
            .lock()
            .map_err(|_| "El Vault no está disponible.".to_string())?;
        restore_backup(&dir, &path, &secret)
    })
    .await
    .map_err(|e| format!("No se pudo restaurar la copia: {e}"))??;
    state.store(master)?;
    Ok(build_status(&handle, &state))
}

/// Verifica la copia **antes** de escribir nada: si la contraseña no abre el
/// `vault.key` que contiene, el Vault existente no se toca. Solo después se
/// copian los miembros en un segundo pase, por bloques.
fn restore_backup(dir: &Path, archive: &Path, secret: &str) -> Result<MasterKey, String> {
    let (key_bytes, index_bytes) = read_backup_metadata(archive)?;
    let key_file: KeyFile = serde_json::from_slice(&key_bytes)
        .map_err(|_| "La clave de la copia está dañada.".to_string())?;
    let master = unwrap_master_from_key(&key_file, secret)?;

    if index_bytes.len() < NONCE_LEN + TAG_LEN {
        return Err("El índice de la copia está dañado.".to_string());
    }
    let plain = open(
        &master[..],
        &index_aad(),
        &index_bytes[..NONCE_LEN],
        &index_bytes[NONCE_LEN..],
    )?;
    let manifest: Manifest = serde_json::from_slice(&plain)
        .map_err(|_| "El índice de la copia está dañado.".to_string())?;

    ensure_layout(dir)?;
    let mut reader = open_backup(archive)?;
    while let Some((name, size)) = reader.next()? {
        let target = dir.join(&name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("No se pudo restaurar el Vault: {e}"))?;
        }
        atomic_write(&target, |out| {
            let copied = std::io::copy(&mut reader.body().take(size), out)
                .map_err(|e| format!("No se pudo restaurar el Vault: {e}"))?;
            if copied != size {
                return Err("La copia está incompleta.".to_string());
            }
            Ok(())
        })?;
    }
    reader.finish()?;

    // Comprobación final: cada entrada del manifiesto debe descifrarse.
    for entry in &manifest.entries {
        read_entry_file(dir, &master, &entry.id)
            .map_err(|_| "Una entrada de la copia está dañada.".to_string())?;
    }
    Ok(master)
}

// ---------------------------------------------------------------------------
// Traslado de transcripciones
// ---------------------------------------------------------------------------

/// Copia las transcripciones locales al Vault, verifica cada copia y **solo
/// entonces** retira las migradas del historial en claro. Si algo falla por el
/// camino, el historial queda intacto.
#[tauri::command]
pub(crate) async fn vault_migrate_transcripts(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
) -> Result<MigrationReport, String> {
    let master = state.require()?;
    let dir = dir_for(&app);
    let history = crate::paths::transcripts_file(&app);
    let writer = state.writer();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = writer
            .lock()
            .map_err(|_| "El Vault no está disponible.".to_string())?;
        migrate(&dir, &master, &history)
    })
    .await
    .map_err(|e| format!("No se pudo trasladar el historial: {e}"))?
}

fn migrate(dir: &Path, master: &MasterKey, history: &Path) -> Result<MigrationReport, String> {
    let Ok(bytes) = std::fs::read(history) else {
        return Ok(MigrationReport {
            copied: 0,
            remaining: 0,
        });
    };
    let transcripts: Vec<crate::types::StoredTranscript> =
        serde_json::from_slice(&bytes).map_err(|_| {
            "No se pudo leer el historial; el archivo se conserva tal cual.".to_string()
        })?;
    if transcripts.is_empty() {
        return Ok(MigrationReport {
            copied: 0,
            remaining: 0,
        });
    }

    ensure_layout(dir)?;
    let mut manifest = load_manifest(dir, master)?;
    let mut migrated_ids = Vec::new();
    for transcript in &transcripts {
        let payload = serde_json::to_vec(transcript)
            .map_err(|e| format!("No se pudo preparar una transcripción: {e}"))?;
        let entry = VaultEntry {
            id: uuid::Uuid::new_v4().to_string(),
            title: transcript.title.clone(),
            kind: "audio".to_string(),
            mime: "application/json".to_string(),
            size: payload.len() as u64,
            created_at: now_iso(),
            description: "Transcripción trasladada desde el historial local.".to_string(),
            sha256: sha256_hex(&payload),
        };
        write_entry_file(dir, master, &entry, &payload)?;
        // Verificación real: se vuelve a descifrar y se compara el hash.
        let restored = read_entry_file(dir, master, &entry.id)?;
        if sha256_hex(&restored) != entry.sha256 {
            return Err(
                "La verificación de una transcripción falló; no se borró nada del historial."
                    .to_string(),
            );
        }
        manifest.entries.push(entry);
        migrated_ids.push(transcript.id.clone());
    }
    save_manifest(dir, master, &manifest)?;

    let remaining: Vec<crate::types::StoredTranscript> = transcripts
        .into_iter()
        .filter(|transcript| !migrated_ids.contains(&transcript.id))
        .collect();
    let encoded = serde_json::to_vec_pretty(&remaining)
        .map_err(|e| format!("No se pudo actualizar el historial: {e}"))?;
    atomic_write(history, |file| {
        file.write_all(&encoded)
            .map_err(|e| format!("No se pudo actualizar el historial: {e}"))
    })?;

    Ok(MigrationReport {
        copied: migrated_ids.len(),
        remaining: remaining.len(),
    })
}

#[cfg(test)]
mod tests;
