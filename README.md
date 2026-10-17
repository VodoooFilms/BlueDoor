# Local Legal

Aplicación de escritorio para preparar documentos legales y transcripciones en tu propio equipo: análisis local de información sensible, alias revisables, edición enriquecida y exportación PDF.

Nombre e identidad visual provisionales para demostraciones privadas a abogados. Base independiente de Belegal, conservada desde el commit `b21bf40` del 17 de octubre de 2026. Belegal queda en pausa en su repositorio original; esta copia no tiene remoto configurado.

## Desarrollo

```sh
pnpm install --frozen-lockfile
pnpm dev
# Aplicación nativa (requiere toolchain Rust/Xcode y runtime local)
pnpm tauri dev
```

## Verificación

```sh
pnpm typecheck
pnpm build
pnpm test:native
pnpm check:native
pnpm test:ui
pnpm test:workspace:ui
pnpm test:layout:ui
```

## Compilación macOS

`pnpm release:mac:dev` crea una compilación de desarrollo con firma ad hoc. `pnpm release:mac` requiere identidad de distribución y notarización configuradas. No se ha generado ni publicado un instalador Local Legal en esta adaptación.

## Identidad y datos

- Aplicación: **Local Legal**, identificador `com.locallegal.desktop`.
- Historial y modelos: directorio de datos propio de Tauri; en macOS `~/Library/Application Support/com.locallegal.desktop/`.
- Las preferencias y la primera bienvenida usan claves propias. No se copian documentos, historial ni modelos personales de Belegal.
- Se conservan los identificadores y hashes de los modelos existentes (incluido `belegal-pii-v1`) para no alterar la compatibilidad de sus artefactos.
- Marca SVG e iconos editables en `src/assets/` y `assets/branding/`.

Los documentos en `docs/archive/` y `docs/milestones/` son antecedentes históricos de la base. La licencia y los avisos originales se conservan. Véanse [PRIVACY.md](PRIVACY.md), [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) y [docs/LOCAL_LEGAL_FORK.md](docs/LOCAL_LEGAL_FORK.md).
