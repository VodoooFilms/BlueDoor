# Protección de códigos QR: evaluación

4 de octubre de 2026. Estado: primera integración automática implementada y verificada en código; pendiente de activar en la app instalada.

Los QR detectados por Vision se guardan como coordenadas inmutables, sin payload. La vista original conserva el QR. Una vez protegido, el render nativo cubre toda la región con blanco opaco y margen, después de cualquier edición. El PDF contiene únicamente los píxeles modificados. Un fallo de detección detiene la importación.

Regresión nativa: dos páginas con QR ficticios, sin alias textuales; se verifica detección inicial, ausencia en preview y export, conservación del archivo fuente y activación solo en la sesión protegida.

Validación local adicional del render integrado: un recibo (1 QR) y un certificado de cinco páginas (6 QR); se comprobó blanco RGB 255 en toda cada región interior y ningún QR detectable en el PDF resultante. Los archivos privados de prueba quedan excluidos de Git.

Pendiente: marcado manual, pruebas de escaneos difíciles y lector independiente. Un resultado vacío no garantiza ausencia de QR. Los archivos ya importados deben volver a importarse con la versión nueva para ejecutar la detección.

## Problema

La sanitización del texto no elimina la información codificada en un QR ni
el acceso a un documento original mediante su enlace. El render anterior
conservaba las imágenes ajenas a las entidades textuales; un QR queda intacto.
El PDF rasterizado sigue conteniendo el patrón escaneable.

## Decisión recomendada

Detectar los QR localmente por página y eliminarlos completamente al proteger
el documento, con un pequeño margen adicional de cobertura. Sustituir los
píxeles del código con una zona opaca; opcionalmente mostrar «QR protegido».
Aplicar la misma operación en el render de la vista protegida y el PDF
exportado. No usar únicamente blur, pixelado o daños parciales: la corrección
de errores del formato permite recuperar códigos dañados.

No hace falta abrir enlaces, clasificar el contenido del QR con Qwen ni guardar
su payload. Por defecto tratar todo QR como contenido privado. Conservar el
archivo fuente y su versión original localmente como hasta ahora.

## Prueba local

Se creó un prototipo Swift temporal usando `VNDetectBarcodesRequest`, limitado
a la simbología QR. No se imprimió ni persistió el contenido codificado; no se
realizaron solicitudes a sus enlaces. Se volvió a ejecutar la detección sobre
los píxeles con las zonas completamente cubiertas.

| Documento | QR detectados | Detectados después del borrado |
| --- | ---: | ---: |
| Recibo de una página | 1 | 0 |
| Certificado de cinco páginas | 6 | 0 |

El prototipo y sus logs permanecen en `tmp/pdfs/qr-*`, excluidos de Git. Los
datos privados y sus imágenes no forman parte de esta documentación. Este
resultado con un detector y dos documentos no garantiza detección completa.

## Integración propuesta

1. Guardar tipo y región de los códigos detectados en las páginas inmutables
   del documento, con coordenadas convertidas al mismo sistema que PDF/OCR.
   Ejecutar la detección una vez al importar y reutilizar esas regiones.
2. Mantener estas regiones separadas de las entidades textuales; mostrarlas
   en la revisión como `QR_01`, página y estado protegido, sin payload.
3. Calcular el borrado en Rust y ejecutarlo en el render nativo después de las
   correcciones. No depender de un recuadro CSS ni copiar objetos originales
   al PDF. Ninguna edición o cambio de alias debe restaurar el código.
4. Incorporar selección manual de región para códigos no detectados. Un
   resultado vacío de detección no demuestra que el documento no tenga QR.
5. Rechazar la protección/exportación si falla el componente de detección o
   el borrado. Antes de exportar verificar cobertura de regiones y volver a
   buscar códigos en la salida, manteniendo la revisión visual.
6. Probar múltiples QR, color, rotación, perspectiva, tamaño reducido,
   compresión, páginas múltiples, QR incrustados en PDF y escaneos. Verificar
   píxeles y usar un lector independiente para complementar Vision.

## Dificultad y alcance

- Detección macOS: baja a media. El proyecto ya incluye Vision, PDFKit y
  render de páginas; el prototipo no necesita modelos ni servicios nuevos.
- Integración consistente en sesión, revisión, edición y exportación: media.
  Se puede reutilizar la arquitectura de borrado de píxeles, con nuevas
  regiones que no dependan de los alias textuales.
- Cobertura de imágenes deterioradas y herramientas manuales: media a alta.
  Requiere pruebas de omisiones y de cobertura, no solo un test de lectura.
- Fuera de macOS hará falta elegir y verificar un detector equivalente;
  esta prueba no valida Windows ni Linux.

Es una corrección de privacidad prioritaria. Las exportaciones anteriores
con QR intactos siguen exponiendo el contenido o acceso que codifican;
una actualización futura no modifica los archivos ya exportados.

Fuentes técnicas:

- [Apple Vision: detección de códigos](https://developer.apple.com/documentation/vision/vndetectbarcodesrequest).
- [DENSO WAVE: corrección de errores del QR](https://www.qrcode.com/en/about/error_correction.html).
