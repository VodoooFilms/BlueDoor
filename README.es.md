# Belegal _ Local demo

Base independiente de Voice to PDF, copiada del estado local del proyecto el 30 de septiembre de 2026 (commit de referencia `1dbcc88b2092b61e3e98d9c360affc1623cdca92`, incluyendo cambios locales).

Conserva grabación/importación, transcripción local, edición, modelos y exportación PDF. Private AI añade importación local de documentos, OCR local de PDF escaneados e imágenes en macOS, análisis Qwen3, mapa privado editable, sanitización determinista y vista previa sin envíos externos. Ver [PRIVATE_AI.md](PRIVATE_AI.md) para arquitectura, límites y pruebas.

## Desarrollo

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
# macOS: compilar y abrir la app local preparada
pnpm demo:mac
```

`pnpm dev` inicia únicamente la interfaz web; audio, modelos y persistencia requieren Tauri. El empaquetado nativo requiere preparar los recursos de audio según los scripts existentes.

## Identidad visual

Paleta marfil y blanco, bordes cálidos, acentos carbón, cuerpo sans serif de 15–16 px y títulos serif de 20–24 px. Fuentes de sistema locales aproximadas a la captura de Belegal, sin descargas externas.

Los datos se guardan bajo `com.belegal.localdemo`, separados de la aplicación original. No se copiaron transcripciones, credenciales, modelos descargados ni cachés de compilación.

Documentación heredada de la base en los demás archivos Markdown; no implica autorización para publicar este demo ni describe todavía su arquitectura futura.
