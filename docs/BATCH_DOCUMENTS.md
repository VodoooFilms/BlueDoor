# Lotes de documentos

El flujo individual se conserva. El modo lote se activa explícitamente desde
**··· → Detectar varios archivos…**, tanto en la pantalla de entrada como en un
documento abierto.

1. Elegir una carpeta y confirmar los documentos mediante casillas. No se recorren
   subcarpetas. Máximo 20 documentos, 10 MB por archivo y 50 MB por carpeta elegida.
2. Detección local secuencial. El resumen ocupa dos líneas. Un error no detiene el
   resto; **Cancelar pendientes** deja terminar el archivo en curso.
3. Al terminar, abrir Edit con el primer resultado disponible. **Anterior / Ver
   lista / Siguiente** aparece en una franja inferior de dos líneas y omite archivos
   fallidos o cancelados. Los borradores sin guardar bloquean el cambio de archivo.
4. Revisar y aprobar cada documento por separado. La detección nunca aprueba.
5. **··· → Exportar aprobados a…** muestra el resumen y abre el selector nativo de
   carpeta. Solo exporta sesiones aprobadas, aplicando las mismas validaciones
   nativas que el exportador individual. Cada PDF recibe un nombre nuevo; no se
   sobrescriben archivos existentes.
6. **Cerrar lote** pide confirmación y libera sus sesiones. Se restaura el documento
   que estaba abierto antes de empezar, si lo había.

## Alias compartidos

Las coincidencias exactas (mismo valor y categoría) reciben el mismo alias en todo
el lote; entidades distintas no reutilizan los números. En Mapa privado,
**Alias del lote** abre un panel con búsqueda, variantes y número de documentos.
Las formas abreviadas o nombres parecidos se proponen como posibles variantes,
pero solo una confirmación explícita las une. Compartir apellido no prueba identidad.

Editar un alias en el mapa individual también afecta a sus coincidencias en el
lote. Antes de aplicar un cambio compartido se muestran los documentos afectados
y cuántos perderán su aprobación. Los cambios son atómicos: se validan todos antes
de modificar ninguna sesión. Se conservan las correcciones durante cambios de
alias; los cambios estructurales de mapa que reconstruyen el texto muestran un
aviso específico antes de confirmar. Los PDF ya exportados no se modifican y deben
volver a exportarse después de revisar. La correspondencia se elimina al cerrar
el lote y nunca se reutiliza entre lotes.

## Privacidad y límites

Las sesiones, sus páginas, alias, revisiones y permisos de origen permanecen en
memoria nativa. No se serializan documentos en localStorage ni en logs. Cerrar el
proceso pierde el lote; esto se indica antes de empezar. Los originales no se
modifican. Los temporales de importación/exportación de Android se eliminan después
de usarlos. Se limita a 128 MiB la estimación de imágenes, glifos y texto retenidos
por el lote; un documento que supera el presupuesto queda con error.

Desktop usa el selector nativo de carpetas y rechaza enlaces simbólicos. Android
usa Storage Access Framework y acceso temporal a las carpetas elegidas; no requiere
acceso general al almacenamiento. La compilación Kotlin y Rust ARM64 se verifica,
pero el selector SAF necesita además una prueba manual en teléfono antes de dar
por verificada la interacción con todos los proveedores de archivos.

La primera versión termina la detección antes de permitir la revisión. Puede
reanudar archivos pendientes si se vuelve a cargar la vista mientras el proceso
nativo sigue vivo. TXT/MD/DOCX se exportan como PDF de texto; PDF e imágenes con
layout conservado usan sus páginas y correcciones nativas.

## Verificación

- `pnpm typecheck`
- `pnpm build`
- `pnpm test:batch:ui` (ES/EN, escritorio/móvil, texto/layout)
- `pnpm test:privacy:ui` (regresión del documento individual)
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Las pruebas de interfaz ejercitan la confirmación, exclusión, secuencialidad,
continuidad ante errores, borradores, alias compartidos, uniones explícitas, invalidación de aprobaciones, aprobación manual,
exportación limitada a aprobados, cancelación de pendientes y cierre del lote.
