# Hito: flujo privado y editor de PDF estable

Fecha: 4 de octubre de 2026. Etiqueta: `milestone/2026-10-04-workflow-editor-stable`.

La [auditoría previa y sus correcciones](2026-10-04-audit.md) documentan los
problemas encontrados en conservación de gráficos y orden de Deshacer, las
regresiones añadidas y los límites restantes.

Este hito conserva el flujo Importar → Proteger → Editar documento y mapa privado → Revisar → Exportar. El usuario confirmó el funcionamiento con un recibo bancario y un PDF de cinco páginas. El código continúa sobre la base 0.3.0; la etiqueta identifica este avance sin anunciar un instalador nuevo.

## Interfaz y edición

- Documentos, Grabar y Editar son las acciones principales. Modelos, idioma, apariencia, pantalla completa y acerca de se agrupan en Ajustes.
- El documento y el mapa privado comparten el mismo espacio de trabajo. La bienvenida se oculta al trabajar con un archivo; se elimina la ruta duplicada al editor de texto plano con comparación dividida.
- PDF e imágenes se editan sobre sus páginas originales. TXT y DOCX utilizan el editor con formato integrado.
- Fuente, tamaño, estilos, color, alineación, restablecer, deshacer, copiar, guardar y terminar edición comparten la barra del editor.
- Deshacer respeta el orden compartido entre cambios de texto y dimensiones; Restablecer limpia ambos historiales.
- Las líneas próximas se agrupan conservadoramente en párrafos. Celdas de tablas, columnas y elementos de listas conservan áreas independientes cuando su geometría permite distinguirlos.
- El texto se distribuye al escribir y admite párrafos nuevos dentro del espacio disponible. Los bordes laterales ajustan el ancho, el borde inferior ajusta la altura y la regla controla sangrías y márgenes internos.
- Tabulaciones, retornos y espacios no separables usados para justificar el PDF se normalizan únicamente en el texto editable. El archivo fuente y sus coordenadas no se modifican.
- Un clic fuera, Escape o Terminar edición permiten salir del párrafo. Los cambios pendientes ofrecen guardar y salir, descartar o continuar. El cierre usa el resultado confirmado del guardado; si falla, el diálogo muestra el error y mantiene el borrador accesible.
- El render nativo rechaza texto incompleto, superposición con otras correcciones y expansión sobre gráficos o bordes sin coordenadas de texto. La vista aplicada y el PDF exportado usan el mismo render.
- Antes de borrar el rectángulo original, comprueba también el contenido entre los glifos: un separador o borde dentro del párrafo provoca un rechazo visible del guardado, conservando el borrador. No reconstruye tablas o imágenes; se mantiene la revisión visual de fondos complejos.
- Alias automáticos demasiado largos se compactan cuando es necesario. Si una región es demasiado pequeña para una etiqueta legible, se borran los píxeles privados y su referencia permanece en el mapa; la etiqueta no bloquea la exportación.

## Análisis local y capacidad

El fallo con documentos largos combinaba respuestas incompletas, reintentos excesivos y listas vacías válidas tratadas como errores. Se reducen las tareas iniciales de 4.000 a 2.000 caracteres, se preserva solapamiento al dividirlas y se informa el avance real. Las respuestas de extracción tienen un límite de 768 tokens; una salida incompleta se divide y vuelve a analizar, sin aprobarla silenciosamente.

El caché acotado en memoria conserva sugerencias estructuradas exitosas y divisiones necesarias. No evita la validación contra el original ni los controles de exportación. Se limpia al cerrar explícitamente el documento, cambiar el modelo o terminar la app. No es un vault ni un historial persistente.

En macOS, NaturalLanguage complementa las reglas y Qwen con detección local de personas. Sus resultados también se contrastan con el original. Este complemento no se utiliza como detector adicional de organizaciones; no hay un modelo nuevo entrenado.

Mediciones locales de un PDF de cinco páginas, en el equipo de desarrollo:

| Medición | Resultado |
| --- | --- |
| Análisis inicial antes de las correcciones | 165,11 s; cuatro fragmentos fallidos |
| Análisis con las correcciones y solapamiento | 61,10 s; cero fragmentos fallidos |
| Dos repeticiones con caché | 0,15 s cada una; cero generaciones nuevas |
| Render y cobertura revisada | Cinco páginas; nueve nombres de referencia cubiertos |

Los tiempos corresponden al motor de análisis, no al arranque, importación y render completos. Son resultados de un documento y un equipo concretos, no una garantía general de rendimiento o de detección completa. Los documentos y resultados privados usados para medir permanecen fuera de Git.

## Verificación

- `pnpm typecheck`: cero errores y advertencias; `pnpm build`: correcto.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` y Clippy con advertencias como errores.
- Suite nativa: 100 pruebas correctas y 15 omitidas. Las omitidas requieren recursos o datos locales y no cuentan como verificadas por esta ejecución.
- Regresión nativa de gráficos interiores: separador, borde de tabla y separador gris claro rechazados en vista aplicada y exportación; PDF fuente intacto.
- `pnpm test:layout:ui`: importar, proteger, editar, cambiar dimensiones, guardar/reabrir, salir del párrafo, error de guardado visible, vista previa demorada, alias compartidos y exportación revisada.
- `pnpm test:workspace:ui`: espacio unificado y editor en español e inglés.
- Verificación independiente de píxeles, geometría, tabla/ilustración intactas, vista aplicada igual al PDF y ausencia de la capa de texto original en la salida.
- App macOS empaquetada con recursos locales y firma ad hoc comprobada.
- Prueba manual en la app instalada: una corrección pequeña en el PDF de cinco páginas, clic fuera y Save and close. La caja se cerró; se retiró el texto de prueba y se repitió el guardado y cierre.
- Después del audit se recompiló, verificó e instaló la app corregida en Applications. Se importó y protegió la fixture ficticia de párrafos, se guardó una corrección con Save and close y se comprobó que la caja desaparecía. Se cerró esa sesión de prueba y la app quedó en la pantalla de importación.

La suite UI usa mocks del IPC; las pruebas nativas y la comprobación manual complementan esa cobertura.

## Límites y próximos pasos

- No hay redistribución automática del resto de la página ni entre páginas. El crecimiento se limita al espacio libre comprobado; quitar todas las áreas de edición exigiría otro modelo de documento.
- El ajuste geométrico no equivale a cambiar los márgenes globales de todo el PDF. La agrupación es conservadora y puede dejar líneas sueltas.
- OCR, fuentes y fondos complejos pueden reducir la fidelidad. La revisión humana de cobertura sigue siendo necesaria.
- Los documentos privados y sus cambios viven en memoria. Falta una biblioteca/vault local persistente y recuperación de sesiones después de cerrar.
- No se publica en este hito un DMG nuevo ni se modifica el repositorio original de Voice to PDF.

## Retomar desde otra máquina

```sh
git clone https://github.com/VodoooFilms/belegal.git
cd belegal
git switch --detach milestone/2026-10-04-workflow-editor-stable
pnpm install --frozen-lockfile
pnpm demo:mac
```

Para continuar trabajando, crear una rama desde esa etiqueta. El empaquetado requiere las dependencias nativas descritas en la documentación del proyecto; los pesos de los modelos se descargan o instalan por separado y no se incluyen en Git.
