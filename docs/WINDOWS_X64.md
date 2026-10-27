# BlueDoor Windows x64 CPU

Estado al 26 de octubre de 2026: implementación preparada localmente; **todavía no hay instalador Windows compilado ni ejecución de Actions verificada**. El usuario autorizó subir una rama y ejecutar Actions; no se creará una release ni se fusionará la PR automáticamente. El remoto existente pertenece a `VodoooFilms/BlueDoor`; no se modificó su configuración.

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

El código y las dependencias están fijados por lockfiles/checksums; la imagen mantenida por GitHub, las acciones y sus herramientas pueden actualizarse. El reporte registra versiones y host; no se promete identidad binaria entre runners de fechas distintas. Primera ejecución y depuración de Windows aún pendientes.

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
| YAML y configuración generada Tauri | Parseo YAML y validación con el esquema local de Tauri: PASS. PowerShell pendiente de ejecución Windows. |
| Nuevo flujo PDF con Qwen real Q4 | PASS: 3 entidades, 0 fragmentos fallidos; carga 0.83 s, detección 2.65 s, flujo 5.32 s, usando Metal en macOS. |
| Inspección independiente del PDF Q4 | PASS: texto, metadatos, objetos y streams sin los datos ficticios. |
| Q8 real y descarga limpia de ambos pesos | Pendiente del runner; no se descargaron nuevos pesos en el equipo del usuario. |
| Instalador/arranque/CPU/RAM Windows | Pendiente de Actions. No existe aún un `.exe` entregable verificado. |

La primera ejecución nativa local encontró cuatro fallos de coordenadas en pruebas macOS. La comparación aislada con HEAD pasó y, tras recompilar los helpers Swift, la suite completa pasó sin cambios en maquetación. La primera versión de la prueba nueva también detectó espacios no separables en la extracción del fixture; se corrigieron sus aserciones, sin cambiar el texto procesado ni el pipeline de producto.

Evidencias locales: `artifacts/windows-x64-local/` (ignorado por Git), incluyendo `native-suite.log`, `ui-suite.log`, `clippy.log`, `baseline-layout.log`, `qwen3-fast.log` y `qwen3-fast/{input.pdf,protected.pdf,workflow.json,independent-pdf.json}`.

## Pendiente en Windows físico

- Instalar y desinstalar con un usuario de oficina en Windows 10 y Windows 11, Intel y AMD; probar rutas y nombres con tildes, usuario sin permisos administrativos y equipo sin Visual Studio.
- Verificar la interfaz WebView2, diálogos, importación autorizada, descarga/cancelación/reanudación desde la aplicación, revisión humana y exportación completa. Probar micrófono y los motores de audio existentes.
- Medir documentos representativos y ambas cuantizaciones con CPUs/RAM de oficina, presión de memoria y tiempos sostenidos. **No se afirma compatibilidad con 4 GB.** El runner Windows Server no sustituye estas pruebas ni garantiza el rendimiento de Intel y AMD físicos.
- OCR de PDFs escaneados, detector complementario de nombres y conservación de páginas originales siguen dependiendo de helpers macOS ya existentes. No se portaron ni se eliminó su barrera de error. Windows mantiene su soporte previo de PDF con texto/TXT/DOCX y exportación reconstruida; no se promete paridad de estas funciones de macOS.
- No hay firma Authenticode configurada. Falta observar la experiencia de instalación de este build en un equipo limpio.
