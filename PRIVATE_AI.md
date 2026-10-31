# Private AI — frontera local del demo

Estado: implementación del primer hito, 30 de septiembre de 2026. Actualizado el 1 de octubre de 2026 con el diseño del prompt de Qwen, la tolerancia de anclaje y el cierre de la puerta de exportación. Este documento sustituye las referencias históricas a la corrección con Qwen para el flujo nuevo.

## Recorrido

Private AI → importar TXT/MD, DOCX, PDF o imagen (PNG/JPG/TIFF/HEIC), o pegar texto → analizar con Qwen3 local → revisar valores y aliases → abrir la versión sanitizada en Edit para ajustar texto, tipografía y sangrías → guardar → confirmar revisión → “Preview what leaves this computer”. En macOS, los PDF escaneados y las imágenes se leen con OCR local antes del análisis. El documento privado editado y su HTML permanecen en memoria durante la sesión; no se incorporan al historial de audio.

La previsualización no envía nada. Es el JSON exacto de una posible solicitud futura: `{ "schemaVersion": 1, "document": "…texto sanitizado…" }`. No contiene nombre de archivo, documento original ni mapa de entidades. No existen APIs de OpenAI o BLegal, clientes externos de documentos, RAG, agentes ni sincronización.

## Arquitectura

- `src-tauri/src/documents.rs`: selector nativo; extracción local UTF-8, XML de DOCX, PDF e imágenes. Máximo 10 MB por archivo y 64 KB de texto. DOCX XML limitado a 4 MB; sin ejecución de macros ni descarga de recursos. DOCX procesa el cuerpo principal, no encabezados, notas, comentarios o anexos.
- `src-tauri/ocr/bluedoor-ocr.swift`: componente empaquetado para macOS con Vision, ImageIO y PDFKit del sistema. Renderiza y reconoce cada página del PDF localmente, usa texto embebido cuando es más completo y OCR para páginas escaneadas o mixtas. Límite de 30 páginas, 40 megapíxeles por imagen y 3600 píxeles de lado mayor en el render OCR. No usa red ni instala modelos adicionales. En Windows se mantiene la extracción de PDF con texto; OCR de imágenes y escaneos aún no está disponible en ese paquete.
- `src-tauri/src/llm.rs`: proveedor de extracción (`EntityExtractor`), llama.cpp en proceso, modelo cacheado, contexto 8192 tokens, presupuesto de salida 2048. Qwen devuelve JSON de entidades, no una reescritura. Se rechazan salidas vacías, incompletas, JSON inválido, categorías desconocidas y predicciones sin coincidencia en la fuente. Fragmentos completos de hasta 4000 caracteres con 300 de solapamiento; no truncado silencioso. Si un fragmento aun así se pasa del presupuesto, `split_oversized_chunk` lo divide por párrafos (y, si no basta, por líneas) con un mínimo de 600 caracteres, de modo que un bloque sin saltos de línea no desborda el contexto ni pierde cobertura; la subdivisión está fijada por un test con un bloque de 12.000 caracteres. Los delimitadores de chat del documento se neutralizan en el prompt: los tokens de control ChatML (`<|im_start|>`, `<|im_end|>`, `/no_think`) y, como defensa en profundidad, cualquier línea que sea exactamente una etiqueta de rol (`user`, `assistant`, …), porque los modelos pequeños no siempre analizan ChatML de forma estricta. Solo se reescriben esas líneas; el resto del documento sobrevive byte a byte para que las entidades sigan coincidiendo con la fuente.

  **Diseño del prompt** (`privacy_prompt`). Es deliberadamente corto: con un prompt largo el modelo se diluye y devuelve menos entidades. Son cuatro bloques en inglés (el modelo responde mejor en inglés aunque el documento sea español):
  - `PURPOSE`: el sistema redacta documentos jurídicos de identidad, mercantiles, procesales y notariales. Elimina datos identificadores y **preserva la naturaleza jurídica del documento**.
  - `DOCUMENT INDEPENDENCE`: el contenido puede ser de cualquier género; no se infiere el tipo ni se aplican reglas por tipo.
  - `EXTRACT`: personas, organizaciones, identificadores, bienes, ubicaciones, contactos y referencias procesales identificables.
  - `NEVER EXTRACT`: fechas, importes, citas legales, datos agregados y texto no identificable.

  Antes el sistema se describía como corrector ortográfico de una transcripción, así que el modelo olvidaba organizaciones y números de proceso en documentos que no fueran transcripciones. Ahora el prompt declara el propósito real y la independencia del tipo de documento. La prohibición de extraer fechas e importes nunca se le decía al modelo, solo se filtraba después: era una defensa tardía, y por eso las fechas de un certificado salían como `DATE` y las notarías quedaban redactadas. 
- `src-tauri/src/privacy.rs`: fuente de verdad del documento y del mapa en memoria Rust; una sesión activa, sin archivos privados de persistencia. Combina reglas deterministas y entidades del modelo. Aliases estables, sustitución por rangos del original, sin cascadas, prioridad a coincidencias más largas y búsqueda sin distinción de mayúsculas. Fechas, importes y argumentos no son reescritos por el modelo. `ground_model_detections` descarta los spans del modelo que no aparecen en el fragmento, pero solo si la mayoría de lo propuesto es inventado; si la respuesta es mayoritariamente correcta se conservan los spans válidos en lugar de perder toda la detección. `is_non_identifying` filtra en el extremo las citas legales con número, las fechas desnudas y los importes con símbolo, comparando sin acentos para que el filtrado no dependa de la codificación del OCR. `strip_field_label` quita el rótulo antes de comparar (`CEDULA:` no debe impedir identificar el valor). Variantes de una entidad pueden agruparse con el mismo alias mediante revisión. Los aliases permiten etiquetas personalizadas de 1–120 caracteres, incluidos espacios; se conservan al aplicar el mapa durante la sesión. Se rechazan valores vacíos, caracteres de control y coincidencias con información privada del documento.
- **Invariante de sanitización** (`require_sanitized`): un mapa vacío produce `sanitize() == original`, es decir, el documento privado entero devuelto como salida «SAFE». Por eso análisis, revisión, vista previa y exportación exigen mapa no vacío **y** texto sanitizado distinto del original. Sin este invariante, un testamento o una minuta sin emails, teléfonos o IDs se aprobaba como si estuviera limpio.
- `review_private_document`: valida entidades contra el original, aplica el mapa, comprueba patrones y actualiza la revisión. Editar invalida aprobación y vista previa. Las revisiones están ligadas al ID y a una revisión monotónica; resultados desactualizados se descartan. Reanalizar revoca la aprobación inmediatamente, incluso si después falla Qwen.
- `update_private_document`: guarda el texto editado solo en la sesión privada, revoca aprobación y vuelve a buscar datos residuales. La vista de salida entrega exactamente ese texto solo después de una nueva revisión. Un cambio en el mapa regenera la versión sanitizada y descarta las ediciones anteriores para evitar mezclar revisiones.
- `preview_private_outbound`: requiere análisis completo y revisión humana confirmada; comprueba de nuevo el texto exacto de la sesión y sus filtraciones antes de devolver únicamente campos autorizados. Sin edición manual exige que la salida coincida con la sanitización reconstruida desde el original. El frontend no puede pasar una salida arbitraria como “segura”.
- **Puerta de exportación en el escritor** (`gate_private_export`): `generate_pdf` recibe la identidad de la sesión revisada (`{id, revision}`) y la valida en Rust antes de escribir. Exige revisión confirmada, mapa no vacío, y que el texto y el HTML de la maquetación coincidan con el texto revisado salvo espacios. El rich formatting se conserva porque `html_visible_text` compara el texto realmente dibujado, no el marcado. El título también se valida con las mismas reglas: el escritor lo sella en el encabezado de cada página y en los metadatos del PDF, así que es contenido exportado aunque no venga del cuerpo. Antes de este cambio la puerta vivía solo en el renderer y `generate_pdf` no validaba nada. Desde el 1 de octubre la puerta es **incondicional**: `generate_pdf` valida siempre, y si el id de sesión no está en el mapa devuelve un error en vez de omitir la comprobación. La función pura `check_export_authorization` queda separada para poder testearla, y `check_export_labels` aplica el mismo filtro anti-fuga a `language` y `model`, que antes viajaban al PDF sin pasar por el gate.
- `html_visible_text` concatena los runs de estilo sin separador y une los bloques con un espacio. Antes insertaba un espacio entre runs, lo que partía "EMAIL_01." en "EMAIL_01 ." y rechazaba un PDF cuyo texto era idéntico al revisado. El error estaba también en la maquetación: `flush_text` descartaba los runs de solo espacios, así que el PDF impreso pegaba las palabras al cambiar de estilo a mitad de una frase. Ambos comportamientos están fijados por tests.
- `privacy_advisories`: avisos no bloqueantes sobre lo que la capa determinista no cubre. Si el mapa no contiene ninguna persona, la interfaz lo dice, porque los nombres son la única categoría sin regla determinista.
- `get_private_extraction`: la extracción inicial sin correcciones no viaja en la sesión; se pide bajo demanda para que el WebView no conserve una segunda copia del documento privado.
- `src/components/PrivateAI.svelte`: la visualización PRIVATE / SAFE — SANITIZED refleja `session.reviewed` real, corrección manual del mapa, aliases, agrupación de variantes y previsualización. El mapa se edita en una subpágina interna accesible por “Mapa privado”, a la derecha del análisis; el botón se habilita cuando el análisis genera entradas. “Abrir en Edit” aparece inmediatamente después y exige aplicar cambios pendientes del mapa. Volver al documento conserva los borradores. La pantalla se mantiene montada al navegar a audio para conservar borradores; cerrar la app descarta la sesión. El rótulo traduce `private` con el idioma del documento; `Pending review` / `Pendiente de revisión` sustituye a `SAFE` mientras `reviewed` sea falso, porque antes la interfaz prometía un documento saneado antes de que nadie hubiera revisado.
- `src/components/ModelsView.svelte`, `models.rs` y `model-manifest.json`: selector de pesos Qwen, descarga, cancelación, progreso y papelera en la pestaña Modelos. Cada peso tiene tamaño/hash fijos, HTTPS, archivo temporal y comprobación de disco. Se verifica SHA-256 antes de cargar el modelo en caché. La descarga solo solicita pesos públicos; nunca contiene el documento.
- Audio/transcripción, editor, historial y generación de PDF siguen con los módulos originales. No se registró `correct_transcript`.

## Resultados medidos

Con los pesos Q4_K_M verificados, sobre Apple Silicon y backend Metal (unos 8 tokens/s, 2048 tokens de salida por fragmento):

- **Fixture legal** (`tests/fixtures/private-document.txt`): 10 entidades, incluidos `Banco Andino S.A.` y `Carlos Rivera`; 0 huecos de revisión.
- **Certificado de dominio** (sintético, 17 detecciones): 17 entidades, 0 huecos. Detecta `COMPAÑIA INMOBILIARIA ANDINA S.A.` e `INVERSIONES PACIFICO CIA LTDA`; ya no redacta fechas ni importes. Residuo conocido: la matrícula y el catastro siguen siendo clasificadas como teléfono.
- **Documento denso**: 11.802 caracteres, 4–5 pasadas, ~170 entidades, 0 huecos. El solapamiento de 300 caracteres es lo que elimina las entidades partidas entre fragmentos.
- **Fragmentación grande**: 13.440 caracteres en 4 fragmentos, 7 entidades, 0 huecos.
- **Batería**: `cargo test --lib` con 88 pruebas (12 ignoradas) y los 11 tests reales de Qwen en verde; `clippy -D warnings` limpio.

Estos números son de QA con documentos sintéticos y dependen del modelo elegido: no son una garantía de cobertura.

## Pendientes conocidos

Ordenados por lo que más puede romper la promesa de privacidad. El primero ya está resuelto y se conserva como aviso:

1. **Empaquetado de los backends de IA — corregido el 1 de octubre de 2026.** `platform.rs` exige `Contents/Frameworks/libggml-cpu-apple_m1.so` y `libggml-metal.so`, más las cuatro dylibs de ggml/llama, con rpath `@executable_path/../Frameworks`. El flujo correcto es `pnpm demo:mac` (`scripts/build-local-demo.py`), que encadena build, `package-local-ai.py`, `verify-bundle.py` y la firma ad hoc. Un `pnpm tauri build --bundles app` a secas no copia nada y produce una app sin motor: hay que usar `demo:mac`. `package-local-ai.py` ahora lee la lista de backends de `platform.rs` y aborta si falta alguna, en vez de confiar en lo que el build emitió, y añade el rpath de Frameworks a cada Mach-O y no solo al ejecutable. Comprobado: `verify-bundle.py` en verde y la app instalada supera `check_installation`, que es la prueba de que ambos backends cargan.
2. **`Session.original` sigue serializándose al WebView.** La puerta de exportación ya no se puede omitir, pero el documento original continúa expuesto al renderer: un XSS todavía lo leería. La solución de fondo es no enviar el original al WebView.
3. **Audio sin límites.** No hay tope de tamaño ni de duración, y el `ffmpeg` se lanza sin timeout ni cancelación.
4. **`parse_color` entra en pánico** con bytes no ASCII (`html.rs`), por indexar la cadena como si fuera un slice de un byte.
5. **`store.rs`** reescribe el JSON completo del historial en el hilo del IPC y no pide confirmación al borrar una entrada vacía.
6. **PDF:** los estilos permitidos no están en whitelist y el texto se inserta escapando solo algunos caracteres.
7. **Dependencias:** Svelte 5.57.0, Tauri CLI/API y TypeScript 5→7, Vite 6→8. Sin vulnerabilidad verificada, pero merece un `pnpm audit`.
8. **GGML** sigue abortando al descargar Metal al cerrar la app.

## Reutilización histórica

Referencia: repositorio local Voice to PDF, versión anterior a `1e203da` (eliminación de Qwen), junto a `MACOS_PORTABILITY.md` y los registros de septiembre de 2026.

Se reutilizaron llama-cpp-2/llama-cpp-sys-2 0.1.156, carga de backends CPU/Metal propios, inicialización/contexto/batches, `AddBos::Never` para Qwen3, sampler `top_k(20) → top_p(0.8) → temp(0.25) → dist(42)`, comprobación de fin de generación y asset Qwen3 fijado por hash. La limpieza de marcadores de pensamiento se corrigió para usar longitudes reales de los tags. El prompt y el comando de corrección de transcripciones no se restauraron.

El código usa bibliotecas nativas dinámicas para evitar el conflicto de ggml con el motor STT. `package-local-ai.py` empaqueta las bibliotecas del build y sus módulos, y elimina rpaths absolutos. `demo:mac` genera y abre un `.app` local firmado ad hoc; no distribuye ni publica. La ruta comercial existente reutiliza el mismo empaquetado de IA.

## Modelo y recursos

Qwen3, seleccionable en Modelos:

- Peso predeterminado Qwen3-0.6B Q4_K_M: 396.705.472 bytes (397 MB) en disco; SHA-256 `ac2d97712095a558e31573f62f466a3f9d93990898b0ec79d7c974c1780d524a`. Fuente inmutable: `unsloth/Qwen3-0.6B-GGUF` pineada por commit `50968a4468ef4233ed78cd7c3de230dd1d61a56b`; licencia Apache-2.0 de Qwen3 incluida en `licenses/models`. Elegido por velocidad en dispositivos móviles y equipos con poca RAM; la arquitectura `qwen3` se detecta por metadatos GGUF, por lo que el sampler ChatML existente se aplica sin cambios. Los pesos antiguos Q4_K_M de 1,7B (blob Ollama `3d0b7905…`) dejan de servirse; una instalación con ese fichero lo volverá a descargar como 0.6B bajo el mismo id `qwen3-fast`.
- Peso alternativo Q8_0: 2.220.088.992 bytes (2,22 GB), SHA-256 `48bffa1478e612465567147cae67bf5e1539ed3080ec5415a35789d6fd4c851d`; se descarga por separado y solo se carga si el usuario lo selecciona.
- Fuentes primarias: [Qwen3-0.6B](https://huggingface.co/Qwen/Qwen3-0.6B), [Qwen3-1.7B](https://huggingface.co/Qwen/Qwen3-1.7B) y [llama-cpp-2 0.1.156](https://docs.rs/llama-cpp-2/0.1.156/llama_cpp_2/).

## Límites de privacidad

Esta es pseudonimización revisada, no anonimización garantizada. Qwen puede omitir nombres, variantes, organizaciones, direcciones o identificadores, o proponer detecciones incorrectas. Las reglas detectan emails, formatos de teléfonos, números de 10/13 dígitos, procesos ecuatorianos, identificadores etiquetados y direcciones etiquetadas; no cubren todos los formatos y pueden generar falsos positivos. Los nombres no tienen un detector determinista general.

Datos indirectos, hechos, fechas, lugares, relaciones y cantidades conservados pueden permitir reidentificar. Las variantes pueden requerir agrupación manual. Eliminar una detección falsa restaura el texto correspondiente; el usuario debe evaluar el resultado, y las reglas aún bloquearán patrones conocidos de identificadores. No se promete cobertura completa ni revisión jurídica. El OCR puede confundir cifras, nombres, acentos, columnas o páginas; compara siempre el texto extraído con el archivo original antes de analizar y aprobar.

El editor ofrece una página con márgenes y conserva los saltos de línea y párrafos extraídos. Todavía no recupera coordenadas de texto, tablas ni márgenes exactos de un PDF escaneado; Qwen se usa para detectar entidades, no para reescribir hechos o inventar maquetación. El formato rico sirve para edición local y exportación PDF; la vista de salida externa contiene texto plano revisado.

“Exportar PDF” está siempre visible en Edit y exporta el contenido actual, incluidos los cambios manuales, a una ruta local elegida por el usuario. Antes de abrir el selector se comprueban entidades residuales del mapa y patrones de identificadores; si permanecen datos detectables, se bloquea la exportación. La revisión confirmada se exige para la vista de salida externa. Los números de juicio etiquetados y los números compactos de 14–15 cifras se detectan aunque el OCR haya eliminado guiones.

“Revisar OCR con Qwen” propone una sola letra inicial faltante en palabras comunes o etiquetas. Solo se admiten palabras existentes, coincidencias completas y propuestas alfabéticas que conservan exactamente el resto de la palabra. Las sugerencias no se aplican automáticamente. El usuario las compara con el archivo y acepta o descarta; aplicar actualiza las coincidencias exactas e invalida el análisis, mapa y aprobación anteriores. La extracción inicial se conserva en memoria para comparar. Qwen recibe texto, no imágenes, y no puede verificar caracteres realmente invisibles; el prompt excluye nombres, identificadores, importes y fechas de estas correcciones.

El original y el mapa existen en memoria del proceso y del WebView. No se implementaron cifrado de RAM, zeroization, protección ante swap/dumps ni sandbox de parsers. El archivo fuente del usuario permanece en disco. El texto extraído debe verificarse antes del análisis. La previsualización se limita al texto, no autoriza a compartir el archivo original ni sus metadatos.

## Uso y verificación

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
# macOS, app local preparada:
pnpm demo:mac

pnpm typecheck
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
# Con Qwen descargado y verificado:
cargo test --manifest-path src-tauri/Cargo.toml real_qwen_extracts_private_document_locally -- --ignored
# Con pnpm dev en puerto 1420 (y vite --config tests/vite.config.ts en 5198
# para los tests del editor):
node tests/check-private-ai.mjs
node tests/check-editor.mjs
node tests/check-history.mjs
node tests/check-conversation.mjs
node tests/check-pcm.mjs
```

Los mismos comandos corren en CI (`.github/workflows/ci.yml`): typecheck y build del frontend, clippy con `-D warnings` y `cargo test --lib` en macOS arm64, y los cuatro tests de UI. `cargo fmt --check` queda fuera a propósito: el crate ya venía sin formatear y reformatearlo mezclaría estilo dentro de un commit de privacidad. Los fixtures `tests/fixtures/private-document.*` son ficticios y se usan solo para QA. No contienen un caso real. Los tests UI simulan IPC —y replican los invariantes de `require_sanitized` para no aprobar en la interfaz lo que Rust rechazaría—, mientras que los tests Rust comprueban la lógica real. El test explícito de Qwen exige pesos verificados y no omite inferencia cuando falta el modelo. Algunas pruebas heredadas de STT hacen skip si faltan sus pesos/fixtures; no equivalen a una prueba real de Whisper.
