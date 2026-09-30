# Mapa del código

## Interfaz

| Ruta | Responsabilidad |
| --- | --- |
| `src/App.svelte` | Navegación, sesión privada, historial y coordinación de vistas. |
| `src/components/PrivateAI.svelte` | Importación, análisis, revisión y mapa privado. |
| `src/components/ModelsView.svelte` | Catálogo, setup inicial, permisos y cola de descarga. |
| `src/components/Recorder.svelte` | Grabación e importación de audio. |
| `src/components/TranscriptView.svelte` | Estado, selección, formato y acciones del editor. |
| `src/lib/editor/content.ts` | Conversión texto/HTML, estructura de hablantes y cambios de nombres. No mantiene estado Svelte. |
| `src/lib/editor/export.ts` | Nombre seguro del PDF con fecha y hora local. |
| `src/lib/pcm-recorder.ts`, `public/pcm-worklet.js` | Captura PCM y generación WAV. |
| `src/lib/*store.svelte.ts` | Preferencias y estado compartido. |
| `src/lib/types.ts`, `privacy.ts` | Contratos de datos con la capa nativa. |
| `src/lib/i18n.ts`, `private-i18n.ts` | Copia de interfaz en español e inglés. |

## Capa nativa

`src-tauri/src/lib.rs` registra los comandos Tauri y el estado. La autoridad para revisar y exportar documentos privados permanece en Rust.

| Módulo | Responsabilidad |
| --- | --- |
| `privacy.rs` | Sesión privada, entidades, sanitización y puerta de exportación. |
| `documents.rs` | Extracción acotada de TXT, DOCX y PDF, y OCR local. |
| `llm.rs` | Inferencia Qwen y validación de su salida. |
| `audio.rs` | Decodificación, transcripción y segmentación. |
| `diarization.rs` | Integración experimental de identificación de hablantes. |
| `models.rs` | Catálogo, descarga e integridad de modelos. |
| `diagnostics.rs` | Comprobaciones de instalación y preparación inicial. |
| `pdf.rs` | Destino autorizado, validación de exportación y render PDF. |
| `pdf/html.rs` | Parser del subconjunto HTML del editor y texto visible usado para validar exportaciones. |
| `store.rs`, `types.rs`, `platform.rs`, `paths.rs` | Escritura atómica, estado, rutas y recursos de plataforma. |

Las pruebas extensas de audio, privacidad y PDF viven en `audio/tests.rs`, `privacy/tests.rs` y `pdf/tests.rs`, como módulos hijos. Mantienen acceso a los detalles internos sin convertirlos en API pública.

## Herramientas y datos

- `scripts/`: preparación de componentes, empaquetado y comprobaciones de bundles. Los comandos de uso diario se exponen desde `package.json`.
- `tests/`: harness del editor, mocks Tauri y pruebas WebKit. `run-ui.mjs` gestiona ambos servidores y libera los puertos al terminar.
- `tests/fixtures/`: documentos sintéticos que deben conservarse.
- `licenses/`, `THIRD_PARTY_NOTICES.md`: licencias y avisos de dependencias y modelos.
- `experiments/`: prototipos conservados; sus recursos locales no se versionan.
- `docs/archive/`: documentos históricos de la base; no describen una publicación actual de la demo.

## Alcance de la primera limpieza

Se separaron los helpers del editor y el parser PDF, se extrajeron las pruebas largas, se aplicó rustfmt y se unificó la ejecución de pruebas UI en local y CI. Se eliminaron dos imágenes antiguas sin referencias, una declaración TypeScript duplicada y un wrapper Windows con rutas personales obsoletas. El historial anterior permite recuperar estos archivos.

La coordinación de `App`, la grabación y el flujo de setup permanecen en sus vistas. Dividirlos exige trabajar sus estados y contratos en otra iteración; no se cambió su arquitectura durante esta limpieza.
