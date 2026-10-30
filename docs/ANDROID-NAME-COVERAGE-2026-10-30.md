# Cobertura de nombres y apellidos — Android 0.5.3

GLiNER no recibe instrucciones conversacionales. Se conservaron las etiquetas `person` / `person name`: el experimento con `full person name including all given names and surnames` perdió detecciones en mayúsculas y no se incorporó. Tampoco se incorporaron las otras cuatro etiquetas alternativas evaluadas.

El refuerzo está en la validación compartida de protección:

- Nombres detectados que ocupan una línea independiente pueden completarse con una continuación corta en la línea siguiente. Se exige un ancla PERSON en una primera línea de al menos dos palabras, texto de apariencia nominal y un máximo total de ocho palabras. No hay lista de apellidos ni dependencia de una plantilla.
- Líneas vacías, tabulaciones, rótulos, cifras, palabras funcionales o de campos y entidades detectadas en la continuación impiden esta unión. Otra persona ya detectada sigue siendo una entidad independiente. Estos límites reducen falsas uniones, no prueban identidad: una segunda persona no detectada puede resultar ambigua y requiere revisión.
- Campos explícitos de apellidos paternos/maternos, primer/segundo apellido, nombre completo y sus equivalentes ingleses se cubren aunque el modelo no proponga PERSON. Se admite un apellido solo y nombres con conectores, guiones, apóstrofos y acentos.
- Se conserva exactamente la escritura de la fuente, incluidos los saltos de línea; el saneamiento y las coordenadas usan los mismos spans. No se reconstruyen palabras ausentes del OCR.

Validación: 138 pruebas nativas activas correctas (21 ignoradas por requisitos externos), más regresión explícita contra el ONNX INT8 y tokenizador locales: 14 documentos sintéticos pasan. Incluyen VALDEZ como uno de varios apellidos, compuestos, mayúsculas, minúsculas en campos, dos personas y prosa/etiquetas preservadas. Nuevas pruebas negativas comprueban límites entre campos, líneas vacías y personas detectadas. Typecheck y build web correctos.

Los documentos de prueba son sintéticos, no el documento del usuario. Este refuerzo no garantiza que GLiNER reconozca todos los datos de cualquier documento. La revisión y las barreras de exportación se mantienen.

APK ARM64 0.5.3 / 5003 generado con firma local preview. Versión, firma y alineación ZIP/ELF de 16 KiB verificadas. SHA-256: `b096dbcdd90a14b9b51f5b87657086dd5f0cfa3480ea2cf4cac6e0e95b4774d6`. No probado en el teléfono del usuario.
