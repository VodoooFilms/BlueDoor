# Reporte de cambios — Belegal _ Local demo

**Fecha:** 2026-09-08 17:30 (hora local)
**Estado:** cambios en el árbol de trabajo (sin commit, sin push). Pendientes de prueba local por el usuario.

## Resumen

Lote de puesta a punto (UX/lógica/flujo) aprobado tras una auditoría. 8 correcciones en 10 ficheros. Frontend (`vite build`) OK, backend (`cargo check`) OK, tests PDF (`pdf_generation_writes_file`, `pdf_report_is_multipage`) OK, `cargo test --no-run` compila. svelte-check: mismo nivel que antes (7 errores + 1 warning pre-existentes, sin nuevos).

## Cambios

### 1. Conservación de datos al guardar la corrección (fix importante)
- **Antes:** `handleUpdateTranscriptText` colapsaba los `segments` en un único segmento 0..fin cuando guardabas la corrección → se perdían los timestamps originales.
- **Ahora:** la corrección se guarda aparte en `Transcript.corrected?: string` y `StoredTranscript.corrected: Option<String>` (serde `default`/`skip_serializing_if`). La vista con timestamps sigue mostrando los segments originales; el PDF usa el texto corregido. Al reabrir, TranscriptView restaura `corrected` en el `$effect` por `transcript.id`.
- Ficheros: `src/lib/types.ts`, `src/App.svelte` (`toStored`/`fromStored`/`handleUpdateTranscriptText`), `src-tauri/src/lib.rs` (struct `StoredTranscript`), `src/components/TranscriptView.svelte`.

### 2. LLM activo = motor real (chip sincronizado)
- **Antes:** la selección de Qwen en la ventana Modelos era solo de UI (localStorage); la corrección usaba el último descargado en Rust → el chip podía mentir.
- **Ahora:** `AppState.llm_path` es la fuente de verdad.
  - `get_active_llm()` devuelve el id de variante que coincide con `llm_path` (fallback `qwen15`).
  - `set_active_llm(selected)` activa el LLM cacheado SOLO si el fichero existe (error claro si no).
  - `setup()` arranca con el PRIMER LLM instalado (no asume `qwen15`).
  - `delete_llm` sobre el activo cae al siguiente LLM instalado (o cadena vacía → error claro al corregir).
  - La UI sincroniza el chip con `get_active_llm` en `App.onMount` y en `ModelsView.refresh`/`pickLlm`.
- Ficheros: `src-tauri/src/lib.rs`, `src/App.svelte`, `src/components/ModelsView.svelte`, `src/lib/models-store.svelte.ts` (comentario actualizado).

### 3. Grabación: atajos de teclado y clic-para-detener
- **Antes:** el hint decía "haz clic o Cmd+Enter para detener" pero no existía el handler; además el clic en la zona solo funcionaba en `idle`.
- **Ahora:** handler global `keydown` (Espacio y Cmd/Ctrl+Enter) para arrancar y parar, que ignora INPUT/BUTTON/SELECT/TEXTAREA/editables. El clic en la zona de grabación también detiene en plena grabación.
- Copies actualizados: "haz clic o pulsa Espacio para grabar" / "Grabando… haz clic, Espacio o Cmd+Enter para detener".
- Ficheros: `src/components/Recorder.svelte`, `src/lib/i18n.ts`.

### 4. Historial lateral con empty-state
- **Antes:** el aside se ocultaba cuando no había transcripciones (reflow al crear la primera).
- **Ahora:** siempre visible, con "Aún no hay transcripciones" cuando está vacío.
- Ficheros: `src/components/Recorder.svelte`.

### 5. Feedback en botón "Copiar"
- **Antes:** copiaba sin aviso (la clave `t.copied` existía sin uso).
- **Ahora:** flash "Copiado"/"Copied" durante ~1.2 s (borde+texto en `--success`).
- Ficheros: `src/components/TranscriptView.svelte`.

### 6. Back-link "← Nuevo" condicional
- **Antes:** aparecía en la masthead dentro de Modelos/Skills (inútil ahí).
- **Ahora:** solo cuando hay transcripción activa y NO se está en Modelos/Skills.
- Ficheros: `src/App.svelte`.

### 7. Confirmaciones Panto (sin `confirm()` nativo)
- **Antes:** borrar modelos y "Borrar todas" usaban `confirm()` que rompía el estilo.
- **Ahora:** diálogo in-app en overlay (cancelar/eliminar, Escape o clic fuera para cerrar), con claves nuevas `deleteTitle`, `deleteWhisperBody`, `deleteLlmBody`, `cancel`; `deleteConfirm` pasa a ser la etiqueta del botón.
- Ficheros: `src/components/ModelsView.svelte`, `src/components/SkillsView.svelte`, `src/lib/i18n.ts`.

### 8. Limpieza de i18n muertas
- Eliminadas claves sin uso: `importing`, `recordingStopHint`, `selectLanguage`, `errorPrefix`, `brand` (la marca se renderiza como texto literal). `copied`/`noHistory` ahora sí se usan. `usingModelsTitle` → "pestaña Modelos".
- Fichero: `src/lib/i18n.ts`.

## Notas para la prueba local

- El motor LLM ahora responde a lo que elijas en Modelos → Qwen. Si borras el Qwen activo, cae al siguiente instalado.
- Al reabrir una transcripción corregida se restaura el texto corregido (botón en "Grabado"); los timestamps siguen mostrando los segments originales.
- Si tras el rebuild el micrófono no funciona: `tccutil reset Microphone com.belegal.localdemo`.
- Sin cambios de build/empaquetado: aplican las reglas de dylibs/rpath/firma de siempre.
- Recuerda: cuando corra `cargo build` que regenere el binario hay que re-aplicar LC_RPATH (`scripts/build-install.sh` o `scripts/build-dmg.sh` lo hacen solos).
---

# Auditoría de infraestructura aplicada — 2026-09-09

**Estado:** committeado y pusheado; app reinstalada en /Applications (OK: running). Verificación: `cargo clippy --all-targets` limpio, `cargo test` = 4/4 OK, `pnpm typecheck` = 0 errores / 0 warnings (con `strict`), `pnpm build` OK, release `tauri build` + instalación OK.

## Limpieza segura
- `logos/` (raíz) borrado de git: era código muerto, los activos reales viven en `src/assets/logos/`.
- `.gitignore`: eliminadas entradas duplicadas (`dist/`, `.DS_Store`, `.env`).
- `Recorder.svelte`: fusionado el `onDestroy` duplicado; `languages` ahora `$derived` (capturaba la `t` inicial).
- `TranscriptView.onBack`: prop muerta eliminada (y su paso desde `App.svelte`) → era uno de los errores de svelte-check.
- `build-install.sh`: eliminado el `pnpm build` previo al bundle (tauri ya ejecuta `beforeBuildCommand`).
- `index.html lang="es"` (el idioma por defecto es español; ahora UI y atributo coherentes).
- `entitlements.mac.plist`: `app-sandbox = false` (la app corre sin sandbox; antes era una bomba de relojería si se firmara con Developer ID).

## Backend: catálogo unificado y estado LLM honesto
- `WHISPER_MODELS` ahora incluye el fichero (`id, name, size, file`); eliminado `model_file_name()`. `whisper_entry()` como único lookup.
- `get_active_llm()` devuelve `""` cuando no hay ningún LLM activo (antes devolvía `qwen15` aunque no existiera). La UI lo muestra como "—".
- `setup()` ya no deja un `llm_path` fantasma: si no hay ningún LLM instalado, queda vacío.

## Restructuración: `lib.rs` monolito (~1190 líneas) → módulos
- `types.rs`: Segment, StoredTranscript, TranscriptionResult, WhisperConfig, AppState (con `resolve_model_path`/`llm_engine`).
- `paths.rs`: models_dir / transcripts_file / skills_file.
- `models.rs`: catálogos, descargas con progreso + reintentos, list/delete, `set/get_active_llm`, `whisper_default_file`/`first_installed_llm`.
- `audio.rs`: whisper, ffmpeg, WAV/resample, temporales privados, `cleanup_leftovers`, test de transcripción.
- `pdf.rs`: fuentes, `ReportDecorator`, `generate_pdf`, tests PDF.
- `store.rs`: persistencia atómica de transcripts y skills.
- `llm.rs`: LlmEngine + `correct_transcript` (antes vivía en lib.rs).
- `lib.rs` queda como orquestador: `mod`s + `run()` (setup + `invoke_handler`).

## Concurrencia: comandos pesados async con `spawn_blocking`
- `transcribe_file`, `transcribe_blob`, `correct_transcript`, `generate_pdf` y `download_model`/`download_llm` ahora corren en hilos de fondo (antes bloqueaban el hilo principal). Los `invoke` del frontend no cambian.

## Calidad / CI
- `tsconfig`: activado `"strict": true`.
- `package.json`: nuevo script `typecheck` (`svelte-check --tsconfig ./tsconfig.json`) → **0 errores / 0 warnings**.
  - Recorder: la variable `state` colisionaba con el sistema de runes de svelte-check → renombrada `recStatus`.
  - `$state<T>(v)` con genéricos en Recorder → se usa cast en el argumento (`$state("idle" as RecordingState)`) para evitar el error de svelte-check.
- CI nuevo: `.github/workflows/ci.yml` — frontend (ubuntu): install → typecheck → build; backend (macos, para que los tests PDF encuentren Arial): clippy `-D warnings` → test.
- Test `transcribe_spanish_works`: ahora hace SKIP si falta el fixture `/tmp/vtp-es.wav` (antes paniceaba con `unwrap()`).
- `svelte`/`svelte-check` actualizados al último dentro de sus rangos (5.57.0 / 4.7.6).

---

# Sesión 2026-09-09 — icono de dock + renombrado de marca + README bilingüe

**Autor:** big pickle (asistente de desarrollo)
**Fecha:** 2026-09-09
**Estado:** todo committeado y pusheado a `main` (`3ed02a8`, `ecc7ee8`, `244ec63`); app reinstalada localmente como `Belegal _ Local demo.app` vía `scripts/build-install.sh`.

## 1. Icono de dock corregido (fin del "doble contenedor")
- El `belegal-local-demo_icon.png` anterior mostraba un doble contenedor (marco doble/borde blanco) al renderizarse en el dock.
- Se localizó el logo real del usuario en `~/Desktop/V_icon.png` (V violeta sobre transparencia, 1254×1254).
- Nuevo máster 1024×1024 **a sangre completa**: fondo gris oscuro opaco `#2d2d32` + V violeta centrada, **sin transparencia en las esquinas** → macOS no le añade su propio marco → desaparece el efecto doble contenedor.
- Regenerada toda la familia de iconos con `pnpm tauri icon` (`src-tauri/icons/*`, `icon.icns`, `icon.ico`, carpetas android/ e ios/).
- Verificado: 0 píxeles blancos, esquinas totalmente opacas, V dentro del tile. Preview en `~/Desktop/belegal-local-demo_dock_icon.png`.
- Se retiró del Desktop un archivo `belegal-local-demo_icon.png` generado por error del asistente; el logo real del usuario (`V_icon.png`) quedó intacto. Disculpas por el contratiempo previo.

## 2. Reinstalación local
- `scripts/build-install.sh` OK → empaquetado `Belegal _ Local demo.app`, dylibs + LC_RPATH + cuarentena + firma ad-hoc, instalado en `/Applications` y lanzado (`OK: Belegal _ Local demo running`).

## 3. Renombrado de marca: VoiceToPDF → Belegal _ Local demo (T minúscula)
Cambiado en app, build y documentación; se conservan intactos los identificadores de máquina (`com.belegal.localdemo`, binario/crate `belegal-local-demo`, ficheros de datos):
- **App**: masthead en `src/App.svelte` (texto + comentario), `<title>` en `index.html`, título de ventana y `productName` en `tauri.conf.json`.
- **Backend/PDF**: cabecera y pie del PDF (`download`), ejemplo de marca en el prompt del LLM (`llm.rs`), mensaje de arranque (`lib.rs`), `Info.plist` (descripciones de uso de micrófono), `authors` en `Cargo.toml`, capabilities (`default.json` + esquema generado).
- **Build**: scripts buscando `Belegal _ Local demo.app` (target/release o target/<triple>), instalación en `/Applications/Belegal _ Local demo.app`, DMG → `Belegal _ Local demo_${VER}_aarch64.dmg` y volname `Belegal _ Local demo`.
- **Docs**: README.md, PRIVACY.md, AGENTS.md y este reporte.
- Verificación: `cargo check` OK y `svelte-check` = 0 errores / 0 warnings; `rg 'VoiceToPDF'` = 0 coincidencias.

## 4. README bilingüe (ES/EN)
- Nuevo `README.es.md`: traducción completa al español (mismo índice que la versión EN).
- Selector de idioma en la cabecera de ambos: `English · Español` (con enlace mutuo).
- README referencia el icono nuevo (`src-tauri/icons/icon.png`) que ya muestra la V violeta sobre gris oscuro.

## Commits de la sesión
- `3ed02a8` — icono regenerado desde V_icon (gris oscuro a sangre completa, sin contenedor).
- `ecc7ee8` — renombrado de marca VoiceToPDF → Belegal _ Local demo (16 ficheros).
- `244ec63` — README bilingüe (README.es.md + selector).

## Pendiente
- Revisar el repo en GitHub (README bilingüe visible).
- Recordatorio: tras el cambio de `productName`, la app instalada ahora es `Belegal _ Local demo.app` (la antigua `VoiceToPDF.app` quedó fuera de /Applications al reinstalar).
