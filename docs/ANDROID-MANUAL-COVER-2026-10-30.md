# Android: cobertura manual y evaluación de detección

El usuario confirmó que 0.5.3 todavía deja visible el apellido en una segunda línea. Sus medidas: GLiNER ≈7 s, Qwen ≈2 min y sí lo cubre. No hay muestra del documento ni texto OCR capturado; no se considera resuelta la omisión automática con las regresiones sintéticas anteriores.

## Cobertura manual

En Editar → Documento, `Cubrir zona manualmente` activa una superficie para arrastrar un rectángulo. También admite selección por teclado y ajuste numérico accesible. `Guardar cobertura` valida página y coordenadas en Rust, amplía a palabras OCR intersectadas, renderiza antes de guardar, compara id/revisión y revoca la aprobación. Funciona aunque el OCR no haya reconocido ninguna palabra en la región. `Deshacer última cobertura` actúa por página y también revoca la aprobación.

Las coberturas persisten en el documento nativo de la sesión, no en CSS ni solo en la vista. Android las aplica con blanco opaco al final, después de alias y ediciones; vista previa y exportación usan la misma operación. Se comprueban los píxeles antes de aceptar la salida. Los spans OCR cubiertos se eliminan también del texto protegido alternativo. Si una cobertura intersecta un párrafo ya editado, su representación alternativa completa se sustituye por `[CUBIERTO]`, al no haber geometría fiable por palabra de la edición. El PDF con maquetación sigue siendo raster. El estado no se guarda fuera de la sesión salvo exportación/Vault explícitos.

Se conserva el bloqueo de análisis incompleto y mapa vacío: dibujar una cobertura no declara automáticamente protegido todo el documento. Se mantiene pinch zoom fuera del modo de cobertura.

## UX

- Volver de Modelos queda en el pie inferior derecho.
- Elegir idioma o una acción del documento cierra su menú. El selector de modelos ya se cerraba al elegir; se conserva y se verifica con sus pruebas.

## Modelo intermedio

Candidato a evaluar: [urchade/gliner_multi_pii-v1](https://huggingface.co/urchade/gliner_multi_pii-v1/raw/main/README.md), PII multilingüe con español declarado y licencia Apache-2.0. El repositorio original tiene pesos PyTorch de 1,16 GB; no es el tamaño de una futura variante INT8. Necesita exportación/compatibilidad ONNX, verificación de offsets, memoria y medición en el teléfono; no se puede introducir como sustitución directa del grafo actual. No se ha descargado ni instalado como nuevo modelo de usuario.

[GLiNER PII Base](https://huggingface.co/knowledgator/gliner-pii-base-v1.0) ofrece ONNX y mayor capacidad dentro de la familia actual, pero declara inglés; aumentar tamaño por sí solo no prueba que resuelva apellidos españoles partidos en líneas.

No se prometen tiempos ni mejor precisión sin medir con un conjunto de documentos sintéticos y representativos. La cobertura manual resuelve la corrección del resultado sin depender de sustituir el modelo.

## Validación y artefacto

Versión 0.5.4 / 5004. 140 pruebas nativas correctas, 21 ignoradas por recursos externos; ocho pruebas instrumentadas Android correctas (debug firmado localmente), incluida cobertura sin OCR y comprobación de píxeles del PDF. Suites de maquetación, modelos y UX móvil correctas; esta última verifica arrastre, coordenadas accesibles, guardar/deshacer y aprobación invalidada. Typecheck sin errores ni avisos y build release correctos. Firma, versión y alineación ZIP/ELF verificadas. Instalación por actualización y arranque release correctos en BlueDoorQA, sin errores en el buffer de crash al comprobar. No es una prueba en Redmi ni una reproducción del documento del usuario.

APK: `installers/BlueDoor-android-arm64-preview.apk`. SHA-256 `a25589d31fd54565c97d01fbae2bfdde9ba469ffecfff02ae536e0d4329b3f7e`.
