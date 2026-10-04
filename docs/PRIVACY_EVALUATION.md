# Evaluación de privacidad y conservación de documentos

Evaluación local de línea base del 3 de octubre de 2026. Modelo: Qwen3 1.7B Q4, antes del postentrenamiento. El PDF privado de PayPal se procesó únicamente en este equipo. El usuario posteriormente autorizó el postentrenamiento; la ejecución se documenta en [POSTTRAINING_RUN.md](POSTTRAINING_RUN.md).

## Diferencia entre detección, revisión y formato

| Etapa | Resultado observado | Qué permite concluir |
|---|---|---|
| Detección automática, OpenPII | 195/282 anotaciones eliminadas, **69,1 %**; 87 visibles; 4/32 documentos sin residuos anotados; 1 análisis incompleto | La muestra no permite confiar en exportación automática sin revisión |
| Detección automática, Gretel financiero | 67/109 anotaciones eliminadas, **61,5 %**; 42 visibles; 5/20 documentos sin residuos anotados; 3 análisis incompletos | El dominio financiero también presenta omisiones, incluidas 16/41 anotaciones de nombres |
| Revisión manual | Se pueden añadir entidades omitidas y corregir alias; el mapa se aplica a todas las apariciones | Es una intervención humana; no se presenta como una mejora del modelo |
| Conservación del PDF, después de aplicar un mapa conocido | Dos flujos: PDF digital de dos tamaños de página e imagen escaneada. Sin cambios de píxeles fuera de las regiones privadas; exportación igual a vista previa; OCR independiente no encuentra los valores privados del fixture | Verifica renderizado y eliminación del contenido original para estos casos, no el recall del detector |
| Postentrenamiento | **Ejecución posterior interrumpida** | Las cifras son exclusivamente la línea base. El intento terminó sin completar el entrenamiento; no hay resultados finales ni nuevo modelo validado |

Se invocó `analyze_with_progress` y `sanitize`, las funciones reales del motor, con los pesos instalados. Las muestras están congeladas: primeros 24 registros españoles y 8 ingleses de la validación OpenPII; primeros 12 españoles y 8 ingleses del test Gretel. Son muestras consecutivas pequeñas, **no una estimación representativa de todo el corpus**. Cero errores de ejecución; los análisis incompletos siguen contados como fallos.

La métrica busca cada valor anotado en el texto protegido, sin distinguir mayúsculas y tolerando espacios. Un valor repetido solo se considera eliminado si desaparece de todas sus apariciones. No es F1 de spans ni prueba de anonimato. Se mantienen todas las categorías del dataset, incluso fechas, edades y títulos que la política actual puede conservar. Los offsets de OpenPII se comprobaron contra sus valores; los rangos Gretel se comprobaron y sus valores se derivaron del texto. Las anotaciones sintéticas pueden tener errores. No se midieron falsos positivos con una auditoría humana exhaustiva.

| Categoría ilustrativa | OpenPII: eliminado / anotado | Gretel: eliminado / anotado |
|---|---:|---:|
| Nombre | GIVENNAME 30/32; SURNAME 26/28 | name 25/41 |
| Email | 24/24 | 1/1 |
| Teléfono | 21/22 | 2/2 |
| Dirección | STREET 8/10; CITY 9/14; ZIPCODE 8/12 | street_address 14/16 |
| Fecha / nacimiento | DATE 8/37 | date 2/21; date_of_birth 0/2 |
| Tarjeta | 1/4 | 1/1 |

No se comparan directamente las columnas como una clasificación entre modelos: son documentos y taxonomías diferentes.

Los resultados completos, entradas, salidas reales y omisiones están en `artifacts/benchmark/{baseline,gretel}-{results,score}.json`. Son artefactos locales ignorados por Git. El documento de PayPal no forma parte del score público y su mapa previo quedó preservado localmente en `artifacts/paypal/previous-reviewed-map.json`.

También se verificó el flujo real de la app instalada en `/Applications`: importar PayPal, analizar, añadir manualmente dirección, variante multilínea del ID, dígitos de tarjeta y nombre abreviado del comercio, aplicar el mapa y exportar una copia de QA. La copia tiene dos páginas del tamaño original; OCR independiente confirma ausencia de esos valores y presencia de alias e importes. Los píxeles fuera de las regiones privadas por línea permanecen idénticos. La primera comprobación de píxeles esperaba máscaras por carácter y señaló los espacios entre caracteres que también se borran; se corrigió la expectativa a las regiones completas por línea y se comprobó de nuevo, sin excluir cambios fuera de esas regiones. Es evidencia de un documento revisado, no una tasa de acierto automática. La copia de QA permanece en `artifacts/paypal/PayPal-layout-QA.pdf`; la app queda abierta para revisión y edición del usuario antes de su exportación final.

## Corpus de texto

Las cifras son las publicadas, salvo las comprobaciones explícitas de archivos. Hay volumen para postentrenamiento; volumen no sustituye revisión de etiquetas, pertinencia ni independencia de los ejemplos.

| Dataset y fuente | Volumen / idioma | Supervisión disponible | Uso propuesto / límites |
|---|---|---|---|
| [OpenPII 1M](https://huggingface.co/datasets/ai4privacy/pii-masking-openpii-1m) | 1.428.143; 23 idiomas, español incluido | Original, texto enmascarado y spans; 19 categorías; sintético | CC BY 4.0. Base de evaluación ya medida; reservar esta muestra fuera del entrenamiento |
| [OpenPII 1.5M](https://huggingface.co/datasets/ai4privacy/pii-masking-openpii-1.5m) | 1.636.375; 30 idiomas; 1.309.912 train / 326.463 validation | Original, enmascarado, spans y BIO | CC BY 4.0. Extiende 1M: **no sumar ambos** ni mezclar particiones duplicadas |
| [Gretel financiero multilingüe](https://huggingface.co/datasets/gretelai/synthetic_pii_finance_multilingual) | 55.940; 4.609 españoles publicados; 7 idiomas | Texto sintético y spans; 29 tipos | Apache 2.0. Prioridad para recibos y datos financieros; validación principalmente automática. El test descargado contiene 5.594 filas, 444 españolas; la descripción narrativa de splits difiere del archivo |
| [Gretel PII inglés](https://huggingface.co/datasets/gretelai/gretel-pii-masking-en-v1) | 60.000; 50k train / 5k validation / 5k test | Texto sintético y listas de entidades; múltiples dominios | Apache 2.0. Convertir entidades a rangos comprobados; no asumir offsets perfectos |
| [Nemotron-PII](https://huggingface.co/datasets/nvidia/Nemotron-PII) | Inglés; 55+ tipos, 50+ sectores | Spans y texto etiquetado sintético | CC BY 4.0. La tarjeta dice 100k y el visor 200k: **conteo pendiente de verificar antes de presupuestar entrenamiento** |
| [MEDDOCAN](https://zenodo.org/records/4279323) | 1.000 casos clínicos españoles | Anotación manual de PHI en BRAT/XML; splits oficiales | CC BY 4.0. Útil para evaluación humana en español y adaptación; los textos de fondo sin anotaciones no cuentan como gold |
| [TAB](https://github.com/NorskRegnesentral/text-anonymization-benchmark) | 1.268 casos jurídicos reales, inglés | Entidades directas/indirectas, correferencias y política contextual | MIT. Reserva jurídica externa; se anonimiza una persona objetivo, no necesariamente toda entidad del documento |
| [SPY](https://huggingface.co/datasets/mks-logic/SPY) | 4.491 plantillas médicas + 4.197 jurídicas, inglés | Placeholders con valores generables | CC BY 4.0. Pruebas adversas y generación controlada. Variaciones de Faker no son plantillas independientes |
| [MAPA original](https://aclanthology.org/2022.lrec-1.400/) | Corpus legal manual pequeño en español y alemán; también silver alemán | Anotaciones gold y silver distintas | Los documentos manuales son útiles para validar; no presentar silver como gold. Descarga original todavía no verificada |
| [MAPA EUR-Lex armonizado](https://huggingface.co/datasets/piimb/mapa-eur-lex-pii) | 42 documentos = 2 originales × 21 idiomas | Inglés manual; otros idiomas proyectados con LLM | CC BY 4.0. Prueba pequeña, **no 42 ejemplos independientes** ni español exhaustivamente validado por humanos |

No se han descargado millones de ejemplos innecesariamente. Ya están locales el subset OpenPII, test financiero Gretel, MEDDOCAN completo y DialogPII. Se preservan metadatos de revisión y checksums de las descargas en `artifacts/datasets`.

## Audio: distinguir grabación, transcripción y etiquetas PII

| Dataset y fuente | Audio real / supervisión | Español / tamaño | Utilidad y restricciones |
|---|---|---|---|
| [SLUE-VoxPopuli NER](https://huggingface.co/datasets/asapp/slue), [toolkit](https://github.com/asappresearch/slue-toolkit) | Audio + transcripción + NER | Inglés; train 5.000 utterances / 14,5 h; dev/test separados | Base para ASR→entidades. NER general no cubre todos los teléfonos, documentos o cuentas. Licencia depende del componente: VoxPopuli CC0, VoxCeleb CC BY 4.0 |
| [SLUE Phase 2, vp_nel](https://huggingface.co/datasets/asapp/slue-phase-2) | Audio, tiempos de palabras y de entidades | Inglés; archivo descargado **1.750 validation**, test publicado 1.838 | Validación de localización temporal. No tiene train de NEL. VoxPopuli upstream CC0; preservar y comprobar licencia del componente, no asumir una licencia uniforme de toda la colección |
| [MSNER](https://huggingface.co/datasets/qmeeus/MSNER), [código/paper](https://github.com/qmeeus/MSNER) | Audio VoxPopuli; train/dev silver; test revisado por humanos | Español, alemán, francés y neerlandés; ~7,3k ejemplos test entre idiomas | Relevante para español. El [LICENSE del repositorio](https://github.com/qmeeus/MSNER/blob/main/LICENSE) es **CC BY-NC 4.0**: no incluir en un entrenamiento comercial sin aclarar permisos de anotaciones |
| [HarperValley](https://github.com/cricketclub/gridspace-stanford-harper-valley) | Llamadas bancarias representadas, transcripciones y tiempos | Inglés | CC BY 4.0. Cercano al dominio de llamadas; faltan spans PII explícitos, habría que anotarlos |
| [DialogPII](https://zenodo.org/records/20863452) | Diálogos/transcripciones anotadas, incluyendo transcripciones derivadas de TTS→ASR | 11 idiomas, español incluido; 147 diálogos base reportados | CC BY 4.0. **ZIP verificado: 176 JSON, ningún WAV/audio.** Útil para detector sobre transcripción con errores; no para medir silenciamiento del audio |
| [VoxPopuli](https://github.com/facebookresearch/voxpopuli) | Grabaciones y transcripciones, sin gold PII exhaustivo | Español y otros idiomas | Audio CC0. Material para construir un benchmark consentido/anotado; por sí solo no evalúa anonimización |
| [CallCenterEN](https://arxiv.org/abs/2507.02958) | Publicación de transcripciones ya redactadas | Inglés; el corpus fuente del paper es mucho mayor | CC BY-NC 4.0; **audio fuente no publicado**. No ofrece pares de audio original/redactado ni todos los valores originales para medir recall |

El Parquet SLUE descargado se inspeccionó: incluye bytes de audio, `word_timestamps` y `ne_timestamps` con etiquetas, rangos de caracteres y segundos. Se ha verificado disponibilidad, **no se ha medido aún el ASR ni la redacción de audio**. La feature actual protege documentos; no silencia audio. Anonimizar el contenido hablado tampoco elimina la identidad biométrica de una voz.

## Diseño del postentrenamiento

**Viable para texto; aún no demostrado que mejore este producto.** Primero comparar el Qwen actual con un detector de spans multilingüe especializado. Elegir por recall de PII crítica, falsos positivos, latencia y memoria, no solo tamaño del corpus. Un detector más pequeño puede ser más apropiado que entrenar generación libre.

La ejecución autorizada utiliza 21.315 fragmentos de entrenamiento y 1.182 de validación, equilibrados entre español e inglés y preparados desde splits autorizados. Se comprobó la separación de grupos, los offsets y que cada entidad objetivo aparezca literalmente en el original. Enseña extracción estructurada; los reemplazos y la geometría se aplican con código determinista. Los detalles y exclusiones están en [POSTTRAINING_RUN.md](POSTTRAINING_RUN.md).

Separar train/dev/test **antes** de generar variantes, por plantilla, persona y documento fuente. Deduplicar OpenPII 1M frente a 1.5M. Mantener los 52 documentos aquí medidos como test congelado. Reservar además 500–1.000 documentos del dominio real español con anotaciones humanas y ejemplos sin PII; no introducirlos en train. Guardar evaluación externa MEDDOCAN/TAB con sus políticas diferenciadas. Estos tamaños son una propuesta de trabajo y no garantizan fiabilidad.

Se está ejecutando QLoRA del modelo de 1,7B en este Mac M4 de 16 GB mediante [MLX-LM](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/LORA.md). Se descargó la revisión original de los pesos y se convirtió a MLX Q4. Después se fusionarán los adaptadores con los pesos base originales y se validará el candidato en el mismo runtime GGUF de BeLegal.

Comparación que debe reportar el entrenamiento: mismo test antes/después; valores aún visibles por tipo; documentos con alguna fuga; análisis completos; latencia/memoria. Recall exacto por spans y falsos positivos necesitan además auditoría de las anotaciones. Para audio, medir por separado (1) detector con transcripción gold, (2) ASR→detector, (3) cobertura temporal del audio protegido, incluyendo números hablados, emails, acentos, ruido y solapamiento. Hace falta un test español con PII exhaustiva y tiempos humanos: los datasets encontrados no cubren completamente ese objetivo.

## Vista y exportación con formato original

En macOS se conservan instantáneas inmutables de páginas PDF/imagen y rangos UTF-8 vinculados a coordenadas. PDF digital: geometría del texto embebido. Escaneado/imagen: Vision OCR; las zonas son estimadas. Pulsar un alias abre su editor; aplicar el mapa actualiza sus repeticiones; añadir omisiones se hace en «Mapa privado». Antes de exportar, revisar visualmente todas las páginas.

El PDF se reconstruye de píxeles protegidos a resolución alta, conservando tamaño de página y gráficos fuera de las zonas. **No retiene texto seleccionable, vectores, enlaces, formularios ni metadatos del original**. El contenido privado se elimina de la imagen, no se tapa encima de objetos recuperables. Se ajustan las fuentes disponibles y el tamaño del alias al espacio original; aliases demasiado largos bloquean exportación. Fondos planos se muestrean alrededor de la zona; fondos complejos y texto superpuesto a gráficos requieren comprobación visual. No se promete identidad vectorial ni geometría OCR exacta universal.

Límites actuales: 10 MB al importar, 30 páginas, 64 KB de texto y presupuesto de resolución. DOCX/TXT conservan el flujo de texto existente; conservación de páginas disponible en PDF e imágenes macOS. Una modificación de prosa/OCR invalida las coordenadas; aplicar de nuevo el mapa o reimportar antes de exportar con formato.

## Reproducir lo comprobado

En esta máquina, usar SDK 14.5: `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX14.5.sdk`, `CC=/usr/bin/clang`, `CXX=/usr/bin/clang++`, `CMAKE_OSX_SYSROOT=$SDKROOT`. El SDK 26.2 del sistema no coincide con su toolchain. Las variables deben ir en el entorno, no como cambios del proyecto.

- `pnpm check`: tipos Svelte y build.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib layout::tests`: genera artefactos nativos para PDF digital/escaneado y comprueba rechazo de coordenadas incompletas y sesiones modificadas.
- `python scripts/verify-layout-output.py` con Pillow y pypdf: verificación independiente de PDF, píxeles y OCR.
- Con Vite de app y harness abiertos, `node tests/check-layout.mjs`, `BELEGAL_TEST_LANG=es node tests/check-private-ai.mjs`, equivalente `en` y `node tests/check-editor.mjs`: flujo de edición/revisión/exportación y regresión.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib measure_dataset_workflow -- --ignored --nocapture`: medición opt-in con datos/modelo instalados. Variables `BELEGAL_BENCHMARK_FILE` y `BELEGAL_BENCHMARK_OUTPUT` seleccionan Gretel. `python3 scripts/score-privacy-benchmark.py artifacts/benchmark/gretel-results.json` puntúa sin ocultar omisiones.
- `python3 scripts/build-local-demo.py --no-open`: empaquetado, firmas y comprobación del bundle local.

El código de la feature no cambia el detector ni pretende convertir los fallos de privacidad observados en tests «verdes». Los tests del formato verifican un mapa conocido independientemente; la medición del modelo registra también sus fallos.


## Editor con formato — 4 de octubre de 2026

La vista de páginas reutiliza `TranscriptView.svelte` para corregir bloques de texto detectado con fuente, tamaño exacto, estilos y regla de sangrías. Guardar vuelve a renderizar la página; la exportación usa el mismo render nativo y las comprobaciones de revisión. Los gráficos y píxeles fuera de las zonas modificadas se conservan. El cambio global de aliases conserva las correcciones y sus estilos, incluso si un alias atraviesa varios fragmentos de formato.

La edición conserva las posiciones: no convierte un PDF en un documento Word con redistribución automática. Si el texto supera el espacio disponible hasta la siguiente línea, el guardado/exportador lo rechaza, preservando el borrador para corregirlo. Las fuentes no disponibles usan una fuente local de respaldo; el OCR sigue siendo estimado. Los documentos permanecen solo en memoria hasta exportarlos.

Validación: flujo de UI en WebKit con el editor existente (selección, fuente, tamaño, negrita, regla, rechazo de desbordamiento, guardar/reabrir y exportar), tres flujos nativos de formato, y comprobación independiente de píxeles/páginas/OCR del PDF. `scripts/verify-layout-output.py` comprueba que la vista previa y el PDF corregido coinciden y que gráficos, aliases y texto no editados permanecen iguales.

La interfaz ahora usa una sola superficie: Proteger abre Edit automáticamente, los controles del procesador quedan arriba y el texto se corrige directamente en las coordenadas de la página. No hay editor lateral; el mapa y la exportación tienen un único acceso. Se verificó la posición del campo editable respecto a la página, guardar/reabrir con fuente y tamaño correctos, y la exportación del documento completo.
