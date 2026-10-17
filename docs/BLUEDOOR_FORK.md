# BlueDoor — 17 de octubre de 2026

Copia independiente de `/Users/antoin/Documents/local-legal`, commit `bb28079`, guardada en `/Users/antoin/Documents/BlueDoor`. Conserva el historial Git; rama local `codex/bluedoor`, sin remoto. No modifica ni publica Belegal, Local Legal o Prompthea.

## Identidad

- Producto: BlueDoor; paquete/ejecutable `bluedoor`; crate `bluedoor_lib`; identificador `com.bluedoor.desktop`.
- Preferencias, historial y directorio de modelos independientes. No se migran datos personales. Los identificadores/hashes de los artefactos de modelos conservan su compatibilidad.
- Fondo #1248bc, navegación y wordmark blancos, puerta geométrica, sans serif, acción principal blanca con texto azul. Las ilustraciones y el flujo inferior tienen opacity .8; la navegación mantiene opacity 1.
- La apariencia oscura conserva la identidad con un azul más profundo.
- SVG editables e iconos de plataforma regenerados. Los controles sin texto visible mantienen nombres accesibles y tooltips. Los nombres de modelos, contenido de documentos, campos y mensajes de revisión/error se conservan para no sacrificar información necesaria.
- Los documentos quedan fuera del tema: las páginas nativas se muestran sin filtros ni opacidad; el editor conserva colores, fuentes, tamaños y formato explícitos. El cambio de apariencia no reescribe su HTML. La tinta predeterminada del texto sin formato es oscura sobre papel blanco.

## Validación

`pnpm typecheck`, `pnpm build`, `pnpm test:native` (104 aprobadas, 15 omitidas por modelos/fixtures), `pnpm check:native`, `pnpm test:ui`, `pnpm test:workspace:ui` y `pnpm test:layout:ui`. La prueba BlueDoor adicional recorre importación, audio, ajustes, modelos, editor y mapa; verifica la opacidad de ilustraciones y navegación, el botón blanco, la legibilidad del papel y la ventana mínima 960 × 680.

Capturas PNG en `artifacts/bluedoor/` generadas con IPC simulado y un documento ficticio. Los flujos heredados verifican guardado, fallos, alias y exportación revisada. No se ha creado ni instalado un nuevo bundle de BlueDoor; la instalación anterior permanece intacta.

Se alineó la cabecera IPC de grabación con el backend (`X-BlueDoor-Config`), manteniendo el token de configuración de un solo uso.

Por instrucción del usuario, la carpeta Local Legal se movió a `~/.Trash/local-legal-2026-10-17` después de verificar que no tenía cambios pendientes y que BlueDoor conservaba su commit de origen. Belegal permanece intacto.

`pnpm test:brand:ui` también comprueba grabación PCM con audio sintético, envío de WAV binario y cabecera de autorización del backend simulado, además de la independencia entre formato del documento y tema.
