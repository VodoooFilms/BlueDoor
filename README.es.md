# Belegal _ Local demo

Base independiente de Voice to PDF, copiada del estado local del proyecto el 30 de septiembre de 2026 (commit de referencia `1dbcc88b2092b61e3e98d9c360affc1623cdca92`, incluyendo cambios locales).

Conserva grabación/importación, transcripción local, edición, modelos y exportación PDF. Private Legal añade importación local de documentos, OCR local de PDF escaneados e imágenes en macOS, análisis Qwen3, mapa privado editable, sanitización determinista y vista previa sin envíos externos. Ver [PRIVATE_AI.md](PRIVATE_AI.md) para arquitectura, límites y pruebas.

## Instalador local para macOS

El DMG para Apple Silicon incluye la app y un acceso a Aplicaciones. Abre el DMG y arrastra **Belegal _ Local demo.app** a **Aplicaciones**. Requiere macOS 13.4 o posterior. Los modelos se descargan desde Models / Modelos y se conservan en los datos locales de la app; no se incluyen en el instalador.

La versión local está firmada ad hoc y **no está notarizada por Apple**. Para distribuir una versión notarizada, usa `pnpm release:mac` con un certificado Developer ID y credenciales de notarización.

Para empaquetar una app ya construida y verificada, sin recompilar dependencias:

```sh
python3 scripts/package-local-installer.py \
  --app "/ruta/Belegal _ Local demo.app" \
  --output "/ruta/BeLegal-local.dmg"
```

El script verifica la app, genera el DMG, comprueba su integridad y escribe su SHA-256. El archivo de salida debe ser nuevo. Los instaladores, modelos, cachés y datos de usuario quedan fuera de Git.

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
