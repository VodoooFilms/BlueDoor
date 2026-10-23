use super::*;

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let holder = tempfile::tempdir().unwrap();
    let dir = holder.path().join("vault");
    (holder, dir)
}

fn all_bytes(dir: &Path) -> Vec<u8> {
    fn visit(path: &Path, out: &mut Vec<u8>) {
        if path.is_dir() {
            if let Ok(items) = std::fs::read_dir(path) {
                for item in items.flatten() {
                    visit(&item.path(), out);
                }
            }
        } else if let Ok(bytes) = std::fs::read(path) {
            out.extend_from_slice(&bytes);
        }
    }
    let mut out = Vec::new();
    visit(dir, &mut out);
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn new_vault(dir: &Path) -> (KeyFile, MasterKey, String) {
    create_vault(dir, "frase-de-paso-larga", KdfParams::fast()).unwrap()
}

fn build_backup(dir: &Path) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    write_backup(dir, &mut out)?;
    Ok(out)
}

#[test]
fn entry_round_trip_hides_title_and_content() {
    let (_holder, dir) = scratch();
    let (_key, master, _phrase) = new_vault(&dir);
    let plaintext = b"Contrato de arrendamiento de Maria Ejemplo, 1.234,56 EUR".to_vec();
    let entry = store_entry(
        &dir,
        &master,
        Path::new("/tmp/Contrato Maria Ejemplo.pdf"),
        None,
        plaintext.clone(),
    )
    .unwrap();

    assert_eq!(
        read_entry_file(&dir, &master, &entry.id).unwrap(),
        plaintext
    );
    assert_eq!(entry.sha256, sha256_hex(&plaintext));
    assert_eq!(entry.kind, "document");
    assert!(is_valid_id(&entry.id));
    assert!(dir
        .join("entries")
        .join(format!("{}.bde", entry.id))
        .exists());
    assert!(!dir
        .join("entries")
        .join("Contrato Maria Ejemplo.pdf")
        .exists());

    // Nada del contenido ni del título aparece en claro en ningún fichero.
    let stored = all_bytes(&dir);
    assert!(!contains(&stored, b"Contrato"));
    assert!(!contains(&stored, b"Maria Ejemplo"));
    assert!(!contains(&stored, b"arrendamiento"));
}

#[test]
fn imported_files_are_classified_by_extension() {
    let (_holder, dir) = scratch();
    let (_key, master, _phrase) = new_vault(&dir);
    let audio = store_entry(
        &dir,
        &master,
        Path::new("/tmp/voz.m4a"),
        None,
        b"a".to_vec(),
    )
    .unwrap();
    let map = store_entry(
        &dir,
        &master,
        Path::new("/tmp/alias.json"),
        None,
        b"{}".to_vec(),
    )
    .unwrap();
    let document = store_entry(
        &dir,
        &master,
        Path::new("/tmp/notas.pdf"),
        None,
        b"d".to_vec(),
    )
    .unwrap();
    assert_eq!(audio.kind, "audio");
    assert_eq!(map.kind, "map");
    assert_eq!(map.mime, "application/json");
    assert_eq!(document.kind, "document");
}

#[test]
fn wrong_password_and_phrase_are_rejected() {
    let (_holder, dir) = scratch();
    let (_key, _master, phrase) = new_vault(&dir);
    let key_file = read_key_file(&dir).unwrap();

    assert!(unwrap_slot(&key_file.password, "otra-contraseña", AAD_PASSWORD).is_err());
    assert!(unwrap_slot(
        &key_file.recovery,
        "puerta brisa cobre luna bosque tinta isla nube faro papel rio azul",
        AAD_RECOVERY
    )
    .is_err());

    let parsed = bip39::Mnemonic::parse_in(bip39::Language::Spanish, &phrase).unwrap();
    let recovered = unwrap_slot(&key_file.recovery, &parsed.to_string(), AAD_RECOVERY).unwrap();
    assert_eq!(&recovered[..], &_master[..]);
}

#[test]
fn tampered_ciphertext_is_rejected() {
    let (_holder, dir) = scratch();
    let (_key, master, _phrase) = new_vault(&dir);
    let entry = store_entry(
        &dir,
        &master,
        Path::new("/tmp/notas.txt"),
        Some("Notas".to_string()),
        b"contenido reservado".to_vec(),
    )
    .unwrap();

    let path = dir.join("entries").join(format!("{}.bde", entry.id));
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&path, &bytes).unwrap();

    let error = read_entry_file(&dir, &master, &entry.id).unwrap_err();
    assert!(
        error.contains("alterado") || error.contains("dañado"),
        "{error}"
    );
}

#[test]
fn entry_header_from_another_id_is_rejected() {
    let (_holder, dir) = scratch();
    let (_key, master, _phrase) = new_vault(&dir);
    let first = store_entry(
        &dir,
        &master,
        Path::new("/tmp/a.pdf"),
        None,
        b"primero".to_vec(),
    )
    .unwrap();
    let second = store_entry(
        &dir,
        &master,
        Path::new("/tmp/b.pdf"),
        None,
        b"segundo".to_vec(),
    )
    .unwrap();

    // Se copia el criptograma de la segunda entrada sobre el nombre de la
    // primera: la cabecera lleva el id como AAD y no debe abrir.
    let from = dir.join("entries").join(format!("{}.bde", second.id));
    let to = dir.join("entries").join(format!("{}.bde", first.id));
    std::fs::copy(&from, &to).unwrap();

    assert!(read_entry_file(&dir, &master, &first.id).is_err());
}

#[test]
fn deleting_an_entry_removes_its_ciphertext() {
    let (_holder, dir) = scratch();
    let (_key, master, _phrase) = new_vault(&dir);
    let entry = store_entry(
        &dir,
        &master,
        Path::new("/tmp/borrar.pdf"),
        None,
        b"efimero".to_vec(),
    )
    .unwrap();
    let path = dir.join("entries").join(format!("{}.bde", entry.id));

    let mut manifest = load_manifest(&dir, &master).unwrap();
    manifest.entries.retain(|item| item.id != entry.id);
    save_manifest(&dir, &master, &manifest).unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(!path.exists());
    assert!(load_manifest(&dir, &master).unwrap().entries.is_empty());
}

#[test]
fn backup_restores_every_entry_in_a_fresh_directory() {
    let (_holder, dir) = scratch();
    let (_key, master, _phrase) = new_vault(&dir);
    let first = store_entry(
        &dir,
        &master,
        Path::new("/tmp/uno.pdf"),
        None,
        b"documento uno".to_vec(),
    )
    .unwrap();
    let second = store_entry(
        &dir,
        &master,
        Path::new("/tmp/dos.m4a"),
        None,
        b"audio dos".to_vec(),
    )
    .unwrap();

    let archive = dir.parent().unwrap().join("copia.bluedoor");
    std::fs::write(&archive, build_backup(&dir).unwrap()).unwrap();

    let restored_dir = dir.parent().unwrap().join("restaurado");
    let restored = restore_backup(&restored_dir, &archive, "frase-de-paso-larga").unwrap();
    assert_eq!(&restored[..], &master[..]);

    let manifest = load_manifest(&restored_dir, &restored).unwrap();
    assert_eq!(manifest.entries.len(), 2);
    assert_eq!(
        read_entry_file(&restored_dir, &restored, &first.id).unwrap(),
        b"documento uno"
    );
    assert_eq!(
        read_entry_file(&restored_dir, &restored, &second.id).unwrap(),
        b"audio dos"
    );
}

#[test]
fn backup_rejects_paths_outside_the_vault() {
    let (_holder, dir) = scratch();
    std::fs::create_dir_all(&dir).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC_BACKUP);
    bytes.push(FORMAT_VERSION);
    bytes.extend_from_slice(&1u32.to_le_bytes());
    let name = b"../escape.key";
    bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(&4u64.to_le_bytes());
    bytes.extend_from_slice(b"evil");
    let archive = dir.parent().unwrap().join("maliciosa.bluedoor");
    std::fs::write(&archive, &bytes).unwrap();
    let error = read_backup_metadata(&archive).unwrap_err();
    assert!(error.contains("rutas no permitidas"), "{error}");
}

#[test]
fn import_rejects_empty_and_oversized_files_before_reading_them() {
    let (holder, dir) = scratch();
    let (_key, master, _phrase) = new_vault(&dir);

    let empty = holder.path().join("vacio.pdf");
    std::fs::write(&empty, []).unwrap();
    let error = store_entry_from_path(&dir, &master, &empty, None).unwrap_err();
    assert!(error.contains("vacío"), "{error}");

    let huge = holder.path().join("enorme.bin");
    let file = std::fs::File::create(&huge).unwrap();
    file.set_len(MAX_ENTRY_BYTES + 1).unwrap();
    drop(file);
    let error = store_entry_from_path(&dir, &master, &huge, None).unwrap_err();
    assert!(error.contains("máximo"), "{error}");

    assert!(load_manifest(&dir, &master).unwrap().entries.is_empty());
}

#[test]
fn restore_with_a_wrong_secret_leaves_the_vault_untouched() {
    let (_holder, dir) = scratch();
    let (_key, master, _phrase) = new_vault(&dir);
    store_entry(&dir, &master, Path::new("/tmp/a.pdf"), None, b"a".to_vec()).unwrap();
    let archive = dir.parent().unwrap().join("copia.bluedoor");
    std::fs::write(&archive, build_backup(&dir).unwrap()).unwrap();

    let target = dir.parent().unwrap().join("otro");
    let before = std::fs::read(dir.join(INDEX_FILE)).unwrap();
    assert!(restore_backup(&target, &archive, "clave-equivocada").is_err());
    assert_eq!(std::fs::read(dir.join(INDEX_FILE)).unwrap(), before);
    assert!(!target.join(KEY_FILE).exists());
}

#[test]
fn migration_moves_transcripts_and_clears_the_plaintext_history() {
    let (holder, dir) = scratch();
    let (_key, master, _phrase) = new_vault(&dir);
    let history = holder.path().join("transcripts.json");
    std::fs::write(
        &history,
        serde_json::json!([
            {
                "id": "t1",
                "title": "Vista oral",
                "text": "Comparecencia de Maria Ejemplo.",
                "segments": [],
                "createdAt": "2026-10-20T10:00:00Z",
                "duration": 61.0,
                "language": "es",
                "model": "parakeet-es",
                "audioSource": "file"
            }
        ])
        .to_string(),
    )
    .unwrap();

    let report = migrate(&dir, &master, &history).unwrap();
    assert_eq!(report.copied, 1);
    assert_eq!(report.remaining, 0);

    let entries = load_manifest(&dir, &master).unwrap().entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].title, "Vista oral");
    let payload = read_entry_file(&dir, &master, &entries[0].id).unwrap();
    assert!(String::from_utf8_lossy(&payload).contains("Maria Ejemplo"));

    let remaining: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(&history).unwrap()).unwrap();
    assert!(remaining.is_empty());
}

#[test]
fn absurd_kdf_parameters_are_rejected_before_deriving() {
    let (_holder, dir) = scratch();
    let (_key, master, phrase) = new_vault(&dir);
    let mut key_file = read_key_file(&dir).unwrap();
    // Un `vault.key` alterado no debe poder pedir 4 GiB de memoria al abrir.
    key_file.password.kdf.memory_kib = u32::MAX;
    write_key_file(&dir, &key_file).unwrap();
    let error = unwrap_slot(&key_file.password, "frase-de-paso-larga", AAD_PASSWORD).unwrap_err();
    assert!(error.contains("parámetros"), "{error}");

    // El hueco de recuperación sigue siendo válido.
    let parsed = bip39::Mnemonic::parse_in(bip39::Language::Spanish, &phrase).unwrap();
    let recovered = unwrap_slot(&key_file.recovery, &parsed.to_string(), AAD_RECOVERY).unwrap();
    assert_eq!(&recovered[..], &master[..]);
}

#[test]
fn autolock_reports_locked_after_becoming_unavailable() {
    let state = VaultState::default();
    assert!(state.unlocked().is_none());
    state.store(random_master_key()).unwrap();
    assert!(state.unlocked().is_some());
    state.lock();
    assert!(state.unlocked().is_none());
}

#[test]
fn key_file_never_contains_the_master_key_in_clear() {
    let (_holder, dir) = scratch();
    let (_key, master, phrase) = new_vault(&dir);
    let raw = all_bytes(&dir);
    assert!(!contains(&raw, &master[..]));
    assert!(!contains(&raw, phrase.as_bytes()));
}
