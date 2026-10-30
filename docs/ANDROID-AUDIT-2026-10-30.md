# Auditoría del APK Android — 30 de octubre de 2026

Artefacto auditado: `installers/BlueDoor-android-arm64-preview.apk`

- SHA-256: `7fd5979598ac148087c1bac8c2fc85611b203d36dffb76f65199e03b522e9994`
- Tamaño: 72 041 084 bytes. Construido el 30/10/2026 05:54 desde un árbol de trabajo con cambios sin confirmar (HEAD `1fae3cf`).
- Firma local de prueba `CN=BlueDoor local preview` (SHA-256 `ad6b9f9b…c214`), esquemas v2 y v3 válidos; sin v1.
- `versionCode=4002`, `versionName=0.4.2`, `minSdk=24`, `targetSdk=34`, `compileSdk=36`, solo `arm64-v8a`.
- `allowBackup=false`, `usesCleartextTraffic=false`, `extractNativeLibs=false`, no depurable, alineación ZIP/ELF de 16 KiB verificada.

Las pruebas de dispositivo se hicieron en el emulador `BlueDoorQA` (arm64-v8a, API 35, 2 GB). **No es un Redmi 15C** y sus tiempos no representan al teléfono.

## Veredicto

El APK se instala, arranca y ejecuta el recorrido nuevo completo en el emulador: importar → OCR y diseño → analizar con GLiNER → revisar entidades y alias → aprobar → exportar y compartir.

No está listo para entregarse como «0.4.2»:

1. El PDF exportado conserva restos visibles del texto original en la zona redactada.
2. ML Kit envía telemetría a Google durante el OCR, y no está desactivada ni documentada.
3. El PDF protegido no tiene capa de texto (no se puede buscar, copiar ni leer con lector de pantalla).
4. La importación depende por completo del OCR y falla sin alternativa en documentos habituales (>30 páginas, >10 MB, texto OCR >60 KiB).
5. El código del APK no está confirmado y la versión no distingue esta compilación de la 0.4.2 anterior.

## Verificación en ejecución

| Comprobación | Resultado |
|---|---|
| `adb install -r` | Correcta; el `base.apk` instalado tiene el mismo SHA-256 que el archivo auditado. |
| Arranque | `Displayed … +833ms`, sin `FATAL EXCEPTION`, sin `UnsatisfiedLinkError`; `libbluedoor_lib.so` cargada desde el APK. |
| Captura de pantalla | `screencap` devuelve negro: `FLAG_SECURE` activo en `MainActivity`. El árbol de accesibilidad sí se puede leer. |
| Permisos concedidos | Solo `INTERNET` y `ACCESS_NETWORK_STATE`; no hay cámara, micrófono, almacenamiento ni notificaciones. |
| Descarga de modelo | GLiNER PII Small INT8: 82 680 500 B + tokenizador 3 583 593 B, idénticos al manifiesto; queda en `models/`. Qwen no está incluido en el APK. |
| Importación de `tests/fixtures/private-scan.pdf` | OCR + detección de códigos + vista de página original; aviso «OCR regions are estimates…». |
| Análisis con GLiNER | 5 entidades; mapa privado con `PERSON_01`, `PERSON_02`, `ID_01`, `COMPANY_01`, `CASE_01`. |
| Aprobación y exportación | Botón de compartir con `documento-protegido-<uuid>.pdf`; copia guardada en `cache/exports/` y aviso con la ruta. |
| PDF exportado | Una sola imagen 1565×2000, `MediaBox [0 0 864 1104]` (igual al original), sin fuentes, sin operadores de texto y sin cadenas del original. |
| PDF de 31 páginas | Rechazado con «El PDF supera 30 pages; divídelo.»; no hay reserva a extracción de texto. |
| Mapa vacío | La aprobación se bloquea: «The map has no entities: the protected version would be the original». |
| Telemetría | `TransportRuntime.CctTransportBackend: Making request to: https://firebaselogging.googleapis.com/v0cc/log/batch?format=json_proto3` justo después del OCR. |
| Datos en logs | Ninguna aparición de los valores sintéticos (`BANCO ANDINO`, `0912345678`) en `logcat`. |
| Tras exportar y volver del selector | La interfaz dejó de responder; el proceso sandbox de WebView quedó congelado (`do_freezer_trap`) y `MetricsBridgeService` se reinició. Compatible con presión de memoria del emulador de 2 GB; no concluyente para el Redmi. Tras reiniciar, aviso correcto «La sesión anterior terminó…». |

## Hallazgos

### Alta

1. **Restos del texto original en el PDF exportado.** En la exportación real de `private-scan.pdf` queda una «C» negra del original «CEDULA» pegada al alias (`CPERSON_02ID_01`), los alias contiguos se dibujan sin separador y la zona borrada deja un rectángulo tenue con halo gris. La cobertura se calcula con rectángulos de entidad y de párrafo (`layout.rs`), y el borrado pinta solo el color de fondo muestreado (`DocumentLayout.eraseParagraph`); los glifos fuera del rectángulo o entre dos rectángulos permanecen. El usuario puede creer protegido un valor que sigue siendo legible en parte.
2. **Telemetría de ML Kit activa.** `MLKitLoggingOptions{libraryName=common, enableFirelog=true, firelogEventType=1}`, endpoint `visionkit-pa.googleapis.com` en `classes.dex` y peticiones reales a `firebaselogging.googleapis.com`; la app transmitió unos 343 KB en la sesión (descarga del modelo incluida). No se encontró código que envíe texto del documento y el contenido exacto del lote no es inspeccionable desde aquí, pero es una salida externa no desactivada que contradice la afirmación de que todo el procesamiento es local.
3. **El PDF protegido no tiene capa de texto.** El resultado es un raster único: no se puede buscar, seleccionar, copiar ni leer con lector de pantalla, y el texto reflowado que el usuario edita no llega al archivo. No hay aviso en la interfaz ni en la documentación.
4. **Importación dependiente del OCR, sin alternativa.** Todo PDF e imagen pasa por `DocumentLayout.extract`. Se perdió la extracción de texto nativa de PDF; un PDF de más de 30 páginas falla entero (verificado), igual que uno de más de 10 MB, uno cuyo OCR supere 60 KiB de texto o cuyas imágenes superen 80 MB en base64. Un documento legal habitual cae en alguno de esos límites.
5. **Origen no reproducible y versión engañosa.** `DocumentLayout.kt`, `DocumentLayoutTest.kt`, `document-pinch.ts`, `layout.rs`, `privacy.rs`, `App.svelte` y otros siguen sin confirmar; `versionCode=4002`/`versionName=0.4.2` son idénticos a la entrega anterior aunque el contenido cambia; `tauri.conf.json`, `Cargo.toml` y `package.json` siguen en `0.4.0`. Nadie puede distinguir esta compilación de la 0.4.2 previa ni reconstruirla desde la rama.

### Media

6. **Páginas de imagen con píxeles como puntos.** `page(bitmap, bitmap.width.toDouble(), bitmap.height.toDouble())` crea páginas de hasta 2000 pt (≈27.8 in) porque `MAX_PIXELS=2000`. La impresión y la escala percibida quedan mal.
7. **Lo que el OCR no detecta permanece visible.** El export conserva los píxeles originales salvo redacciones y códigos; manuscritos, firmas, sellos o letra pequeña no detectada siguen a la vista y este APK no ofrece una herramienta manual para cubrir regiones. El aviso ayuda, pero la garantía «nunca aparece protegido un análisis incompleto» no cubre ese caso.
8. **Blanqueo automático de QR/códigos.** Tras el análisis se borran los códigos detectados sin confirmación ni revisión; un QR legítimo (verificación, sello) desaparece del documento exportado.
9. **Memoria.** Se acumulan hasta 80 MB de imágenes en base64 dentro del JSON que cruza JNI y luego se decodifican página a página; con 30 páginas a 2000 px el pico es alto para Android y el emulador de 2 GB ya mostró el WebView congelado tras una exportación.
10. **Sin `dataExtractionRules`.** `allowBackup=false` no bloquea explícitamente la transferencia entre dispositivos de Android 12+.
11. **Peso y símbolos.** El escaneo de códigos añade ~5.9 MB (`libbarhopper_v3.so` 4.9 MB, modelos TFLite y dex) para una función de borrado automático; el release conserva `.symtab` (3.46 MB en `libbluedoor_lib.so`) por `jniLibs.keepDebugSymbols`.

### Baja

12. Textos: «El PDF supera 30 pages; divídelo.» mezcla idiomas, «· 1 pages» ignora el singular, y los alias contiguos se concatenan.
13. El nombre exportado incluye el UUID completo.
14. El onboarding reaparece tras un arranque en frío.
15. «Enviar a GPT» abre `chatgpt.com` sin adjuntar nada (correcto y con tooltip), pero conviene comprobar que el texto saneado no se pegue solo.

## No verificado

- Nada en el Redmi 15C: no hay dispositivo conectado; no hay medidas de tiempo, temperatura ni memoria del teléfono.
- Contenido exacto del lote de telemetría de ML Kit (requeriría interceptar TLS con una CA instalada).
- El recorrido con Qwen3-1.7B Q4_K_M: no se descargó (1.36 GB) en esta auditoría.
- Exportación de PDF a partir de PNG/JPG: solo se revisó el código.

## Recomendaciones, por orden

1. Confirmar los cambios, alinear versión (`0.4.3`/`4003`) y reconstruir; publicar el SHA-256 junto al APK.
2. Corregir el borrado: cubrir el párrafo completo con blanco sólido en vez del color muestreado, ampliar los rectángulos, separar alias contiguos y añadir una comprobación posterior al render que detecte píxeles oscuros fuera de los alias.
3. Desactivar la telemetría de ML Kit o documentarla y verificarla con captura de red.
4. Añadir capa de texto al PDF exportado con fuentes embebidas, o advertir con claridad de que es solo imagen.
5. Recuperar la extracción de texto nativa para PDF con capa de texto y una reserva cuando el OCR o el diseño fallen o se superen los límites; subir el tope de páginas o procesar por lotes.
6. Convertir píxeles a puntos con una resolución razonable (72–150 ppp) en las páginas de imagen.
7. Ofrecer cobertura manual de regiones y confirmación antes de blanquear códigos.
8. Reducir memoria: pasar rutas de archivo en vez de base64 y procesar página a página.
9. Añadir `dataExtractionRules`, quitar `keepDebugSymbols` del release y valorar el escaneo de códigos como módulo opcional.

## Reproducción

```bash
./scripts/build-android.sh                       # APK de prueba firmado con la clave local
adb install -r installers/BlueDoor-android-arm64-preview.apk
adb shell am start -n com.bluedoor.desktop/.MainActivity
adb push tests/fixtures/private-scan.pdf /sdcard/Download/
# Importar desde Documentos, analizar con GLiNER, aprobar y exportar.
adb root && adb pull /data/data/com.bluedoor.desktop/cache/exports/<archivo>.pdf
```

El PDF exportado se puede comparar con el original: no contiene fuentes ni operadores de texto, y la imagen de página se puede extraer del flujo `/Subtype /Image` para revisar la cobertura del borrado píxel a píxel.

## Correcciones aplicadas en 0.5.0

Versión 0.5.0 / versionCode 5000 en `package.json`, `tauri.conf.json` y `Cargo.toml`.

| # | Estado | Cambio |
|---|---|---|
| 1 | Corregido | Se borran primero todas las áreas de la página y luego se dibujan los alias, con una separación mínima de 1,5 pt para que dos valores contiguos no se lean como uno. |
| 2 | Corregido | Se elimina el registro del backend CCT del manifiesto, conservando sus clases necesarias para iniciar OCR. La exclusión completa de la dependencia se descartó por romper ML Kit. Falta captura de red. Se conserva `INTERNET` solo para descargar los modelos. |
| 3 | Pendiente | El PDF exportado con maquetación sigue siendo raster sin capa de texto. Requiere mapear los desplazamientos saneados a la geometría de los glifos. Se documenta como pendiente en `ANDROID.md`. |
| 4 | Corregido | Si `extract_android` falla en un PDF, se usa su capa de texto o el OCR completo (`fallback_text`) y la sesión queda como sesión de texto con `layout_available=false`, en vez de abortar la importación. |
| 5 | Corregido | Versión alineada a 0.5.0/5000 y binarios release sin símbolos (`strip = "symbols"`). |
| 6 | Corregido | Las páginas de imagen se ajustan a A4 conservando la proporción (`pageBox`). |
| 7 | Pendiente | Sigue sin haber cobertura manual de regiones; el aviso de revisión se mantiene. |
| 8 | Pendiente | Los códigos QR se siguen blanqueando automáticamente. |
| 9 | Parcial | Tope de páginas 60 con mensaje dinámico; el paso de imágenes por base64 no cambió. |
| 10 | Corregido | `dataExtractionRules` excluye todos los dominios de copia en la nube y transferencia; `fullBackupContent=false`. |
| 11 | Parcial | Release sin `.symtab`; el escaneo de códigos sigue incluido. |
| 12 | Corregido | «página/páginas» en singular y plural, mensaje de límite en español con el número real. |
| 13 | Intencionado | El UUID del nombre de exportación impide que el nombre del documento llegue a la URI compartida. |
| 14 | Pendiente | El onboarding reaparece tras arranque en frío. |
| 15 | Sin cambio | «Enviar a GPT» no adjunta nada; el texto saneado no se pega solo. |

Además, GLiNER rechaza spans de persona que absorben etiquetas impresas («CEDULA 0912…») y resuelve solapamientos entre sus dos pasadas prefiriendo el tipo específico (ID, teléfono, cuenta) y, a igual tipo, el span más largo. Qwen usa lotes de prefill de 128 tokens y hasta dos hilos más que la generación, que sigue limitada a cuatro.


## Continuación: borrado y validación del código

- Se mantiene la versión ya presente 0.5.0; no se retrocede a 0.4.3.
- Android borra párrafos con blanco sólido y un margen de 1,5 pt; las redacciones de entidad usan el mismo margen. Comprueba que los píxeles interiores de cada zona borrada sean blancos antes de dibujar texto nuevo. Esta comprobación cubre el rectángulo previsto, no demuestra que el OCR haya localizado todo el dato privado fuera de él.
- Los alias contiguos respetan 1,5 pt incluso cuando sus bordes coinciden. Si la separación provoca desbordamiento del área, el render bloquea la operación en vez de escribir sobre contenido vecino.
- Se añaden regresiones Android para papel teñido, bordes de OCR y alias contiguos. Se actualiza la expectativa de geometría de imágenes a A4.
- Validación: `pnpm typecheck` (sin errores ni avisos), `pnpm build`, `pnpm test:layout:ui` y compilación Kotlin de aplicación y pruebas correctos. La compilación Kotlin omite recompilar Rust; no valida un nuevo APK release completo.
- Siguen pendientes capa de texto, cobertura manual, revisión de QR, transporte de imágenes sin base64 y validación Qwen/Redmi. No hay dispositivo físico conectado.

La ejecución instrumentada en BlueDoorQA se intentó, pero terminó con cero pruebas por `INSTALL_FAILED_UPDATE_INCOMPATIBLE`: la aplicación instalada tiene otra firma. No se desinstaló ni se borraron sus datos. Las nuevas regresiones están compiladas, todavía no ejecutadas. No se reemplazó el APK preview de `installers/`.

## Segunda continuación: ejecución Android

Se resolvió la incompatibilidad de firmas firmando copias temporales de los APK debug y androidTest con la clave preview existente. Se instalaron mediante actualización, sin desinstalar ni borrar datos del emulador.

Las seis pruebas de `DocumentLayoutTest` se ejecutaron: inicialmente cinco correctas y OCR fallido con `NoClassDefFoundError: CCTDestination`. La exclusión de `transport-backend-cct` era incorrecta: ML Kit necesita sus clases al inicializarse. Se restauró la dependencia y se eliminó únicamente el metadato `backend:com.google.android.datatransport.cct.CctBackendFactory` del manifiesto combinado.

Tras la corrección pasan las seis pruebas, incluido OCR, geometría A4, borrado opaco, alias contiguos, edición y PDF. Logcat registra `Transport backend 'cct' is not registered`, sin peticiones Firelog observadas durante estas pruebas. No se ha hecho interceptación ni captura de red; ML Kit sigue generando eventos locales. Se añade además una regresión que comprueba el registro del backend en el manifiesto instalado.

La suite debug ampliada pasa **7/7** en BlueDoorQA, incluida la comprobación del manifiesto instalado. La instrumentación directa sobre release ofuscado terminó en `Process crashed`; no se cuenta como validación release. Se detectó y corrigió además `tauri.android.conf.json`, que todavía sobrescribía la versión con 0.4.2/4002. El script de construcción comprueba ahora las versiones del APK y de ambos archivos de configuración frente a `package.json`.

### Artefacto reconstruido

- `installers/BlueDoor-android-arm64-preview.apk`: 0.5.0 / 5000.
- SHA-256: `921093f88b9e070d6926c473e3980b13f357830ec8e33a001cfe0c8bfafaa38e`.
- Construcción release completa correcta; firma local, alineación ZIP/ELF de 16 KiB y clases JNI ONNX verificadas. Instalación por actualización y arranque en frío correctos en BlueDoorQA (no representa rendimiento Redmi).
- `pnpm typecheck`, `pnpm build` y `pnpm test:layout:ui` correctos. Las siete pruebas instrumentadas corresponden al APK debug; el release solo tiene comprobación de instalación/arranque, no el recorrido privado completo.
- El árbol sigue con cambios sin confirmar; el hash identifica este binario, no una reconstrucción reproducible desde un commit limpio. No se hizo push ni publicación.
