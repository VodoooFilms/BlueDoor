# Ajustes UX Android — 30 de octubre de 2026

Cambios preparados sin construir ni sustituir el APK:

- Marca de cabecera: texto a 18 px e icono a 21 px de alto, con ancho proporcional al canon 211/381. Se conserva la relación del conjunto de escritorio (icono 42 / texto 36), incluido espaciado y tracking escalados. Márgenes interiores 28 px arriba, 24 px a los lados y 32 px abajo.
- MainActivity reserva los insets nativos de barras del sistema y recortes de pantalla alrededor del WebView completo, incluidos sus diálogos; se recalculan al girar. La bienvenida mantiene una X de 44 px dentro de esa zona.
- Estilos móviles también en horizontal. Navegación horizontal compacta.
- Zoom junto al nombre; se mantiene pinch zoom. Menú anclado a la izquierda de su botón para evitar salirse de pantalla. Importar otro / Editar en una fila.
- Ver original debajo del documento, a todo el ancho y sin caja de resultado ni contador adicional.
- GPT visible junto al icono. Se mantiene su comportamiento de abrir ChatGPT, sin enviar automáticamente documentos.
- Se eliminan avisos de sesión anterior y de estimación OCR. Permanecen errores de análisis, ausencia de sustituciones, desbordamiento y bloqueos de exportación.
- Si el modelo seleccionado no está instalado y hay otro instalado, se selecciona y persiste este último, tanto al actualizar Modelos como al comprobar disponibilidad al iniciar.

Validación: typecheck, build web, suite de maquetación (incluido pinch), selección de modelos Android y suite visual móvil. Compilación Kotlin correcta. Las capturas `artifacts/ux-*.png` usan el navegador con comandos nativos simulados y documentos de prueba, sin barras Android. Falta verificar los insets con teléfono real; no se construyó APK de estos cambios.

## Compartir con ChatGPT

El exportador Android existente usa ACTION_SEND, MIME application/pdf, EXTRA_STREAM, URI FileProvider y permiso temporal de lectura, a través del selector del sistema. Solo se comparte el resultado exportado por las barreras de revisión nativas. Si ChatGPT acepta este tipo de intent y está instalado, podrá elegirse como destino; falta verificarlo en el teléfono. No se ha verificado un enlace documentado para adjuntar automáticamente a un chat concreto.

La documentación oficial confirma la adjunción de PDF: https://learn.chatgpt.com/docs/use-chatgpt . No establece el contrato de recepción ACTION_SEND de la app Android. No se automatiza la subida ni el envío a una conversación.

## APK con los ajustes

Generado a petición del usuario: 0.5.1 / 5001, ARM64, firma local preview. Archivo `installers/BlueDoor-android-arm64-preview.apk`, 62915252 bytes. SHA-256: `e0770e32bec6111e9e04d90c5b9d4c4d09d26a8204f8df4dfae90b07e023101a`. Typecheck y build correctos; firma, versión, alineación ZIP/ELF de 16 KiB y clases JNI ONNX verificadas por el script. Sin prueba en teléfono conectado en esta construcción.

## Fondo de barras — 0.5.2 / 5002

Fondo nativo azul #1248BC en ventana y contenedor, barras transparentes e iconos claros; contraste automático desactivado en API 29+ para evitar franjas ajenas a la marca. Temas claro y oscuro usan el mismo fondo, también al arrancar. Se conserva el cálculo de insets y su padding. Typecheck y construcción release correctos; firma y alineación ZIP/ELF verificadas. APK: `installers/BlueDoor-android-arm64-preview.apk`, SHA-256 `abca6a550f2bf7ef225ef19f010128e91c83da8df1127379ac7934cba56f3eb5`. Pendiente comprobar los colores de las barras en teléfono real.
