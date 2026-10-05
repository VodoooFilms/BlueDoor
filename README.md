<p align="center">
  <img src="src/assets/belegal-mark.png" alt="Logo de BeLegal" width="150" />
</p>

<h1 align="center">BeLegal</h1>

<p align="center">
  <strong>Prepara y protege documentos y audio localmente, de manera privada, antes de compartirlos.</strong>
</p>

BeLegal es una aplicación de escritorio para preparar documentos legales y transcripciones en tu propio equipo. Permite detectar información sensible, sustituirla por alias, revisar el resultado y editar una versión para compartir o exportar a PDF.

El propósito es darte control sobre qué información sale de tu equipo antes de enviarla a un servidor, una herramienta de IA o un destinatario. **Esta versión procesa los documentos y el audio localmente; no integra envíos a servicios de IA ni sincronización en la nube.**

La versión de código actual es **0.4.0**: protección de QR, ajuste automático al ancho, zoom hasta 800%, navegación entre alias y limpieza del proyecto. Incluye el [hito del editor estable](docs/milestones/2026-10-04-workflow-editor-stable.md). El instalador disponible más abajo corresponde a 0.3.0; esta versión de código no publica un instalador nuevo.

## Qué puedes hacer

| Área | Función |
| --- | --- |
| **Private Legal** | Importar documentos o pegar texto, analizar entidades con Qwen local y sustituir datos detectados por alias. |
| **OCR local** | Extraer texto de PDF escaneados e imágenes en macOS, con ajustes conservadores para algunos errores de reconocimiento. |
| **Mapa privado** | Revisar y corregir valores, alias y categorías, y agrupar variantes de una misma entidad. |
| **Record / Grabar** | Grabar o importar audio y transcribirlo con Whisper o Parakeet locales. |
| **Edit / Editar** | Ajustar texto, formato y márgenes, y exportar un PDF con fecha y hora en el nombre del documento sanitizado. |
| **Models / Modelos** | Elegir, descargar y eliminar modelos, con setup de permisos y descarga secuencial de los tres modelos predeterminados. |

La identificación de hablantes requiere el modelo de diarización y un motor compatible configurado; no está disponible solo con descargar el modelo.

## Cómo funciona

1. Importa un documento privado o pega su texto.
2. Pulsa **Proteger documento**. En PDF e imágenes, el resultado abre directamente **Edit**.
3. Revisa el documento y el mapa privado en Editar. Selecciona un párrafo sobre la página, escribe y ajusta fuente, estilos, ancho, altura y márgenes internos. Para salir de su caja usa un clic fuera, Escape o Terminar edición; si hay cambios pendientes, elige guardar y salir, descartar o continuar. El mapa permite renombrar conjuntamente todas las entradas que comparten un alias.
4. Guarda las correcciones y pulsa **Exportar PDF**. La exportación conserva la distribución original en las zonas no editadas; si una corrección no cabe, se rechaza para que puedas ajustarla, sin recortarla.

La edición de PDF mantiene posiciones por bloques; no redistribuye automáticamente todo el documento como Word. Las zonas OCR y las fuentes disponibles pueden limitar la fidelidad. El historial de cambios y las páginas permanecen en memoria durante la sesión. Los avances y límites de evaluación y postentrenamiento se documentan en [PRIVACY_EVALUATION.md](docs/PRIVACY_EVALUATION.md); esta versión no incluye un nuevo modelo entrenado y validado.

La exportación privada valida el contenido y el título contra la sesión revisada. La detección y el OCR pueden omitir o interpretar mal datos: **la revisión humana sigue siendo necesaria y la sanitización no garantiza anonimato**.

## Privacidad y datos

- El documento privado, el mapa y sus ediciones permanecen en memoria durante la sesión y no se añaden al historial de audio.
- El archivo original permanece en su ubicación; el PDF exportado se guarda donde elijas.
- Las transcripciones de audio se guardan en el historial local de la aplicación.
- Los modelos se descargan de fuentes externas. Estas descargas no incluyen tus documentos ni tu audio.
- El repositorio contiene código, recursos visuales y pruebas; los modelos, instaladores, cachés y datos locales de la app quedan fuera de Git.

## Instalación en macOS

**Instalador disponible: 0.3.0 · Apple Silicon · macOS 13.4 o posterior.**

Abre el DMG y arrastra **Belegal _ Local demo.app** a **Aplicaciones**. En la primera apertura, el botón de bienvenida lleva a Models si faltan los modelos requeridos; si están instalados, permite comenzar.

El instalador local verificado se publica como versión de prueba en [GitHub Releases](https://github.com/VodoooFilms/belegal/releases/tag/v0.3.0). Los archivos de instalación permanecen fuera del árbol de código. La app está firmada ad hoc y **no está notarizada por Apple**; macOS puede mostrar avisos de seguridad al distribuirla. Los modelos no se incluyen en el DMG.

## Desarrollo

Stack: **Svelte 5 · TypeScript · Tauri 2 · Rust**. El OCR de macOS utiliza Vision y PDFKit; la inferencia de documentos utiliza Qwen3 con llama.cpp.

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` inicia solo la interfaz web. Audio, OCR, modelos y persistencia requieren la app Tauri. Para construir y abrir la app local con sus recursos nativos:

```sh
pnpm demo:mac
```

Verificaciones:

```sh
pnpm check            # typecheck y build
pnpm check:native     # rustfmt y Clippy
pnpm test:native      # pruebas Rust
pnpm exec playwright install webkit
pnpm test:ui          # inicia y cierra sus servidores automáticamente
cargo test --manifest-path src-tauri/Cargo.toml --lib layout::tests  # genera fixtures de QA en macOS
pnpm test:layout:ui   # regenera QA si falta; importar → proteger → editar → exportar
pnpm test:workspace:ui # documento, mapa privado y editor unificados
```

La suite UI cubre privacidad en español e inglés, permisos y descargas del setup, bienvenida, editor, historial, hablantes y captura PCM. Requiere libres los puertos 1420 y 5198. En Linux, instala WebKit con `--with-deps`.

Las pruebas extensas del backend están en `src-tauri/src/privacy/tests.rs`, `audio/tests.rs` y `pdf/tests.rs`. Las cuatro pruebas Qwen marcadas `ignore` necesitan pesos locales y ejecución explícita; algunas pruebas de audio se omiten cuando faltan sus recursos. CI conserva Xcode 16.4 y la imagen Playwright con navegadores preinstalados.

El [mapa de código](docs/CODEMAP.md) describe la separación del editor y del parser PDF y la estructura de módulos.

### Crear un instalador

Para empaquetar una app ya construida y verificada, sin recompilar dependencias:

```sh
python3 scripts/package-local-installer.py \
  --app "/ruta/Belegal _ Local demo.app" \
  --output "/ruta/BeLegal-local.dmg"
```

El script comprueba la firma y los recursos de la app, genera el DMG, verifica su integridad y escribe un archivo SHA-256. El archivo de salida debe ser nuevo.

`pnpm release:mac:dev` construye una versión local desde el código. `pnpm release:mac` prepara una versión con firma Developer ID y notarización; requiere las credenciales de Apple y superar los controles del script de release.

## Documentación y origen

- [Frontera de privacidad y arquitectura](PRIVATE_AI.md): detalles técnicos; algunas referencias de interfaz corresponden a etapas anteriores.
- [Portabilidad de macOS](MACOS_PORTABILITY.md).
- [Licencias y avisos de terceros](THIRD_PARTY_NOTICES.md).

Este proyecto es una base independiente derivada de Voice to PDF, adaptada para BeLegal. Su identificador de aplicación es `com.belegal.localdemo`. Los [documentos históricos](docs/archive/README.md) conservan las etapas anteriores de la base y no describen la publicación actual.
