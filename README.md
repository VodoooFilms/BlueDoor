# Belegal _ Local demo

Aplicación de escritorio independiente con Svelte 5 y Tauri 2. Graba e importa audio, transcribe localmente, analiza documentos con Qwen3, permite revisar el mapa privado y editar el documento sanitizado, y exporta PDF. El procesamiento de documentos permanece en el dispositivo.

La base se copió de Voice to PDF el 30 de septiembre de 2026, desde el commit de referencia `1dbcc88b2092b61e3e98d9c360affc1623cdca92` y cambios locales. Esta demo usa el identificador `com.belegal.localdemo` y su propio almacenamiento. Su repositorio debe ser independiente del original.

## Desarrollo

Requiere Node.js, pnpm `12.3.4`, Rust y las herramientas de compilación de Tauri para la plataforma. macOS Apple Silicon es la plataforma verificada localmente. Los scripts de Windows se conservan; esta limpieza no verifica su empaquetado.

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` ejecuta solo la interfaz web. Audio, modelos, diálogos y persistencia necesitan Tauri. Para compilar una app local en macOS, preparar sus componentes, verificar el bundle y firmarlo ad hoc:

```sh
pnpm demo:mac -- --no-open
```

La app queda en `src-tauri/target/debug/bundle/macos/Belegal _ Local demo.app`. Omite `--no-open` para abrirla al terminar. Los recursos de audio se preparan automáticamente si faltan; el OCR se compila durante el build nativo. Este comando no publica en GitHub.

## Verificación

```sh
pnpm check            # typecheck + build del frontend
pnpm check:native     # rustfmt + Clippy, advertencias como errores
pnpm test:native      # pruebas Rust
pnpm exec playwright install webkit
pnpm test:ui          # inicia y cierra automáticamente los servidores de prueba
```

La suite UI usa WebKit y mocks de Tauri: cubre privacidad en español e inglés, setup de permisos y descargas, bienvenida, editor, historial, hablantes y captura PCM. Necesita libres los puertos `1420` y `5198`; los resultados visuales se generan en `artifacts/`, fuera de Git. En Linux, instala WebKit con `--with-deps`.

Las pruebas nativas con modelos o audio externos pueden omitirse cuando faltan sus recursos locales. Las cuatro pruebas Qwen marcadas `ignore` requieren ejecución explícita y pesos verificados. El resultado de estas suites no equivale a una nueva certificación de inferencia real o de permisos del sistema operativo.

## Organización

Consulta el [mapa de código](docs/CODEMAP.md), la [arquitectura de privacidad](PRIVATE_AI.md) y el [estado de arquitectura](ARCHITECTURE_STATUS.md). La documentación comercial y los registros anteriores se conservan en [docs/archive](docs/archive/README.md) como historial de la base.

La diarización Nemotron requiere un ejecutable adicional y sigue siendo experimental. El botón **Subir al servidor** abre la página principal de Belegal en el navegador; actualmente no envía archivos. El setup inicial solicita los accesos disponibles antes de descargar los tres modelos por orden; el sistema operativo puede solicitar accesos adicionales al abrir otros archivos o destinos.

`node_modules/`, `dist/`, `src-tauri/target/`, los recursos nativos generados, esquemas y resultados de pruebas no se versionan. Las fixtures sintéticas, iconos, fuentes, licencias y lockfiles sí forman parte del proyecto. No añadas modelos, documentos personales ni credenciales al repositorio.
