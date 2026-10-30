# Validación de Android — 29 de octubre de 2026

Primera entrega local en la rama `codex/android-protection`. No se ha publicado el APK ni el código de esta entrega. Los archivos de prueba contienen exclusivamente datos sintéticos.

## Entornos

- Mac Apple Silicon: compilación, pruebas de Rust/Svelte y prueba explícita de Qwen en CPU con perfil de 4096 tokens.
- Emulador Android 15/API 35, `sdk_gphone64_arm64`, ARM64, 4 GB configurados, páginas de 4096 bytes. No es un Redmi ni una simulación de su CPU, temperatura o rendimiento.
- Compilación Android: JDK 17, SDK 36, Build Tools 35.0.0, NDK r27c, Rust ARM64. APK de prueba optimizado y firmado con clave local; validación APK Signature v2/v3, alineación ZIP y segmentos ELF a 16 KiB. Incluye licencia de Liberation Sans.

## Resultados reales

| Verificación | Resultado |
|---|---|
| `pnpm check` | Tipado sin errores y compilación de frontend correctos. |
| Privacidad UI escritorio, español e inglés | Aprobación, revisiones, aliases, cambios y bloqueos de filtraciones correctos. |
| Privacidad UI móvil a 412 px, español e inglés | Recorrido correcto con márgenes de 16 px, controles compactos y acciones secundarias mediante iconos. |
| UI de editor, agrupación de aliases y espacio de trabajo | Correcto. |
| Vault UI y marca en escritorio | Correcto. |
| Suite Rust compartida | 124 pruebas correctas, 0 fallidas; 18 pruebas optativas omitidas en la ejecución ordinaria. |
| `cargo clippy --all-targets -- -D warnings` y formato | Correcto en el host. El build Android conserva avisos de código de escritorio desactivado y APIs de sus dependencias. |
| Qwen real en CPU del Mac, perfil móvil | Importación, tres entidades sintéticas, aprobación y PDF sin valores privados. Prueba optativa ejecutada explícitamente. |
| Qwen real en Android, sin red | Perfil 4096/1024, validación de revisión, PDF, cancelación y liberación de modelo correctos mediante JNI/Rust. |
| Documento legal español en Android | Análisis completo; siete categorías: persona, organización, ID, email, teléfono, dirección y referencia procesal. |
| OCR incluido, sin red | Imagen y PDF escaneado seleccionados con SAF, reconocidos por ML Kit y entregados a sesión sin aprobación automática. |
| APK optimizado mediante su UI | Importar PDF escaneado → analizar con Qwen → aprobar → generar PDF → abrir Compartir. No se seleccionó ni envió a ningún destinatario. |
| Residuales en PDF Android | El lector Rust verificó aliases y ausencia de nombres, identificadores, correo, organización y proceso de las dos fuentes sintéticas. |
| Vault Android | Crear, bloquear, contraseña incorrecta, desbloquear, importar/exportar mediante SAF y guardar/restaurar copia cifrada correctos. |
| Ciclo de vida Vault | Bloqueo nativo al pausar la Activity, incluido Compartir. |
| Modelo ausente o malformado | Análisis y aprobación bloqueados; no se presentan como protección terminada. |
| Visor de fuente | Apertura interna, márgenes, controles accesibles, zoom, arrastre y doble toque para ajustar. |
| Orientación de fotos | Dos pruebas instrumentadas Android correctas: los ocho giros/reflejos EXIF conservan los píxeles y el zoom vuelve al ajuste inicial sin reemplazar el bitmap. |
| Muerte del proceso | Prueba de `force-stop` con marcador nativo de un byte, repetida también en el APK optimizado final; sin originales ni mapas recuperados y aviso visible para reimportar. No depende de que WebStorage alcance a escribir. |

## Mediciones observadas, sin equivalencia con el Redmi

El registro nativo del emulador, durante pruebas sintéticas repetidas, observó:

- Verificación SHA-256 del modelo: 2,56–2,76 s.
- Carga posterior a verificación: 0,127–0,137 s. La verificación había calentado la caché del sistema; no es una medición de arranque en frío del teléfono.
- Análisis del documento corto: 9,09–9,20 s. El documento legal con siete categorías tardó 40,51 s incluyendo preparación de su llamada.
- OCR de los fixtures: 0,347–0,394 s en el registro observado.
- Máximo residente del proceso durante varias pruebas: hasta 2.164.604 KiB, aproximadamente 2,06 GiB. Es VmHWM acumulado del proceso; no una asignación de memoria exclusiva de cada etapa.
- Las lecturas de batería y temperatura del emulador no se consideran evidencia térmica del dispositivo objetivo.

Los JSON y PDFs de evidencia están en `artifacts/android-emulator/` local, excluido de Git. El instrumento para el Redmi es `scripts/measure-android.py` y el procedimiento está en [ANDROID.md](ANDROID.md).

## Repetir las comprobaciones

```sh
pnpm check
pnpm test:native
pnpm check:native
pnpm test:privacy:ui
BLUEDOOR_TEST_MOBILE=1 BLUEDOOR_TEST_BROWSER=chromium pnpm test:privacy:ui
```

Las pruebas JNI de `tests/check-android-native.mjs` y `tests/check-android-ocr.mjs` requieren un APK **debug** instalado en un emulador de prueba, su WebView reenviado mediante `adb forward` al puerto 9223 y los fixtures sintéticos seleccionables con SAF. La prueba nativa requiere además el Qwen verificado en el almacenamiento privado de esa instalación. No están habilitadas en el APK de entrega optimizado.

Las pruebas de imagen residen en `app/src/androidTest/` del proyecto Android. Después de construir el APK debug mediante Tauri, se genera su APK de prueba con `:app:assembleUniversalDebugAndroidTest -x rustBuildUniversalDebug` desde Gradle y se ejecuta `com.bluedoor.desktop.test/androidx.test.runner.AndroidJUnitRunner` mediante `adb shell am instrument -w` en el emulador elegido.

## Límites y verificaciones pendientes

- No hubo acceso al Redmi físico. Faltan memoria, latencia, calentamiento, batería, selector/Compartir del fabricante y pruebas sostenidas en ese dispositivo.
- Qwen se copió desde el archivo existente y verificado del Mac para las pruebas del emulador. No se repitió una descarga completa de 1,36 GB desde Android; el flujo de descarga y su verificación nativa están implementados.
- No se indujo un agotamiento real de RAM. Se verificaron fallos de modelo, cancelación y muerte explícita del proceso; queda probar presión de memoria real en el Redmi.
- La geometría de los ocho valores EXIF está probada. Queda una prueba con fotografías de la cámara real, distintas orientaciones y condiciones de iluminación.
- PDF móvil se exporta como documento de texto nuevo; no conserva el diseño gráfico del original. DOCX incluye encabezados, pies y notas, pero sus imágenes incrustadas deben convertirse a PDF para OCR. TIFF depende del decodificador del sistema.
- Documentos: hasta 10 MB y 64 KiB de texto; OCR hasta 30 páginas/60 KiB. Vault móvil: hasta 32 MB por entrada; transferencia de copias cifradas acotada a 4 GB. Superar límites da error, sin truncamiento silencioso.
- Esta entrega ofrece pseudonimización revisada, no una garantía de anonimato ni de detección exhaustiva sin revisión humana.

## Actualización 0.4.1: incidencias del recibo y gestión de modelos

- Se reutiliza `ModelsView` de escritorio, con catálogo Android sin audio: Qwen Q4 y GLiNER INT8. Prueba UI a 412 px: inicio sin modelo → descargar/progreso → Instalado → recargar sin otra descarga → seleccionar GLiNER → borrar → reabrir en Modelos si falta el seleccionado. La descarga real de GLiNER y su tokenizador se completó desde Android con hashes verificados.
- Rust: 125 pruebas correctas, 19 optativas omitidas. Se ejecutó además la comparación del tokenizador GLiNER frente a la referencia Python: IDs y máscara de primeras subpalabras idénticos. El decodificador comprueba dimensiones, valores finitos, límites y devuelve texto exacto, incluidos acentos.
- Recibo sintético común, ambos motores nativos y red desactivada: nombres, banco, ID, email y proceso quedaron sustituidos; aprobación y generación PDF correctas. La cobertura de contactos/identificadores combina el modelo con las validaciones y patrones del núcleo compartido: estos resultados no son una evaluación aislada del modelo GLiNER.
- En el emulador, **debug**: Qwen 19,45 s incluyendo verificación/carga; GLiNER 0,85 s incluyendo verificación, tokenizador e inicio de ONNX. No son cifras del Redmi. Qwen: verificación 3,21 s, carga 1,05 s, análisis 15,13 s. GLiNER: verificación/tokenizador 0,416 s; inferencia e inicio de la sesión ONNX 0,411 s. VmHWM acumulado del proceso 1.750.184 KiB; no permite atribuir RAM a cada motor por separado.
- Pantalla configurada para apagarse a los 2 s: Qwen mantuvo KEEP_SCREEN_ON durante el análisis y completó el recibo. Al mandar la app al fondo se obtuvo la razón explícita, quedó sin aprobar y el reintento completó Qwen.
- PDF escaneado real seleccionado con SAF: vista original rasterizada dentro de la pantalla y Qwen correcto sin la generación previa de corrección OCR (18,62 s en esa ejecución debug del emulador). El PNG sintético y el informe se guardaron en los artefactos locales.
- Icono: versiones adaptativa, redonda y monocromática derivadas de la puerta SVG del Dock; PNGs anteriores a Android 8 derivados del mismo PNG maestro.

Comandos adicionales: `BLUEDOOR_TEST_BROWSER=chromium node tests/run-ui.mjs --android-models` y `node tests/check-android-model-engines.mjs` (este último restringido al emulador sintético `emulator-5554`, con ambos modelos instalados y WebView debug reenviado al puerto 9223). Evidencias: `model-engines-v2.json`, `source-pdf-v2.json` y `background-retry-v2.json` en `artifacts/android-emulator/`.

El APK optimizado también completó el recorrido real con GLiNER mediante la UI: seleccionar el modelo instalado → importar PDF escaneado → analizar → revisar → aprobar → generar PDF → abrir Compartir, sin destinatario. El lector Rust comprobó aliases y ausencia de residuales en `gliner-release-protected-v2.pdf`. Se detectó y corrigió una incompatibilidad de R8 con nombres JNI de ONNX; el script de construcción verifica ahora que `OnnxJavaType`, `OnnxTensor`, `TensorInfo` y `OrtSession` conserven sus nombres. Los cuatro ELF empaquetados pasan alineación de 16 KiB y la firma v2/v3. Una instalación encima de la anterior conservó ambos modelos y la selección, mostrando **Installed/Instalado** tras reabrir.

Qwen también completó el PDF escaneado mediante la UI del APK optimizado 0.4.1, conservando el original visible y mostrando «Pendiente de revisión». Se corrigió además la posición móvil del progreso: permanece dentro del viewport aunque el PDF ocupe varias pantallas, con cancelación interactiva; la prueba UI comprueba ambos puntos.

## Actualización 0.4.2: cero tokens, nombres y disposición de Modelos

Se leyó `tokenizer.chat_template` del GGUF instalado: exige `<|im_start|>`/`<|im_end|>`. El perfil móvil anterior enviaba líneas de rol sin esos delimitadores. Se corrigió ChatML, se conserva el prefijo de pensamiento vacío de la plantilla (`enable_thinking=false`), y la instrucción breve incluye un ejemplo de nombres sueltos y campos de recibo. No cambian los pesos ni sus hashes.

Qwen prepara la entrada en lotes de 64 tokens, informa sus tokens de entrada y después cada token generado. La extracción se acota a 384 tokens de respuesta y termina en cuanto hay un JSON completo con `entities`. Un límite cooperativo de 180 segundos por intento impide esperas indefinidas: se comprueba entre operaciones/batches, sin publicar ni aprobar una salida parcial; una llamada nativa de carga/decode debe retornar antes de comprobarlo. GLiNER usa la fase `ner`, sin un contador de generación ficticio.

GLiNER podía preferir un primer nombre con mayor puntuación y descartar el nombre completo solapado. Ahora se conserva la propuesta completa de persona que ya supera el umbral, sin inventar una extensión. Se añadieron reglas para campos explícitos Ordenante/Beneficiario/Titular; se excluyen etiquetas de cuenta, importes y textos genéricos. Son un complemento del modelo, no una evaluación aislada de su capacidad en español.

La vista móvil muestra la copia con aliases y el número de entidades al terminar, permite volver al original y declara expresamente cuando no hay sustituciones. Modelos: título de 14 px (aproximadamente 60 % del anterior), cancelación en su misma fila; nombre/editar/borrar alineados y loader/porcentaje en la fila inferior. Las pruebas UI verifican esas posiciones a 412 px, estado instalado y persistencia tras recargar.

Resultados: 128 pruebas Rust correctas, 20 optativas omitidas; se ejecutaron además dos pruebas optativas reales de Qwen en CPU del Mac (importación/protección/PDF y recibo con tres nombres), con salida verificada. En emulador ARM64 debug, sin red: ambos motores detectaron los nombres, banco, ID, email y proceso del recibo ampliado y pasaron revisión/PDF; Qwen 11,78 s y GLiNER 0,96 s en esa ejecución. Se comprobaron eventos de prefill y NER. No son mediciones del Redmi.

UI de privacidad móvil/escritorio ES/EN, Modelos móvil, preparación de modelos de escritorio, bienvenida, editor y feedback correctos. La suite UI ampliada se detuvo en una prueba de historial de audio preexistente (`check-history.mjs`, espera de `.boot-detail`); `App.svelte` y el flujo de historial no cambian en esta actualización. No se presenta la suite ampliada como completamente aprobada.

Evidencia local: `model-engines-v3.json`, `artifacts/android-v3-host/workflow.json` y capturas `android-models-downloading-v3.png`/`android-models-installed.png`.
