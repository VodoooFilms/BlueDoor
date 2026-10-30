# BlueDoor Android — informe de uso y próxima etapa

**Fecha:** 29 de octubre de 2026.

**Versión evaluada:** 0.4.2.

**Rama:** `codex/android-protection`.

**Base de código de la entrega:** `141b6f8`.

**Dispositivo objetivo y de uso reportado:** Redmi 15C, Helio G81-Ultra, 8 GB de RAM física.

## 1. Conclusión

La prueba del usuario en el teléfono confirma que ambos motores funcionan. GLiNER resulta la opción más práctica para el uso móvil por su rapidez; Qwen sigue disponible, con una demora observada de aproximadamente dos minutos por página.

La siguiente prioridad es recuperar el flujo visual de documentos de BlueDoor de escritorio: proteger y editar sobre una representación fiel del PDF, manteniendo su composición gráfica y permitiendo cubrir zonas como códigos QR. El pulido de la interfaz móvil debe acompañar ese trabajo, especialmente la distribución de las herramientas del editor y la claridad de la exportación.

## 2. Resultados comunicados desde el teléfono

| Aspecto | Observación del usuario | Alcance de la conclusión |
| --- | --- | --- |
| GLiNER PII Small INT8 | Funciona y resulta especialmente adecuado para móvil. | Validación funcional y valoración cualitativa de rapidez; no es un benchmark exhaustivo de precisión. |
| Qwen3-1.7B Q4_K_M | Funciona; tarda alrededor de dos minutos por página. | Tiempo aproximado de uso, sin protocolo de medición ni separación de carga, OCR e inferencia. |
| Protección y mapa | El texto se protege y se construye el mapa de entidades. | No demuestra detección exhaustiva de todos los datos de cualquier documento. |
| Resultado visual | Después de proteger se presenta texto protegido, con pérdida de la apariencia original del PDF. | Es la principal diferencia pendiente respecto al flujo de escritorio. |
| Interfaz | Se percibe todavía como una adaptación de prototipo. | Requiere una etapa de diseño y pulido específica para móvil. |
| Exportar PDF | El usuario consulta si está deshabilitado. | Falta confirmar en qué estado del documento se observó; no hay evidencia suficiente para declarar que exportar está roto. |

Estas observaciones corresponden al uso real comunicado por el usuario. Las cifras previas del Mac y del emulador se mantienen separadas y no sustituyen las mediciones del Redmi.

## 3. Estado de la entrega

La versión incluye importación de texto, DOCX, PDF e imágenes; OCR local; selección, descarga y borrado de modelos; detección local; revisión de entidades y aliases; edición del texto protegido; aprobación; exportación PDF y menú Compartir. También incluye Vault cifrado y controles de ciclo de vida móvil. Audio/transcripción se excluyen de Android.

En 0.4.2 se corrigió la plantilla ChatML de Qwen y su instrucción de extracción. Se distingue preparación de entrada de generación de respuesta. GLiNER muestra detección de entidades y evita el contador de generación engañoso. Se corrigió el descarte de apellidos cuando el modelo ya había propuesto un nombre completo y se añadieron detecciones complementarias para campos explícitos de recibos.

La página de Modelos ya tiene título reducido y Cancelar descarga en una misma fila. Cada tarjeta alinea nombre, editar y borrar; el progreso y porcentaje ocupan la fila inferior. La instalación de una actualización encima de la app conserva los modelos; desinstalar elimina normalmente su almacenamiento privado.

## 4. Diferencia principal: edición del PDF con fidelidad visual

El usuario describe el flujo de escritorio así: después de pulsar **Proteger documento**, el documento mantiene sus gráficos y apariencia, y permite editar los textos dentro de esa representación. En sus pruebas con recibos bancarios también utilizaba cuadros blancos para cubrir QR.

Android puede mostrar el PDF original para consulta, pero la protección actual trabaja sobre el texto extraído. La copia protegida y su exportación se reconstruyen como texto; no mantienen la composición gráfica del original. Mostrar la imagen original en un visor no equivale a editar o exportar una versión visualmente protegida.

El código confirma esta separación: en `documents.rs` la extracción de la estructura visual utiliza la integración de macOS; Android no recibe ese objeto de layout. Por ello, la siguiente etapa necesita una integración Android de representación, coordenadas, edición y exportación, además de cambios de interfaz.

### Comportamiento requerido

- Mantener páginas, dimensiones, orientación, distribución y gráficos que no se hayan marcado para retirar.
- Mostrar los aliases en la posición correspondiente a los datos sustituidos y permitir editarlos dentro de la aplicación.
- Vincular las entidades del mapa con su ubicación en la página, también cuando el texto procede de OCR.
- Permitir añadir, mover, redimensionar y eliminar regiones de cobertura, incluyendo un cuadro blanco sobre QR u otras zonas seleccionadas.
- Mantener coherencia entre la vista protegida, los cambios guardados, la revisión aprobada y el PDF exportado.
- Conservar el original como fuente local sin modificarlo.

**Distinción necesaria:** editar textos dentro de BlueDoor no implica que el PDF exportado tenga que conservar objetos de texto editables. La estrategia de salida deberá preservar la apariencia y eliminar los datos protegidos de forma efectiva.

### Protección de regiones y QR

El cuadro blanco debe formar parte de la salida protegida y eliminar realmente los píxeles u objetos cubiertos. Una superposición visual de interfaz no basta. La exportación no debe conservar debajo un QR recuperable ni una capa de texto original con los valores retirados.

El proyecto ya documenta una integración de detección y borrado de QR para macOS. Esa validación no demuestra soporte equivalente en Android. El marcado manual de zonas debe tratarse como un requisito explícito de esta siguiente etapa; la detección automática de QR puede complementarlo.

## 5. Pulido de UX solicitado

### Herramientas del editor

Reorganizar las **tres filas que incluyen fuente, tamaño, formato, alineación y acciones hasta Guardar**. Deben quedar centradas o distribuidas de forma equilibrada, sin escalones accidentales ni agrupaciones pegadas a los bordes.

Criterios:

- Agrupar controles por función y darles una alineación consistente.
- Mantener separaciones regulares entre controles, grupos y bordes.
- Conservar objetivos táctiles cómodos y los nombres accesibles de los iconos.
- Resolver el espacio de Copiar y Guardar sin dejar botones aislados en otra fila.
- Verificar el estado Guardado, los cambios pendientes y los estados deshabilitados.
- Comprobar pantallas estrechas, teclado abierto, zoom y orientación.

La distribución final debe revisarse visualmente en el teléfono; no basta con que los controles técnicamente quepan.

### Exportación PDF

La función está implementada. Actualmente, desde **Revisar y editar**, exige guardar los cambios pendientes y pulsar **Aprobar revisión**. Se bloquea durante operaciones, con cambios sin guardar, sin revisión vigente, con análisis incompleto o con advertencias. Los cambios posteriores invalidan la aprobación y requieren una nueva revisión.

Debe mejorarse la explicación del estado deshabilitado: indicar qué falta para habilitar Exportar, sin obligar al usuario a deducirlo. Si permanece deshabilitado después de cumplir las condiciones, se debe reproducir y corregir como un fallo. Por ahora, la consulta del usuario queda registrada como una comprobación pendiente.

## 6. Orden propuesto para la próxima sesión

1. **Comprobar Exportar en el Redmi:** documentar la secuencia guardar → aprobar → exportar y confirmar si existe un bloqueo indebido. Aclarar el motivo cuando el bloqueo sea correcto.
2. **Definir y probar el motor visual Android:** obtener páginas y posiciones de texto/OCR; reutilizar el núcleo de privacidad y evaluar una salida fiel al original. No dar por resuelta la edición con un simple visor.
3. **Prototipo completo con un recibo sintético:** conservar el aspecto, sustituir nombres en su posición, cubrir un QR y exportar el resultado revisado.
4. **Integrar mapa y edición visual:** seleccionar entidades desde la página, ajustar aliases y mantener la revisión sincronizada con cambios de texto o regiones.
5. **Pulir UX:** reorganizar las tres filas del editor y revisar herramientas, espaciados, teclado, navegación y estados de guardado/exportación.
6. **Validar en el Redmi:** PDF con texto, recibo escaneado, varias páginas y cambios posteriores a la aprobación. Medir el coste adicional de memoria y renderizado.

Este orden es una propuesta de trabajo para la próxima sesión; no constituye una fecha de entrega ni una tarea programada.

## 7. Criterios de aceptación de la siguiente etapa

- Tras proteger un recibo, la vista conserva su composición gráfica y muestra las sustituciones en el lugar adecuado.
- El usuario puede editar un nombre/alias y cubrir un QR desde el teléfono.
- Vista y exportación corresponden a la misma revisión; cualquier cambio vuelve a invalidar la aprobación.
- El PDF exportado conserva páginas y gráficos autorizados, pero no permite recuperar los datos o códigos cubiertos.
- El original permanece intacto.
- Las herramientas del editor quedan distribuidas con claridad y sin desbordamientos.
- Exportar se habilita cuando corresponde y explica el motivo cuando está bloqueado.
- Las pruebas de funcionamiento y rendimiento se identifican por dispositivo; no se atribuyen al Redmi resultados de emulador.

## 8. Evidencia y referencias del proyecto

En la entrega 0.4.2 pasaron 128 pruebas Rust. Se verificaron recorridos de privacidad y modelos, inferencia real con ambos motores, aprobación y PDF usando documentos sintéticos en emulador. La suite UI ampliada tiene una incidencia preexistente en la prueba de historial de audio; no se declara completamente aprobada.

No hay todavía mediciones instrumentadas sostenidas de batería, temperatura o memoria del Redmi. La valoración de GLiNER como opción preferida para móvil se basa en el uso reportado, no en una comparación exhaustiva de precisión.

- [Instalación, arquitectura y límites de Android](ANDROID.md).
- [Validación técnica de las versiones entregadas](ANDROID-VALIDATION.md).
- [Antecedente de protección de QR](QR_PRIVACY_PLAN.md).

El informe registra el estado y los requisitos. No implementa aún la edición visual del PDF ni los cambios adicionales de UX.
