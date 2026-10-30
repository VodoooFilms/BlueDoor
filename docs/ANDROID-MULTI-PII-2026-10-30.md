# Multi PII predeterminado — 0.5.5 / 5005

Se integra `urchade/gliner_multi_pii-v1`, convertido por ONNX Community, revisión fijada `2e0397a7e8a250d76c37122232b3cbde42c8d629`. Variante elegida: FP16. Modelo 579.717.643 bytes y tokenizador 16.331.948 bytes: 596.049.591 bytes en total. La aplicación descarga los pesos solo al pulsar Descargar; no se incluyen en el APK. Tamaños, hashes y URLs se verifican mediante el manifiesto existente.

## Integración

- Grafo span-level con `span_idx`/`span_mask`, ancho máximo de 12 palabras y salida `[1,palabras,12,8]`. Se valida la forma, longitud y finitud de sus puntuaciones antes de aceptar detecciones. Umbral Multi PII: 0,5; Small conserva 0,35.
- Fragmentos ajustados con el tokenizador real al contexto de 384 tokens, en ambas pasadas, con solapamiento de hasta 12 palabras. Nunca se acepta truncamiento silencioso. Un fallo en un fragmento mantiene las barreras de análisis incompleto.
- El motor Android queda fijado a ONNX Runtime 1.24.3. La variante INT8 probada devolvió puntuaciones insuficientes en Android; no se entrega como opción Multi. También se descartó 1.31.1 para Android tras un SIGILL al cargar el modelo cuantizado en el emulador. FP16 con 1.24.3 pasa la inferencia del nombre completo partido en líneas.
- Se selecciona Multi una vez al actualizar, incluso si antes estaba elegido Qwen. Después se respetan las elecciones explícitas. Si faltan los pesos de Multi se abre Modelos para descargarlos; no se sustituye en silencio por otro. Small y Qwen se conservan. Borrar el seleccionado sigue eligiendo otro instalado.
- El procesamiento sigue siendo local. Se mantienen verificación de spans contra el original, mapa privado, cobertura manual, revisión y bloqueo de exportación.

## Verificación

- Typecheck sin errores ni avisos; build web y APK release.
- Android: 9 pruebas instrumentadas correctas, incluida inferencia real de Multi FP16 con apellido en la segunda línea, OCR, exclusión del backend de telemetría y cobertura manual en PDF.
- 14 documentos sintéticos con pesos reales FP16, decodificador Rust y saneamiento: correctos (host ONNX Runtime 1.31.1).
- Pruebas nativas específicas: 6 de GLiNER y 58 de privacidad correctas; las marcadas ignoradas requieren recursos externos. La suite general tuvo 139 correctas y un fallo variable en el helper macOS de geometría PDF, fuera de la ruta de inferencia Android; no se presenta como suite general completamente aprobada.
- Suite UI de modelos: descarga, progreso, selección predeterminada, actualización, borrado y recuperación correctos.

No se midieron tiempo ni memoria en el Redmi y no se recibió el documento personal del usuario. La prueba del apellido utiliza texto sintético; no garantiza la detección de todo documento real.

## Artefacto final

APK local firmado: `installers/BlueDoor-android-arm64-preview.apk`, 69481140 bytes. SHA-256: `d4f9569a61e8c7a8169d899ff6d4bf230218e22bd22069d3d2654e2a1f69203b`. Versión 0.5.5 / 5005, firma v2/v3, clases JNI y alineación ZIP/ELF de 16 KiB verificados. Instalación por actualización y arranque release correctos en BlueDoorQA. El frontend se incluye comprimido en la biblioteca Tauri, no como archivos JS separados del ZIP.
