//! JNI bridge to the small Kotlin helper that owns Android-only facilities:
//! content-URI materialization, ML Kit OCR and the system share sheet.
//!
//! The helper never receives document text from the WebView: it is called from
//! the Rust import/export pipeline, and the model proposes entities only after
//! the extracted text has crossed the same native validation as on desktop.

use jni::objects::{GlobalRef, JClass, JObject, JString, JValue, JValueOwned};
use jni::sys::{jint, JNI_VERSION_1_6};
use jni::JavaVM;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static VM: OnceLock<JavaVM> = OnceLock::new();
static HELPER: OnceLock<GlobalRef> = OnceLock::new();

const HELPER_CLASS: &str = "com/bluedoor/desktop/BlueDoorAndroid";

/// Android calls this when the application library is loaded. Caching the VM
/// and a global reference to the helper class lets any Rust worker thread call
/// Kotlin without depending on the caller's class loader.
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: *mut jni::sys::JavaVM, _reserved: *mut c_void) -> jint {
    if vm.is_null() {
        return -1;
    }
    // Safety: the Android runtime passes a valid JavaVM pointer for the
    // lifetime of the process.
    let vm = match unsafe { JavaVM::from_raw(vm) } {
        Ok(vm) => vm,
        Err(_) => return -1,
    };
    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(_) => return -1,
    };
    if let Ok(class) = env.find_class(HELPER_CLASS) {
        if let Ok(global) = env.new_global_ref(class) {
            let _ = HELPER.set(global);
        }
    }
    drop(env);
    let _ = VM.set(vm);
    JNI_VERSION_1_6
}

fn call_string(method: &str, signature: &str, argument: &str) -> Result<String, String> {
    let vm = VM.get().ok_or("El puente Android no se ha inicializado.")?;
    let helper = HELPER
        .get()
        .ok_or("El componente Android de BlueDoor no está disponible.")?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "No se pudo preparar el componente Android.")?;
    let argument = env
        .new_string(argument)
        .map_err(|_| "No se pudo enviar la ruta al componente Android.")?;
    let result = env.call_static_method(
        <&JClass>::from(helper.as_obj()),
        method,
        signature,
        &[JValue::Object(&JObject::from(argument))],
    );
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return Err("El componente Android no pudo completar la operación.".into());
    }
    let value =
        result.map_err(|_| "El componente Android no pudo completar la operación.".to_string())?;
    match value {
        JValueOwned::Object(object) if !object.is_null() => env
            .get_string(&JString::from(object))
            .map(|text| text.into())
            .map_err(|_| "El componente Android devolvió un resultado no válido.".into()),
        JValueOwned::Void | JValueOwned::Object(_) => Ok(String::new()),
        _ => Ok(String::new()),
    }
}

/// Copies a `content://` document chosen in the system picker into the
/// application cache and returns a regular path Rust can read. The caller
/// removes the copy as soon as extraction finishes.
pub(crate) fn materialize_uri(uri: &str) -> Result<PathBuf, String> {
    let path = call_string(
        "copyUriToCache",
        "(Ljava/lang/String;)Ljava/lang/String;",
        uri,
    )?;
    if path.is_empty() {
        return Err("No se pudo abrir el documento seleccionado.".into());
    }
    Ok(PathBuf::from(path))
}

/// Runs ML Kit text recognition locally. The bundled Latin model needs no
/// download and works without a network connection.
pub(crate) fn ocr_text(path: &Path) -> Result<String, String> {
    let path = path.to_str().ok_or("La ruta del documento no es válida.")?;
    call_string("ocr", "(Ljava/lang/String;)Ljava/lang/String;", path)
}

/// Hands the exported PDF to the Android share sheet.
pub(crate) fn share_file(path: &Path) -> Result<(), String> {
    let path = path.to_str().ok_or("La ruta del documento no es válida.")?;
    call_string("sharePdf", "(Ljava/lang/String;)V", path).map(|_| ())
}

static FOREGROUND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
pub(crate) fn is_foreground() -> bool {
    FOREGROUND.load(std::sync::atomic::Ordering::SeqCst)
}

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
pub(crate) fn set_app(app: tauri::AppHandle) {
    let _ = APP.set(app);
}
#[no_mangle]
pub extern "system" fn Java_com_bluedoor_desktop_MainActivity_nativeBackground(
    _env: jni::JNIEnv,
    _class: JObject,
) {
    use tauri::{Emitter, Manager};
    FOREGROUND.store(false, std::sync::atomic::Ordering::SeqCst);
    if let Some(app) = APP.get() {
        app.state::<crate::vault::VaultState>().lock();
        let _ = app.emit("mobile-background", ());
    }
}

#[tauri::command]
pub(crate) fn preview_import_source() -> Result<(), String> {
    call_string("previewSource", "(Ljava/lang/String;)V", "").map(|_| ())
}

pub(crate) fn write_document(path: &Path, uri: &str) -> Result<(), String> {
    let payload =
        serde_json::json!({"path": path.to_str().ok_or("Ruta no válida")?, "uri": uri}).to_string();
    call_string("writeDocument", "(Ljava/lang/String;)V", &payload).map(|_| ())
}

pub(crate) fn remember_source(uri: &str) -> Result<(), String> {
    call_string("rememberSource", "(Ljava/lang/String;)V", uri).map(|_| ())
}

#[tauri::command]
pub(crate) fn cancel_private_import() -> Result<(), String> {
    call_string("cancelImport", "(Ljava/lang/String;)V", "").map(|_| ())
}

pub(crate) fn materialize_vault_uri(uri: &str) -> Result<PathBuf, String> {
    let path = call_string(
        "copyVaultUriToCache",
        "(Ljava/lang/String;)Ljava/lang/String;",
        uri,
    )?;
    if path.is_empty() {
        return Err("No se pudo abrir el archivo del Vault.".into());
    }
    Ok(PathBuf::from(path))
}

#[no_mangle]
pub extern "system" fn Java_com_bluedoor_desktop_MainActivity_nativeForeground(
    _env: jni::JNIEnv,
    _class: JObject,
) {
    FOREGROUND.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub(crate) fn analysis_active(active: bool) -> Result<(), String> {
    call_string(
        "setAnalysisActive",
        "(Ljava/lang/String;)V",
        if active { "true" } else { "false" },
    )
    .map(|_| ())
}
#[no_mangle]
pub extern "system" fn Java_com_bluedoor_desktop_MainActivity_nativeStopped(
    _env: jni::JNIEnv,
    _class: JObject,
) {
    crate::llm::interrupt_background_analysis();
}

#[tauri::command]
pub(crate) async fn private_source_page(page: u32) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = call_string(
            "sourcePage",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &serde_json::json!({ "page": page }).to_string(),
        )?;
        serde_json::from_str(&result).map_err(|_| "No se pudo mostrar la página original.".into())
    })
    .await
    .map_err(|_| "La vista original se interrumpió.")?
}

pub(crate) fn run_gliner(payload: &str) -> Result<String, String> {
    call_string(
        "runGliner",
        "(Ljava/lang/String;)Ljava/lang/String;",
        payload,
    )
}
pub(crate) fn unload_gliner() -> Result<(), String> {
    call_string("unloadGliner", "(Ljava/lang/String;)V", "").map(|_| ())
}
