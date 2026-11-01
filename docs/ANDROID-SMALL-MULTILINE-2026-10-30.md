# Android 0.5.6: pequeño por defecto y nombres multilínea

- Modelo predeterminado: `gliner-pii-small-int8` (82,7 MB + tokenizador 3,6 MB). La actualización lo selecciona una sola vez; las selecciones posteriores se conservan.
- Se mantienen las dos pasadas `person` / `person name`. La alternativa `full person name including surnames` falló el caso sintético `CARLOS ANDRÉS MENDOZA VALDEZ`, mientras la configuración actual pasó los 14 casos existentes. No se distribuye esa etiqueta experimental.
- La recuperación de nombres ya detectados admite hasta cuatro líneas contiguas y ocho palabras. Requiere un nombre inicial anclado y continuaciones cortas con aspecto de nombre. Se detiene ante líneas vacías, etiquetas, tabulaciones, entidades separadas y palabras reservadas. No inventa letras ni corrige apellidos.
- OCR de texto y de formato original separa bloques de ML Kit con líneas vacías, conserva las líneas internas y calcula las posiciones de glifos sobre el texto resultante. No se aplana toda la página ni se mezclan columnas mediante una unión global.
- Progreso de descarga y tamaños de Small/Multi independientes del modelo predeterminado.

Validación local: typecheck, build, flujo UI Android de modelos, suite de privacidad y 16 casos de nombres con ONNX INT8 real (incluye nombres en tres líneas). La prueba del modelo alimenta salidas ONNX reales al decodificador y al saneamiento Rust. No es una medición en teléfono ni una prueba OCR de imágenes reales; queda pendiente comprobar los documentos del usuario en el dispositivo.
