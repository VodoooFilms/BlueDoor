//! Small, explicit boundary for OS-owned runtime details.
//!
//! Product code deliberately stays platform-neutral: Tauri owns app-data paths,
//! WebView owns microphone consent, and Rust owns local files. This module only
//! contains differences that cannot be represented by those cross-platform APIs.

use std::path::{Path, PathBuf};
use tauri::Manager;

/// Navigation only: the renderer supplies no document, path, or URL parameters.
#[tauri::command]
pub(crate) fn open_chatgpt_home(app_handle: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app_handle
        .opener()
        .open_url("https://chatgpt.com/", None::<&str>)
        .map_err(|_| "No se pudo abrir el navegador. Vuelve a intentarlo.".to_string())
}

/// Directorio de modelos para tests/sandbox: en macOS se mantiene la ruta
/// legada de App Support usada por los scripts de verificación; en Windows se
/// usa `%APPDATA%/com.bluedoor.desktop/models` (el app_data_dir real).
#[cfg(test)]
pub(crate) fn test_models_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(format!(
            "{}/Library/Application Support/com.bluedoor.desktop/models",
            home
        ))
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(format!("{}/com.bluedoor.desktop/models", base))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        PathBuf::from(".")
    }
}

/// Fixture de voz para los tests de transcripción: en macOS se usa `/tmp` (la
/// ruta legada que usan los scripts de verificación); en Windows `%TEMP%`.
#[cfg(test)]
pub(crate) fn test_audio_fixture() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/tmp/vtp-es.wav")
    }
    #[cfg(target_os = "windows")]
    {
        let tmp = std::env::var("TEMP").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(format!("{}/vtp-es.wav", tmp))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        PathBuf::from("/tmp/vtp-es.wav")
    }
}

pub(crate) fn decoder_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

pub(crate) fn decoder_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .resource_dir()
        .map(|dir| dir.join("runtime").join(decoder_file_name()))
        .map_err(|_| {
            "No se pudo encontrar un componente de la aplicación. Reinstala BlueDoor.".to_string()
        })
}

pub(crate) fn font_directories() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![PathBuf::from("/System/Library/Fonts/Supplemental")]
    }
    #[cfg(target_os = "windows")]
    {
        let system_root = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\\Windows"));
        vec![system_root.join("Fonts")]
    }
    #[cfg(target_os = "android")]
    {
        // Android only ships Roboto and not every weight. PDF export uses the
        // font embedded in the application (see `bundled_font`).
        vec![PathBuf::from("/system/fonts")]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "android")))]
    {
        vec![PathBuf::from("/usr/share/fonts/truetype/msttcorefonts")]
    }
}

/// Liberation Sans is metrically compatible with Arial, covers Spanish
/// accents and is licensed under the SIL Open Font License. Bundling the four
/// faces keeps PDF export identical and offline on Android, where no Arial or
/// Times New Roman exists.
#[cfg(target_os = "android")]
pub(crate) fn bundled_font(weight: usize) -> Option<&'static [u8]> {
    Some(match weight {
        0 => include_bytes!("../fonts/LiberationSans-Regular.ttf"),
        1 => include_bytes!("../fonts/LiberationSans-Bold.ttf"),
        2 => include_bytes!("../fonts/LiberationSans-Italic.ttf"),
        3 => include_bytes!("../fonts/LiberationSans-BoldItalic.ttf"),
        _ => return None,
    })
}

/// Identifies the running platform for the shared frontend bundle.
#[tauri::command]
pub(crate) fn app_platform() -> &'static str {
    std::env::consts::OS
}

/// Android export: the app writes the reviewed PDF into its private cache and
/// hands it to the system share sheet. Desktop keeps the explicit save dialog.
#[tauri::command]
pub(crate) fn prepare_share_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::types::AppState>,
    default_name: String,
) -> Result<String, String> {
    let directory = app
        .path()
        .app_cache_dir()
        .map_err(|_| "No se pudo preparar la carpeta de exportación.")?
        .join("exports");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "No se pudo preparar la carpeta de exportación.")?;
    // Export names cannot smuggle source names/identifiers into a share URI.
    let _ = default_name;
    let name = format!("documento-protegido-{}.pdf", uuid::Uuid::new_v4());
    let path = state.authorize_pdf_path(&directory.join(name))?;
    Ok(path.to_string_lossy().to_string())
}

/// Opens the Android share sheet for an already authorized export.
#[tauri::command]
pub(crate) fn share_export(path: String, app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let candidate = Path::new(&path)
            .canonicalize()
            .map_err(|_| "El PDF no existe.")?;
        let directory = app
            .path()
            .app_cache_dir()
            .map_err(|_| "Caché no disponible.")?
            .join("exports")
            .canonicalize()
            .map_err(|_| "No hay exportaciones.")?;
        if candidate.parent() != Some(directory.as_path())
            || candidate.extension().and_then(|s| s.to_str()) != Some("pdf")
        {
            return Err("Solo se pueden compartir los PDF exportados por BlueDoor.".into());
        }
        crate::android::share_file(&candidate)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (path, app);
        Err("Compartir con otras aplicaciones está disponible en Android.".into())
    }
}

#[cfg(not(target_os = "android"))]
fn owned_backends_dir() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let executable_dir = executable
        .parent()
        .ok_or("No se pudo localizar la aplicación")?;
    #[cfg(target_os = "macos")]
    let directory = executable_dir.join("../Frameworks");
    #[cfg(target_os = "windows")]
    let directory = executable_dir.to_path_buf();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let directory = executable_dir.to_path_buf();

    #[cfg(any(test, debug_assertions))]
    if !directory.is_dir() {
        return llama_cpp_2::llama_backend::BACKENDS_DIR
            .map(PathBuf::from)
            .ok_or("Falta el backend de desarrollo".to_string());
    }
    Ok(directory)
}

#[cfg(all(
    not(target_os = "android"),
    not(all(target_os = "windows", target_arch = "x86_64"))
))]
fn load_one_backend(file: &Path) -> Result<(), String> {
    let path = std::ffi::CString::new(file.to_string_lossy().as_bytes())
        .map_err(|_| "Ruta de aplicación no válida")?;
    #[cfg(target_os = "macos")]
    let backend = unsafe {
        // transcribe-cpp also statically links GGML. A direct sys call can
        // resolve to its private registry instead of the one used by libllama.
        // Resolve the loader from Qwen's own dylib so both share a registry.
        unsafe extern "C" {
            fn dlopen(
                path: *const std::ffi::c_char,
                mode: std::ffi::c_int,
            ) -> *mut std::ffi::c_void;
            fn dlsym(
                handle: *mut std::ffi::c_void,
                symbol: *const std::ffi::c_char,
            ) -> *mut std::ffi::c_void;
        }
        let directory = file.parent().ok_or("Falta el directorio del backend")?;
        let bundled = directory.join("libggml.0.dylib");
        let library = if bundled.is_file() {
            bundled
        } else {
            directory.join("../lib/libggml.0.dylib")
        };
        let library = std::ffi::CString::new(library.to_string_lossy().as_bytes())
            .map_err(|_| "Ruta de aplicación no válida")?;
        // Keep the handle alive for the lifetime of the loaded model/backends.
        let handle = dlopen(library.as_ptr(), 2 /* RTLD_NOW */);
        if handle.is_null() {
            return Err("No se pudo abrir el motor de IA local.".into());
        }
        let symbol = dlsym(handle, c"ggml_backend_load".as_ptr());
        if symbol.is_null() {
            return Err("No se pudo inicializar el motor de IA local.".into());
        }
        let loader: unsafe extern "C" fn(*const std::ffi::c_char) -> *mut std::ffi::c_void =
            std::mem::transmute(symbol);
        loader(path.as_ptr())
    };
    #[cfg(not(target_os = "macos"))]
    let backend = unsafe { llama_cpp_sys_2::ggml_backend_load(path.as_ptr()) };
    if backend.is_null() {
        return Err("No se pudo cargar un componente de IA. Reinstala BlueDoor.".into());
    }
    Ok(())
}

/// Score only application-owned CPU modules. Do not use load_all_from_path:
/// upstream also consults GGML_BACKEND_PATH, even with an explicit directory.
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn load_x64_cpu(directory: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(
            name: *const u16,
            file: *mut std::ffi::c_void,
            flags: u32,
        ) -> *mut std::ffi::c_void;
        fn GetProcAddress(module: *mut std::ffi::c_void, name: *const u8) -> *mut std::ffi::c_void;
        fn FreeLibrary(module: *mut std::ffi::c_void) -> i32;
    }
    let mut best = None;
    let mut best_score = 0;
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if !name.starts_with("ggml-cpu-") || path.extension().is_none_or(|e| e != "dll") {
            continue;
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            // DLL's own directory + system/application dirs; exclude CWD/PATH.
            let module = LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0x1100);
            if module.is_null() {
                continue;
            }
            let symbol = GetProcAddress(module, c"ggml_backend_score".as_ptr().cast());
            if !symbol.is_null() {
                let score: unsafe extern "C" fn() -> i32 = std::mem::transmute(symbol);
                let score = score();
                if score > best_score {
                    best_score = score;
                    best = Some(path);
                }
            }
            FreeLibrary(module);
        }
    }
    let path = best.ok_or("Falta un backend de CPU compatible. Reinstala BlueDoor.")?;
    let path = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| "Ruta de aplicación no válida")?;
    if unsafe { llama_cpp_sys_2::ggml_backend_load(path.as_ptr()) }.is_null() {
        return Err("No se pudo cargar el backend de CPU de BlueDoor.".into());
    }
    Ok(())
}

pub(crate) fn load_owned_backends() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        // llama.cpp and its CPU backend are linked statically into the
        // application library; the registry is populated at process start.
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        static LOADED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
        LOADED
            .get_or_init(|| {
                let directory = owned_backends_dir()?;
            #[cfg(target_os = "macos")]
            {
                // Apple Silicon CPU baseline plus Metal, both bundled by the
                // macOS release script in Contents/Frameworks.
                for name in ["libggml-cpu-apple_m1.so", "libggml-metal.so"] {
                    let file = directory.join(name);
                    if !file.is_file() {
                        return Err("Falta un componente de IA de la aplicación. Reinstala BlueDoor; tus documentos se conservan.".into());
                    }
                    load_one_backend(&file)?;
                }
            }
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            load_x64_cpu(&directory)?;
            #[cfg(all(target_os = "windows", not(target_arch = "x86_64")))]
            {
                // CMake emits a CPU backend DLL on Windows. Names vary between
                // llama.cpp revisions (with/without a lib prefix), so discover
                // the CPU module rather than hard-coding a release-only name.
                let cpu = std::fs::read_dir(&directory)
                    .map_err(|_| "Falta un componente de IA de la aplicación. Reinstala BlueDoor; tus documentos se conservan.".to_string())?
                    .flatten()
                    .map(|entry| entry.path())
                    .find(|path| {
                        path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
                            && path.file_name().is_some_and(|name| name.to_string_lossy().contains("ggml-cpu"))
                    })
                    .ok_or("Falta el backend de CPU de la aplicación. Reinstala BlueDoor; tus documentos se conservan.")?;
                load_one_backend(&cpu)?;
            }
            Ok(())
        })
        .clone()
    }
}

/// System picker input with an Android cache copy removed on every exit path.
pub(crate) struct PickedInput {
    path: PathBuf,
    temporary: bool,
}
impl std::ops::Deref for PickedInput {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}
impl Drop for PickedInput {
    fn drop(&mut self) {
        if self.temporary {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
pub(crate) fn picked_input(picked: tauri_plugin_dialog::FilePath) -> Result<PickedInput, String> {
    #[cfg(target_os = "android")]
    if let tauri_plugin_dialog::FilePath::Url(uri) = &picked {
        return Ok(PickedInput {
            path: crate::android::materialize_vault_uri(uri.as_str())?,
            temporary: true,
        });
    }
    let path = picked
        .into_path()
        .map_err(|_| "El archivo no es una ruta local válida.")?;
    Ok(PickedInput {
        path,
        temporary: false,
    })
}

/// SAF output is staged privately and copied only to the URI chosen by Android.
pub(crate) struct PickedOutput {
    path: PathBuf,
    uri: Option<String>,
    _temp: Option<tempfile::TempDir>,
}
impl std::ops::Deref for PickedOutput {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}
impl PickedOutput {
    pub(crate) fn new(
        app: &tauri::AppHandle,
        picked: tauri_plugin_dialog::FilePath,
    ) -> Result<Self, String> {
        #[cfg(not(target_os = "android"))]
        let _ = app;
        #[cfg(target_os = "android")]
        if let tauri_plugin_dialog::FilePath::Url(uri) = &picked {
            let cache = app
                .path()
                .app_cache_dir()
                .map_err(|_| "Caché no disponible.")?;
            let temp =
                tempfile::tempdir_in(cache).map_err(|_| "No se pudo preparar la exportación.")?;
            return Ok(Self {
                path: temp.path().join("vault-export"),
                uri: Some(uri.to_string()),
                _temp: Some(temp),
            });
        }
        let path = picked
            .into_path()
            .map_err(|_| "El destino no es una ruta válida.")?;
        Ok(Self {
            path,
            uri: None,
            _temp: None,
        })
    }
    pub(crate) fn finish(&self) -> Result<String, String> {
        #[cfg(target_os = "android")]
        if let Some(uri) = &self.uri {
            crate::android::write_document(&self.path, uri)?;
            return Ok(uri.clone());
        }
        let _ = &self.uri;
        Ok(self.path.to_string_lossy().to_string())
    }
}
