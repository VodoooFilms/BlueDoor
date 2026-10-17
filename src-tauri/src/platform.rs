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
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        vec![PathBuf::from("/usr/share/fonts/truetype/msttcorefonts")]
    }
}

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

pub(crate) fn load_owned_backends() -> Result<(), String> {
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
            #[cfg(target_os = "windows")]
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
