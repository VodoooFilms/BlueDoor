# Cambios de BeLegal

## 0.3.0 — 4 de octubre de 2026

- Flujo Importar → Proteger documento → Edit → Exportar PDF. Los PDF e imágenes abren Edit después del análisis.
- Una sola página editable con el procesador de texto existente: fuente, tamaño exacto, negrita, cursiva, subrayado, color, alineación y sangrías. Las correcciones se guardan en su posición y se pueden revisar antes de exportar.
- Conservación de páginas, márgenes, gráficos y contenido fuera de las zonas editadas. El exportador utiliza el mismo render nativo que la vista aplicada y rechaza texto que no cabe.
- Renombrar un alias actualiza todas las entradas y apariciones que lo comparten. Las correcciones y sus estilos sobreviven a esos cambios.
- Coordenadas PDFKit/Vision y cobertura comprobada de los datos privados; salida PDF a partir de píxeles modificados, sin la capa de texto ni enlaces del documento fuente.
- Pruebas del flujo completo, comparación independiente de píxeles y OCR, y ejecución del editor con formato en CI sobre macOS.
- Herramientas de evaluación y postentrenamiento reproducible. El entrenamiento local se interrumpió; no se incluye un modelo nuevo ni se afirma una mejora de detección.

El formato se conserva por bloques con posiciones originales; no hay redistribución automática de párrafos entre páginas. OCR y fuentes de respaldo pueden alterar la fidelidad. Los documentos privados permanecen solo en memoria y requieren revisión humana.
