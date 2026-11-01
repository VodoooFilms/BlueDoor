# Android 0.5.7: cobertura de detecciones cruzadas

Una prueba local con una imagen proporcionada por el usuario reprodujo la fuga de un apellido en la segunda línea. El OCR leyó la palabra y GLiNER Small INT8 la incluyó en una detección de persona. Otra detección, clasificada como dirección, terminaba en la primera palabra del nombre. El resolvedor descartaba la persona completa por el cruce; además, la selección de coincidencias de texto y formato podía omitir la cola de una coincidencia cruzada.

Corrección:

- GLiNER conserva las detecciones parcialmente cruzadas; descarta únicamente alternativas completamente cubiertas y mantiene las propuestas más amplias que aportan cobertura. Las personas que absorben etiquetas de campos se rechazan.
- Texto saneado y rectángulos usan una función compartida de cobertura: cada coincidencia contribuye la cola que aún no estaba cubierta. Una coincidencia que comienza antes del cursor no puede descartar bytes posteriores al cursor.
- Las pruebas de modelo real aplican también el resolvedor de solapamientos usado por Android. Antes verificaban decodificación y saneamiento sin ese paso, por lo que no reproducían esta fuga.
- Pruebas sintéticas de regresión para categorías cruzadas y cola multilínea, incluida estabilidad al invertir el orden del mapa.
- Pruebas locales optativas permiten extraer OCR y renderizar planes en el emulador sin incluir imágenes ni texto personal en el repositorio.

Verificación de la imagen local: OCR real de ML Kit en Android; inferencia de los pesos Small INT8 en ONNX Runtime CPU del host; resolvedor, mapa, saneamiento y rectángulos Rust; renderizado Android del plan. Todas las palabras del nombre esperado tienen cobertura de coordenadas; inspección visual confirma el apellido tapado. No equivale a una prueba integral de inferencia JNI/UI en el teléfono. Persisten errores de clasificación y otras entidades no detectadas: este cambio corrige cobertura, no entrena el modelo.

Limitación registrada en la batería sintética: el modelo clasifica «Otra sección» como organización en un caso de nombre en tres líneas. La nueva selección conserva esa cobertura en lugar de ocultar accidentalmente la detección por solapamiento. Ese fixture comprueba cobertura del nombre y no preservación de esa frase; no se afirma precisión perfecta ni ausencia de falsos positivos.
