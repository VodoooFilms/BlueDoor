> Actualización BLegal — 30 Sep 2026: Private AI vuelve a incorporar Qwen exclusivamente como extractor local de entidades. La arquitectura vigente de este flujo está en [PRIVATE_AI.md](PRIVATE_AI.md). `correct_transcript` sigue eliminado. El resto de este documento es contexto histórico del fork Voice to PDF y debe leerse como tal.

> **Estado vigente — 6 Oct 2026 (corte `main`, commit `af0b894` + limpieza):** el producto actual es Tauri 2 + Svelte 5 con dos flujos principales además de la transcripción por audio: **Private Legal** (`privacy.rs`: sesión privada, análisis con Qwen opcional, mapa de alias y puerta de exportación que falla cerrado) y **layout conservable** (`layout.rs` + complemento Swift `ocr/belegal-layout.swift`: vista previa y exportación con redacción de píxeles, borrado de QR y verificación TOCTOU antes de escribir). La importación de documentos privados se hace únicamente vía `import_private_document` con drop zone nativo (drag & drop y diálogo; validaciones de tipo/tamaño/zip-bomb en Rust); el comando IPC `paste_private_document` fue **eliminado** por estar muerto (sin llamadores en la UI). Mapa de componentes vigente en [docs/CODEMAP.md](docs/CODEMAP.md). Lo que sigue debajo describe el fork original (25 Sep 2026) y solo es válido para las rutas de audio/Whisper/historial/PDF que no han cambiado.

# Belegal _ Local demo — estado y arquitectura para un agente

**Corte:** 25 de septiembre de 2026. **Rama:** `feature/osx-nemotron-share`, integrada en `main`. Este reporte describe el código de esta rama, no necesariamente el instalador comercial publicado. Consultar `git status` y `git log -1` antes de actuar.

## Resumen operativo

Aplicación de escritorio Tauri 2 con UI Svelte 5/TypeScript y backend Rust. Flujo implementado: grabar audio PCM o importar un archivo → transcribir localmente con Whisper → editar y dar formato al texto → guardar historial local → exportar PDF. Interfaz EN/ES. La versión del código es **0.2.0** (`package.json`, `Cargo.toml`, `tauri.conf.json`).

**Cambio respecto a `AGENTS.md` (14 Sep):** esta rama ya **no contiene Qwen, `llama-cpp-2`, `llm.rs` ni el comando `correct_transcript`**. La edición es manual. Se añadió una prueba opcional de identificación de hablantes con Nemotron 3. El instalador Windows ARM64 **0.1.0** entregado comercialmente precede estos cambios. `AGENTS.md`, `MACOS_CLEAN_TEST.md` y partes de los documentos de portabilidad aún describen Qwen y rutas/versiones antiguas; no usarlos como fuente de verdad de la arquitectura actual.

## Mapa de componentes

| Capa | Archivos | Responsabilidad |
| --- | --- | --- |
| UI y orquestación | `src/App.svelte` | Navegación, hidratación y persistencia del historial, importación, selección de transcripción, tema. |
| Grabación | `src/components/Recorder.svelte`, `src/lib/pcm-recorder.ts`, `public/pcm-worklet.js` | Micrófono mediante AudioWorklet, WAV PCM, envío a `transcribe_blob`. |
| Editor/PDF | `src/components/TranscriptView.svelte` | Texto rico, guardar edición, deshacer/restablecer, diálogo de exportación. |
| Modelos | `src/components/ModelsView.svelte`, `src/lib/models-store.svelte.ts` | Selección Whisper en `localStorage`, descargas/borrado, estado opcional de Nemotron. |
| Contratos UI | `src/lib/types.ts`, `src/lib/i18n*.ts` | Tipos, catálogo visible de Whisper e idioma. Mantener sincronizado el catálogo con Rust. |
| Entrada IPC | `src-tauri/src/lib.rs` | Estado global, setup y registro de comandos Tauri. |
| Audio/Whisper | `src-tauri/src/audio.rs`, `src-tauri/src/types.rs` | Decodificación, remuestreo mono 16 kHz, inferencia, segmentos, caché del `WhisperContext`. |
| Diarización experimental | `src-tauri/src/diarization.rs` | Ejecutable externo NeMo-Speech.cpp configurado localmente; asigna hablantes a intervalos. |
| Modelos | `src-tauri/src/models.rs`, `src-tauri/model-manifest.json` | Catálogo, descarga HTTPS, tamaño y SHA-256, progreso, cancelación, borrado. |
| Datos/PDF/SO | `src-tauri/src/store.rs`, `pdf.rs`, `paths.rs`, `platform.rs`, `diagnostics.rs` | Historial JSON, escritura atómica, PDF A4, rutas y decoder incluido, comprobación de instalación. |

## Flujos y contratos importantes

1. **Entrada de audio:** la grabación genera WAV PCM y `transcribe_blob` lo lee en memoria. `transcribe_file` acepta una ruta elegida por el diálogo nativo; WAV compatible se lee directamente y otros formatos se convierten con el FFmpeg incluido en `Resources/runtime/` a un temporal privado. Ambos comandos ejecutan el trabajo pesado con `spawn_blocking`.
2. **Transcripción:** Whisper usa idioma automático (`set_language(None)`), hilos automáticos cuando `threads=0` (2–16) y devuelve `TranscriptionResult { id, text, segments, language, duration }`. `Segment` tiene `start`, `end`, `text` y `speaker?: u8`. `AppState` cachea el contexto Whisper por ruta; verifica SHA-256 antes de cargar un modelo no cacheado.
3. **Diarización:** es **opt-in**. `audio.rs` activa la ruta de hablantes solo si llegan `config.diarize == true` desde la UI **y** `diarization_available(...)` es verdadero; tener Nemotron instalado no la activa por sí solo. La disponibilidad exige modelo Nemotron instalado y ejecutable `BELEGAL_LOCAL_DEMO_NEMO_BIN` (o archivo de configuración de la app de prueba solo en debug). Se genera WAV temporal, se invoca `nemo_bin diarize`, se leen intervalos JSON y Whisper transcribe cada intervalo. El binario NeMo **no está integrado en el bundle de producción**; tratar esta capacidad como prototipo local, no como característica lista para entregar. El ID de bundle de prueba es `com.belegal.localdemo.diarizationtest` en `src-tauri/tauri.diarization.conf.json`.
4. **Prefijos de hablante:** `format_speaker_turns` en `audio.rs` escribe `Hablante N: ` **solo cuando el hablante cambia**, no en cada turno. En el editor, `normalizeSpeakerText` / `normalizeSpeakerHtml` reconstruyen los prefijos (reconocen `Hablante N:`, `Speaker N:` y nombres personalizados previos) para que el encabezado aparezca una vez por intervención.
5. **Nombres de hablante:** `speakerNames: Record<string, string>` (clave = id del hablante) viaja en `Transcript`, `StoredTranscript` y `speaker_names` en Rust (`#[serde(default)]`). La barra de hablantes de `TranscriptView` escribe un nombre por voz detectada y este se aplica de inmediato al editor, se persiste por transcripción y se reutiliza en el PDF.
6. **Historial:** `App.svelte` convierte entre tipos UI y `StoredTranscript`, hidrata con `load_transcripts` y serializa las escrituras IPC en una cola. Rust guarda `transcripts.json` en `app_data_dir` mediante temporal y rename atómico; un JSON corrupto no se sobrescribe. `corrected` guarda el texto plano editado y `correctedHtml` el contenido rico; los segmentos originales se conservan. El mutex de `store.rs` protege solo procesos individuales: dos instancias de la app pueden competir.
7. **PDF:** `generate_pdf` acepta `speakerNames` y crea A4 con metadatos, encabezado/paginación y cuerpo desde HTML rico cuando existe; si no, usa texto plano y finalmente segmentos con timestamps, mostrando el prefijo de hablante (con el nombre real si existe) solo al cambiar de voz. Guarda mediante la misma escritura atómica. `genpdf` no representa el subrayado del editor.
8. **Red:** el procesamiento de audio, historial y PDF es local. La red se usa para descargar pesos desde URLs fijadas en `model-manifest.json`. `models.rs` comprueba espacio, usa un archivo parcial exclusivo, compara tamaño/hash y publica con rename; `model-download-progress` informa el avance. No modificar hashes o URLs sin verificar su procedencia.

## IPC registrado en `lib.rs`

`check_installation`; `diarization_available`; `get_model_path`, `check_model_exists`, `list_models`, `download_model`, `cancel_model_download`, `delete_model`; `download_diarization_model`, `delete_diarization_model`; `transcribe_file`, `transcribe_blob`; `generate_pdf`; `load_transcripts`, `save_transcripts`.

`list_models` devuelve `{ whisper: ModelEntry[], diarization: ModelEntry[] }`. El catálogo Rust tiene ocho variantes Whisper (tiny, base, small, medium, large-v3-turbo, large-v3, large-v3-q5, latam-turbo-q5) y Nemotron 3 opcional. La UI omite tiny/base de sus opciones recomendadas. Revisar los nombres camelCase de `WhisperConfig`/`StoredTranscript` al modificar llamadas `invoke`.

## Datos, límites de plataforma y distribución

- Datos y modelos: `app_data_dir/transcripts.json` y `app_data_dir/models/`. En macOS el identificador principal es `com.belegal.localdemo`; en Windows se usa el directorio de datos de Tauri. Preferencias de tema, idioma, modelo Whisper y última edición viven en `localStorage`.
- Tauri configura CSP y permisos de diálogo/sistema de archivos en `src-tauri/tauri.conf.json` y `capabilities/default.json`. El decoder se busca dentro del bundle, no en `PATH`.
- Scripts de empaquetado: `pnpm release:mac`, `pnpm release:mac:dev`, `pnpm release:windows:arm64`, `pnpm release:windows:x64`. `pnpm prepare:runtime` prepara recursos ignorados por Git. Verificadores: `scripts/verify-bundle.py` y `scripts/verify-windows-bundle.py`.
- **Estado comercial:** `docs/archive/DISTRIBUTION.md` describe entrega privada del instalador Windows ARM64 0.1.0 mediante Prompthea/PayPal. Esa documentación no prueba que la rama 0.2.0 esté publicada. Notarización y prueba en Mac limpio, Windows x64 y firma Authenticode siguen indicadas como pendientes en la documentación existente. `docs/archive/FIRST_SALE_STATUS.md` es histórico y contradice el mecanismo de entrega más reciente de `docs/archive/DISTRIBUTION.md`.

## Verificación de este corte y próximos cuidados

- `pnpm typecheck`: **0 errores, 0 advertencias**.
- `cargo check --manifest-path src-tauri/Cargo.toml`: **correcto** en macOS. Esto verifica compilación de desarrollo; no valida un bundle, inferencia real, ejecutable Nemotron ni la ruta comercial.
- Antes de trabajo de release, actualizar los documentos que aún prometen Qwen o citan artefactos `0.1.0`, y verificar el paquete final con los scripts de plataforma. No inferir disponibilidad de Nemotron por el mero hecho de que su peso esté descargado.
- Para cambios del historial, conservar compatibilidad de campos opcionales y la regla de no sobrescribir datos corruptos. Para cambios de modelos, sincronizar catálogo Rust/UI y manifiesto inmutable.
