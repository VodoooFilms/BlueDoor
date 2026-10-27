# BlueDoor Windows x64 CPU

Estado al 27 de octubre de 2026: rama y PR publicadas con autorización; **instalador Windows x64 generado y aprobado por todos los gates de Actions**, incluida instalación, arranque, inferencia real Q4/Q8, privacidad del PDF y suite UI Chromium. Código verificado: `a13f902befd3808d7aaf6ace13fa8f56da14bf75`, basado en `ea9a9e5` de GitHub y sin el commit local `939ecaa` de la Bóveda. El usuario autorizó subir una rama y ejecutar Actions; no se creará una release ni se fusionará la PR automáticamente. El remoto existente pertenece a `VodoooFilms/BlueDoor`; no se modificó su configuración.

## Entrega comprobada

- [Instalador y SHA-256, artifact BlueDoor-Windows-x64-CPU-4](https://github.com/VodoooFilms/BlueDoor/actions/runs/40496004892/artifacts/12731830253).
- [Reporte y evidencias sintéticas, artifact BlueDoor-Windows-x64-report-4](https://github.com/VodoooFilms/BlueDoor/actions/runs/40496004892/artifacts/12731915222).
- [Workflow Windows aprobado](https://github.com/VodoooFilms/BlueDoor/actions/runs/40496004892) y [CI general aprobado](https://github.com/VodoooFilms/BlueDoor/actions/runs/40496004861).
- Archivo: `BlueDoor_0.4.0_x64-setup.exe`, **23,617,351 bytes** (~23.6 MB), sin pesos de modelos.
- SHA-256 del `.exe`: `1d5e0c06652c7af6d8973549b3bdd0284e67c87ea419e06b6709c4c1e581a040`. Se verificó también después de descargarlo al equipo local.
- Copia local: `/Users/antoin/Documents/BlueDoor/installers/windows-x64-actions-40496004892/`, con `.exe`, `.sha256`, este reporte y evidencias. Los artifacts de Actions caducan tras 14 días; la copia local se conserva.

## Infraestructura reutilizada

| Componente | Situación encontrada y adaptación |
| --- | --- |
| Svelte 5 + Tauri 2 | Frontend compartido; sin cambios en interfaz ni comandos de producto. |
| Release Windows | `release-windows.ps1` ya aceptaba `x64` y el target `x86_64-pc-windows-msvc`; se completa su empaquetado. ARM64 conserva su ruta NSIS/MSI. |
| Audio nativo | FFmpeg ya soportaba x64/ARM64 desde la misma fuente y checksum fijados. Sigue limitado a archivos, sin red. |
| llama.cpp | Versiones Rust existentes, CPU con backends dinámicos fuera de macOS. Se empaquetan sus DLL de esta compilación y se elige la variante de CPU compatible con mayor puntuación. Incluye el baseline `ggml-cpu-x64.dll`; no se exige AVX2 ni GPU dedicada. Rendimiento en CPUs antiguas pendiente. |
| transcribe.cpp | Se conserva el crate existente. Solo en x64 se activa su opción `TRANSCRIBE_X86_CONSERVATIVE`, evitando fijar instrucciones del equipo que compila en el motor estático. |
| Runtime Windows | Se incluyen las DLL redistribuibles x64 de Visual C++ al lado del ejecutable. El instalador incluye el bootstrapper de WebView2; puede necesitar Internet para instalar WebView2 si falta. |
| Modelos | Se mantienen catálogo, URLs, tamaños y hashes, sin pesos en recursos/instalador/artifacts. **El manifest actual registra Qwen3 1.7B Q4 y Q8; no contiene un peso Qwen3 4B.** No se añadió ni sustituyó ningún modelo. |
| Seguridad | Mismos límites de importación, revisión y autorización de exportación. La selección de CPU solo examina DLL propias; no invoca el buscador global que consulta `GGML_BACKEND_PATH`. |
| Datos | Mismo identificador BlueDoor y almacenamiento propio; sin migraciones nuevas. |

La documentación oficial describe el [empaquetado NSIS de Tauri](https://v2.tauri.app/distribute/windows-installer/). El workflow usa la [ubicación expuesta por setup-msys2](https://github.com/msys2/setup-msys2#outputs), sin suponer la ruta de instalación del runner.

## Ejecución de GitHub Actions

1. Subir estos cambios a una rama de **BlueDoor** (autorizado por el usuario). `AGENTS.md` exige instrucción del usuario antes de publicar; no se deben subir cambios a Belegal o Local Legal.
2. Abrir una PR hacia la rama principal para disparar `.github/workflows/windows-x64.yml`. Alternativamente, una vez que el workflow esté en la rama por defecto, usar **Actions → Windows x64 CPU installer → Run workflow** y elegir la rama que se desea compilar.
3. El job usa `windows-2022`, Rust 1.98.1, Node 22, pnpm 12.3.4 y los lockfiles. Compila MSVC x64, FFmpeg y llama.cpp; genera NSIS y comprueba el payload instalado. No crea releases ni publica automáticamente.
4. Si pasan todos los gates, descargar `BlueDoor-Windows-x64-CPU-<run>`: contiene `*-setup.exe` y SHA-256. El reporte `BlueDoor-Windows-x64-report-<run>` se intenta guardar también ante errores. Retención: 14 días.

El código y las dependencias están fijados por lockfiles/checksums; la imagen mantenida por GitHub, las acciones y sus herramientas pueden actualizarse. El reporte registra versiones y host; no se promete identidad binaria entre runners de fechas distintas. La ejecución 4 completó todas las comprobaciones; los fallos de preparación encontrados y sus correcciones quedan registrados abajo.

Para reproducir en Windows desde un checkout limpio, usar el entorno de desarrollador Visual Studio x64, Rust 1.98.1 con target MSVC, LLVM, Python, Node/pnpm y MSYS2 con make/nasm/diffutils/pkgconf. Ejecutar `pnpm install --frozen-lockfile`, `pnpm typecheck` y `pnpm release:windows:x64`. El workflow muestra la instalación exacta. No reutilizar un directorio de recursos con ejecutables de macOS: el gate lo rechaza.

## Gates automáticos

- `typecheck`, build y formato; siete pruebas portables del empaquetado: runtime completo, arquitectura incorrecta, GPU inesperada, baseline ausente, compilaciones ambiguas, pesos accidentales y ejecutables macOS.
- Verificar binarios PE x64, dependencias propias y ausencia de pesos; instalar el `.exe` silenciosamente en una ruta con espacios.
- Inicializar las DLL instaladas de CPU/llama y ejecutar el FFmpeg instalado. Lanzar la aplicación instalada con PATH reducido a Windows, exigir ventana nativa que responde y diez segundos de estabilidad. Esto es una comprobación de arranque, no una navegación integral por WebView2.
- Suite Rust de release y suites UI existentes con IPC simulado, claramente separadas de la prueba nativa.
- Descargar **de nuevo** Q4 y Q8 desde las URLs existentes fuera de recursos, verificar tamaño/SHA-256 y cargar cada peso con el motor real. La descarga de CI usa HTTPS de Python; **todavía no es una prueba del downloader de la aplicación desde la interfaz real en Windows**. La aplicación conserva su downloader Rust sin cambios; sus controles de integridad y la UI de descarga tienen pruebas existentes.
- Por cada cuantización: generar PDF con nombre/email/teléfono ficticios, extraerlo con el importador real, Detect mediante Qwen, Protect y autorización de Export con las funciones de producción. Comprobar rechazo antes de revisión y rechazo del original; confirmar mediante un revisor sintético y escribir con el generador real. Los pequeños adaptadores añadidos solo se compilan con `cfg(test)`.
- Verificar el texto exportado con el importador y después con `pypdf`, revisando texto, metadatos, objetos y streams descomprimidos. Fallar si quedan los datos ficticios o falta el alias de persona. Los espacios no separables se tienen en cuenta.
- Guardar JSON con duración de descarga, carga, detección y flujo; host/CPU/RAM; salida/código de proceso, timeout y pico de RSS del árbol de procesos muestreado cada 200 ms. Es una medición de procesos, no un mínimo de RAM del sistema. El arranque registra además el working set del proceso. Los logs y PDFs son exclusivamente sintéticos.

Cada modelo tiene un límite de 20 minutos; la compilación de pruebas, 30 minutos; job completo, 100 minutos. Un fallo de descarga, inferencia, privacidad o tiempo bloquea el artifact de instalador aprobado y conserva los diagnósticos disponibles. Se prueban únicamente los dos pesos de privacidad registrados; no toda la colección de audio con inferencia real.

## Reporte local

Host: macOS Apple Silicon, Rust 1.98.1. Las pruebas locales iniciales se ejecutaron en `939ecaa`, que incluye trabajo local previo de la Bóveda. La rama de esta entrega aplica únicamente el diff x64 sobre `origin/main` (`ea9a9e5`), conservando aquel commit en su rama original sin publicarlo. Actions verifica este estado de entrega por separado. Estos resultados **no certifican Windows ni CPU x64**.

| Comprobación | Resultado |
| --- | --- |
| `pnpm typecheck` | PASS: 0 errores y 0 warnings. |
| `pnpm build` | PASS. |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | PASS. |
| `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS. |
| Suite nativa | PASS: 126 aprobadas, 16 omitidas explícitamente. |
| `pnpm test:ui` | PASS, incluida configuración de modelos y privacidad ES/EN. |
| `pnpm test:privacy:ui` | PASS ES/EN. |
| `python3 tests/check-windows-packaging.py` | PASS: 7 pruebas. |
| YAML y configuración generada Tauri | Parseo YAML y validación con el esquema local de Tauri: PASS. PowerShell ejecutado y aprobado en Windows. |
| Nuevo flujo PDF con Qwen real Q4 | PASS: 3 entidades, 0 fragmentos fallidos; carga 0.83 s, detección 2.65 s, flujo 5.32 s, usando Metal en macOS. |
| Inspección independiente del PDF Q4 | PASS: texto, metadatos, objetos y streams sin los datos ficticios. |
| Q8 real y descarga limpia de ambos pesos | PASS en Actions para Q4 y Q8, sin descargar nuevos pesos de modelos en el equipo del usuario. |
| Build/instalación/arranque Windows | PASS en la ejecución final: NSIS x64, DLL CPU propia, ventana que responde, estabilidad de diez segundos y todos los gates posteriores aprobados. |

La primera ejecución nativa local encontró cuatro fallos de coordenadas en pruebas macOS. La comparación aislada con HEAD pasó y, tras recompilar los helpers Swift, la suite completa pasó sin cambios en maquetación. La primera versión de la prueba nueva también detectó espacios no separables en la extracción del fixture; se corrigieron sus aserciones, sin cambiar el texto procesado ni el pipeline de producto.

Evidencias locales: `artifacts/windows-x64-local/` (ignorado por Git), incluyendo `native-suite.log`, `ui-suite.log`, `clippy.log`, `baseline-layout.log`, `qwen3-fast.log` y `qwen3-fast/{input.pdf,protected.pdf,workflow.json,independent-pdf.json}`.

## Ejecuciones Windows en GitHub

- [Ejecución 1](https://github.com/VodoooFilms/BlueDoor/actions/runs/40432242182): typecheck y pruebas de empaquetado aprobadas; FFmpeg compilado. Falló el enlace de Rust porque MSYS2 anteponía su `link.exe` al de MSVC. Se aisló el PATH de MSYS2 dentro del subprocess de FFmpeg.
- [Ejecución 2](https://github.com/VodoooFilms/BlueDoor/actions/runs/40489807252), commit `d116204`: instalador `BlueDoor_0.4.0_x64-setup.exe` compilado y validado. Instalación silenciosa, carga de DLL CPU, decoder y arranque de ventana aprobados. `startup.json` registra 15.97 s, cero errores y 23,449,600 bytes de pico del proceso principal (no incluye WebView2 ni modelos). Suite Rust: 111 aprobadas, 15 omitidas y una prueba fallida por usar `/tmp/...` como ruta absoluta. Se cambió únicamente el fixture a `std::env::temp_dir()`, conservando las aserciones de rechazo de rutas relativas.
- [Ejecución 3](https://github.com/VodoooFilms/BlueDoor/actions/runs/40492601482), commit `63d0c09`: finalizada. Añade caché de dependencias compiladas y recoge resultados de ambos modelos aunque falle una prueba independiente; cualquier fallo sigue bloqueando el artifact aprobado. Los pesos temporales están fuera del directorio cacheado. Build, instalación y arranque aprobados; suite Windows: **112 aprobadas, 15 omitidas, 0 fallos**. Q4 y Q8 descargados y verificados de nuevo; ambos completaron Detect → Protect → Export y la inspección independiente de objetos/streams del PDF, con tres entidades y cero fragmentos fallidos. La suite UI falló porque WebKit en Windows no proporciona `AudioContext` para la prueba de grabación. Se selecciona Chromium en Windows (motor afín a [WebView2](https://learn.microsoft.com/en-us/microsoft-edge/webview2/)) mediante un helper de pruebas; macOS/Linux conservan WebKit. La suite completa pasó localmente con Chromium antes de volver a ejecutar Actions.

- [Ejecución 4](https://github.com/VodoooFilms/BlueDoor/actions/runs/40496004892), commit `a13f902`: **SUCCESS**. Siete pruebas de empaquetado; NSIS, instalación y arranque; **112 pruebas Rust aprobadas y 15 omitidas explícitamente**; Q4/Q8 con descarga limpia, SHA-256, inferencia CPU y PDF revisado por dos parsers; suite completa de interfaz Chromium, incluidas privacidad ES/EN, configuración de modelos y captura PCM. Se generaron los artifacts del instalador y del reporte. El CI general del mismo commit también terminó en verde, incluyendo las pruebas macOS/WebKit.

### Medición real de CPU, ejecución final 4

VM Windows Server 2022, CPU AMD (AuthenticAMD Family 25 Model 17), cuatro procesadores lógicos y 17,174,360,064 bytes de RAM del host (~16 GiB). Son mediciones del fixture pequeño y del proceso nativo de QA; no incluyen el consumo total de Windows y WebView2 ni establecen un mínimo de RAM.

| Peso existente | Descarga verificada | Carga verificada | Detect | Flujo completo | Pico RSS muestreado | Resultado PDF |
| --- | --- | --- | --- | --- | --- | --- |
| Qwen3 1.7B Q4 | 29.53 s, 1,359,279,776 bytes | 2.01 s | 10.29 s | 12.56 s | 2,908,934,144 bytes (~2.71 GiB) | PASS, 3 entidades, 0 fugas del fixture |
| Qwen3 1.7B Q8 | 24.88 s, 2,220,088,992 bytes | 1.93 s | 15.59 s | 17.78 s | 2,954,027,008 bytes (~2.75 GiB) | PASS, 3 entidades, 0 fugas del fixture |

La carga incluye la verificación Rust del archivo. El tiempo del flujo excluye la descarga. Los reportes por modelo tienen `errors: []`, `failed_fragments: 0`, `export_verified: true` y `independent-pdf.json` aprobado. Artifact de evidencia: [BlueDoor-Windows-x64-report-4](https://github.com/VodoooFilms/BlueDoor/actions/runs/40496004892/artifacts/12731915222). La suite UI Windows también pasó; el artifact del instalador está disponible. El arranque final registró 16.78 s, ventana que responde, cero errores y 23,437,312 bytes de pico del proceso principal; no incluye WebView2 ni modelos. El módulo CPU elegido fue `ggml-cpu-icelake.dll`, nombre de la variante ISA, sobre un host AMD.

## Pendiente en Windows físico

- Instalar y desinstalar con un usuario de oficina en Windows 10 y Windows 11, Intel y AMD; probar rutas y nombres con tildes, usuario sin permisos administrativos y equipo sin Visual Studio.
- Verificar la interfaz WebView2, diálogos, importación autorizada, descarga/cancelación/reanudación desde la aplicación, revisión humana y exportación completa. Probar micrófono y los motores de audio existentes.
- Medir documentos representativos y ambas cuantizaciones con CPUs/RAM de oficina, presión de memoria y tiempos sostenidos. **No se afirma compatibilidad con 4 GB.** El runner Windows Server no sustituye estas pruebas ni garantiza el rendimiento de Intel y AMD físicos.
- OCR de PDFs escaneados, detector complementario de nombres y conservación de páginas originales siguen dependiendo de helpers macOS ya existentes. No se portaron ni se eliminó su barrera de error. Windows mantiene su soporte previo de PDF con texto/TXT/DOCX y exportación reconstruida; no se promete paridad de estas funciones de macOS.
- No hay firma Authenticode configurada. Falta observar la experiencia de instalación de este build en un equipo limpio.
