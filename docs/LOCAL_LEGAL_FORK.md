# Copia Local Legal — 17 de octubre de 2026

Origen: `/Users/antoin/Documents/belegal`, commit `b21bf40`, publicado en el repositorio privado `VodoooFilms/belegal`. Se conserva el historial Git y se elimina el remoto para evitar publicaciones accidentales a Belegal.

Nombre provisional Local Legal; paquete y ejecutable `local-legal`, crate `local_legal_lib`, identificador `com.locallegal.desktop`. Scripts, helpers nativos, preferencias y cabecera IPC de audio tienen nombres propios. Se reemplazan el logotipo y los iconos de Belegal por una identidad provisional editable.

No se migran datos del usuario ni se reemplaza la aplicación Belegal instalada. Los artefactos de modelos conservan sus identidades y metadatos de integridad. Los documentos históricos y las atribuciones de licencia conservan su procedencia.

Belegal permanece sin cambios después de la copia. No se publica Local Legal ni se crea un repositorio remoto en esta etapa.

Verificación de esta adaptación: `pnpm typecheck` (sin errores ni warnings), `pnpm build`, `pnpm test:native` (104 aprobadas, 15 omitidas por fixtures/modelos), `pnpm check:native`, `pnpm test:ui`, `pnpm test:workspace:ui` y `pnpm test:layout:ui`, todo en verde. Los scripts Python se validaron sintácticamente y se revisó la captura de bienvenida en `artifacts/local-legal-welcome.png`.

No se ha generado un instalador ni se ha verificado una instalación nativa de Local Legal. Los runtimes locales de compilación se copiaron como entradas ignoradas; no contienen modelos ni documentos personales. La nueva identidad se guarda en una rama local `codex/local-legal`.
