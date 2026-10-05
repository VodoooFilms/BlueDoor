# Cambios de BeLegal

## 0.4.0 — 5 de octubre de 2026

- Limpieza de cachés, temporales y compilaciones antiguas; una sola app actual en Aplicaciones. Eliminadas cinco definiciones frontend sin uso y regeneración automática de entradas QA para las pruebas de layout.

- Ajuste automático al ancho al importar y entrar en Editar, con recálculo al redimensionar la ventana. La opción «Ajustar al ancho» permite volver al modo automático; los porcentajes manuales se conservan al redimensionar. Verificado en ambos visores.

- Los QR detectados localmente se cubren por completo en blanco al proteger, editar y exportar. Solo se conservan sus coordenadas; el original permanece intacto. Prueba nativa de dos páginas sin alias textuales. La detección automática requiere revisión visual; marcado manual pendiente.
- Zoom hasta 800% en Documentos y Editar, con pasos intermedios y disponible durante la edición de un borrador.
- El documento ampliado se desplaza dentro del visor sin ensanchar la app ni sacar Zoom y Exportar de la ventana. Pruebas a 600% y 800%, alineación y redimensionamiento a escala.
- El panel de alias permite recorrer las cajas con flechas anterior/siguiente, contador y número de página. Lleva el visor hasta la caja seleccionada y conserva los aliases pendientes al cambiar de selección.

## Hito — 4 de octubre de 2026: flujo privado y editor estable

Etiqueta: `milestone/2026-10-04-workflow-editor-stable`. Base de aplicación: 0.3.0; hito de código sin instalador nuevo.

- Espacio unificado para documento y mapa privado, Ajustes agrupados y una barra de edición; se retira el editor de PDF duplicado en texto plano.
- Análisis local por tareas más pequeñas, reintentos acotados, listas vacías válidas, caché en memoria y complemento de detección de personas en macOS.
- Edición por párrafos con crecimiento dentro del espacio libre, bordes de ancho/altura y regla de sangrías y márgenes. Se normaliza el espaciado físico del PDF al editar.
- Salida de la caja mediante clic fuera, Escape o Terminar edición. Guardar y salir utiliza el resultado del guardado y muestra errores sin repetir el aviso genérico.
- Correcciones del audit: protección de gráficos entre glifos dentro del párrafo y Deshacer en orden compartido entre texto y dimensiones. Regresiones para separadores, bordes de tabla y cambios intercalados.
- Protección de regiones pequeñas sin bloquear la exportación por una etiqueta ilegible; comprobación nativa de expansión sobre contenido existente.
- Pruebas de recibo y PDF de cinco páginas, cierre real en la app y verificaciones de interfaz, render y exportación.

Detalles, mediciones y límites: [documentación del hito](docs/milestones/2026-10-04-workflow-editor-stable.md). El caché y las sesiones siguen siendo temporales; no hay vault persistente ni redistribución global entre páginas.

## 0.3.0 — 4 de octubre de 2026

- Flujo Importar → Proteger documento → Edit → Exportar PDF. Los PDF e imágenes abren Edit después del análisis.
- Una sola página editable con el procesador de texto existente: fuente, tamaño exacto, negrita, cursiva, subrayado, color, alineación y sangrías. Las correcciones se guardan en su posición y se pueden revisar antes de exportar.
- Conservación de páginas, márgenes, gráficos y contenido fuera de las zonas editadas. El exportador utiliza el mismo render nativo que la vista aplicada y rechaza texto que no cabe.
- Renombrar un alias actualiza todas las entradas y apariciones que lo comparten. Las correcciones y sus estilos sobreviven a esos cambios.
- Coordenadas PDFKit/Vision y cobertura comprobada de los datos privados; salida PDF a partir de píxeles modificados, sin la capa de texto ni enlaces del documento fuente.
- Pruebas del flujo completo, comparación independiente de píxeles y OCR, y ejecución del editor con formato en CI sobre macOS.
- Herramientas de evaluación y postentrenamiento reproducible. El entrenamiento local se interrumpió; no se incluye un modelo nuevo ni se afirma una mejora de detección.

El formato se conserva por bloques con posiciones originales; no hay redistribución automática de párrafos entre páginas. OCR y fuentes de respaldo pueden alterar la fidelidad. Los documentos privados permanecen solo en memoria y requieren revisión humana.
