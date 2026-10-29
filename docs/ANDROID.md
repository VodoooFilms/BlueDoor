# BlueDoor Android: prueba local

Destino: Redmi 15C, Helio G81-Ultra, 8 GB físicos. La extensión virtual no cuenta como RAM para dimensionar el modelo. Rama: `codex/android-protection`. No se publica en tiendas ni se sube el APK a servicios externos.

## Instalar y empezar

1. Copia `installers/BlueDoor-android-arm64-preview.apk` al teléfono y ábrelo. Permite instalar desde la aplicación que uses para abrirlo. Es una firma local de prueba; conserva la clave de `.build-cache/android-preview.keystore` para actualizar sin desinstalar.
2. También puedes conectar el teléfono por USB y ejecutar `adb install -r installers/BlueDoor-android-arm64-preview.apk`. Requiere Android 7/API 24 o posterior y sistema ARM64. El manifiesto del APK y la ABI se comprueban al construir; hay que confirmar la ABI real del teléfono con `adb shell getprop ro.product.cpu.abilist`.
3. En **Modelo local y texto pegado**, pulsa **Descargar Qwen**. Primera descarga: Qwen3-1.7B Q4_K_M, aproximadamente 1,36 GB. La descarga viene del registro de Ollama y se valida con tamaño y SHA-256 fijados en `src-tauri/model-manifest.json`. Nunca se adjunta un documento a esa petición. Reserva unos 3 GB libres para instalación, modelo y archivos temporales.
4. OCR latino de ML Kit está incluido en el APK. No requiere descargar un modelo OCR ni conectarse a Play Services para reconocer texto.
5. Importa TXT/Markdown UTF-8, DOCX, PDF o imagen. Puedes pegar texto. Abre **Ver fuente PDF / imagen** para contrastar la extracción con el archivo, dentro de la aplicación. Pellizca para ampliar, arrastra para desplazarte y toca dos veces para volver al ajuste de página. Las fotos respetan su orientación EXIF. Puedes corregir el texto extraído antes de analizarlo.
6. Analiza, revisa todas las entidades y sus contextos, agrupa variantes usando el mismo alias, añade omisiones y guarda el mapa. Alterna original/copia protegida. Edita y guarda la copia protegida; pulsa **Aprobar revisión** y después **Exportar PDF**. El menú Compartir permite guardar en Archivos o entregar la copia a una aplicación que elijas.
7. Tras instalar Qwen, repite con modo avión. El análisis, OCR, revisión y PDF deben funcionar sin red. Compartir a servicios de terceros es una decisión explícita del usuario.

Esto es **pseudonimización revisada**, no garantía de anonimato. Los nombres y datos contextuales pueden requerir corrección humana. Una exportación del Vault puede contener el original: no equivale al PDF protegido.

## Arquitectura y garantías

Se mantiene Tauri 2 y Svelte. No se encontró un impedimento que justifique cambiar de tecnología. `privacy.rs`, validaciones de PDF y cifrado de `vault.rs` se comparten entre escritorio y Android; `android.rs`, Kotlin y las ramas `cfg(target_os = "android")` resuelven las diferencias de plataforma.

- llama.cpp está enlazado estáticamente dentro de `libbluedoor_lib.so`; CPU ARMv8-A, sin servidor, sin Metal/Vulkan, sin buscar bibliotecas de escritorio. Máximo inicial de cuatro hilos en Android. Contexto inicial 4096, respuesta máxima 1024, fragmentos de 2000 caracteres y solapamiento de 300. El perfil se puede ajustar dentro de límites nativos; el tokenizador real comprueba que instrucciones, entrada y respuesta caben. Las tareas que no caben se subdividen y los huecos de cobertura bloquean la revisión/exportación.
- Prefill en lotes de 256 tokens con comprobación de cancelación entre lotes; cancelación durante generación entre tokens. Carga/verificación del modelo no es interrumpible inmediatamente. No se publica una sesión nueva si la operación se cancela.
- El modelo solo propone entidades. El núcleo verifica las propuestas contra el original y realiza sustituciones deterministas. Texto, mapa y revisión tienen identidad/revisión nativas. El escritor de PDF vuelve a validar texto visible, HTML, título y metadatos. Una aprobación anterior no sirve tras cambios.
- Para OCR se libera el modelo y se serializa la extracción frente a inferencia. PDF escaneado: una página y bitmap a la vez, imagen acotada a 2000 píxeles de lado. Los PDF mixtos con páginas sin texto suficiente pasan completos por OCR. Límites de documentos: archivo 10 MB, texto 64 KiB; OCR máximo 30 páginas y 60 KiB. Superar límites produce error, sin truncamiento silencioso.
- DOCX incorpora texto principal, encabezados, pies y notas. No conserva diseño DOCX ni extrae automáticamente texto de imágenes incrustadas en DOCX: conviértelo a PDF y usa OCR cuando sea necesario. TIFF no tiene soporte garantizado por el decodificador Android; un fallo se comunica.
- La copia temporal del selector vive en caché privada y se elimina al terminar, incluso en errores normales; al volver a iniciar se limpian importaciones y transferencias del Vault abandonadas por muerte del proceso. No se persiste automáticamente texto original, mapa ni borrador. Solo se guardan indicadores de sesión/operación, nunca su contenido. El núcleo sincroniza un marcador de un byte antes de abrir una sesión para poder avisar de su pérdida incluso si WebStorage no llegó a escribirse. El Vault es el almacenamiento optativo cifrado.
- Vault conserva Argon2 y ChaCha20-Poly1305; archivos e índice permanecen en almacenamiento privado. El selector SAF se adapta para entrada, salida y copia cifrada. Las entradas móviles se acotan a 32 MB para limitar memoria; las copias cifradas se transfieren por bloques con límite de 4 GB. Al pausar la Activity, incluso ante Compartir, se bloquea la clave y la interfaz borra títulos y secretos visibles. Una operación explícita que ya capturó una clave para importar/exportar termina con su copia temporal y la libera al salir; el usuario puede tener que desbloquear de nuevo al volver del selector.
- Atrás envía la aplicación al fondo para preservar el borrador en memoria; cancela análisis activo y bloquea Vault. Rotación mantiene la Activity. Si Android mata el proceso, la siguiente apertura informa de pérdida de la sesión y exige reimportar/reanalizar. No se promete recuperación de originales no guardados.
- Audio/transcripción no están disponibles en navegación Android ni registrados como comandos operativos; transcribe.cpp, hound, FFmpeg y permisos de micrófono se excluyen. El APK solo declara red para descarga de recursos y permisos internos de AndroidX. Se bloquean capturas y vista en Recientes con FLAG_SECURE.
- Exportación móvil crea un PDF nuevo de texto, sin imágenes originales ni metadatos de la fuente. Se empaquetan cuatro variantes de Liberation Sans con su licencia. No se conserva el formato gráfico original del PDF; la familia tipográfica de salida móvil es Liberation Sans.

## Construcción reproducible

Se usaron JDK 17, Android platform 36, Build Tools 35.0.0, NDK r27c, Rust target `aarch64-linux-android` y pnpm del repositorio. Se instalaron las herramientas y se aceptaron las licencias del SDK con autorización del usuario. La carpeta original ausente se recuperó como enlace local a la copia de trabajo existente; no se descartaron sus cambios previos.

```sh
export JAVA_HOME=/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home
export ANDROID_HOME="$HOME/Library/Android/sdk"
export NDK_HOME="$ANDROID_HOME/ndk/android-ndk-r27c"
scripts/build-android.sh
```

El script compila **release**, alinea ZIP a 16 KiB y firma con una clave local de prueba. No es una identidad de publicación. `build.rs` alinea los segmentos ELF a 16 KiB; se comprueba `zipalign -c -P 16 4`, la firma y `llvm-readelf -l`. Referencias: [Tauri Android](https://v2.tauri.app/start/prerequisites/), [configuración por plataforma](https://v2.tauri.app/reference/config/), [páginas Android de 16 KiB](https://developer.android.com/guide/practices/page-sizes).

## Protocolo en el Redmi

Usa `tests/fixtures/android-legal-es.txt` y los documentos sintéticos de `tests/fixtures/`, nunca documentos de clientes.

1. Anota versión Android, ABI, RAM física, espacio libre, batería y temperatura ambiente. Conecta por USB y cierra aplicaciones pesadas.
2. Abre **Mediciones locales** y actualiza después de cada operación. Se separan `model_load`, `extraction_ocr` y `analysis`. La verificación SHA previa al modelo se mide como `model_verify`, separada de `model_load`; el tiempo total de cara al usuario incluye ambas. Los intentos fallidos también se miden. La memoria indicada es VmHWM del proceso completo, no de cada etapa aislada.
3. En el Mac ejecuta `python3 scripts/measure-android.py --seconds 300 --output artifacts/redmi-prueba-01.json`. No lee documentos ni logcat: toma PSS, RSS, temperatura de batería, presencia del proceso y estado térmico. La temperatura de batería es un indicador indirecto; anota calentamiento al tacto y throttling percibido por separado.
4. Primero libera el modelo y analiza un TXT corto. Después repite en caliente; luego OCR de imagen/PDF y análisis. Mide tres repeticiones y una sesión sostenida de 10–15 minutos. Registra carga, OCR, análisis, tiempo total, PSS/RSS máxima y temperatura inicial/final.
5. Verifica nombres, compañías, IDs, contactos, domicilio y proceso; revisa variantes sin acento y una entidad que cruce el límite entre fragmentos. Corrige una omisión, personaliza aliases y comprueba el contexto.
6. Aprueba, edita y confirma que exportar vuelve a bloquearse. Guarda, revisa y aprueba otra vez. En el PDF exportado busca literalmente todos los valores privados de la fuente, incluidos título y metadatos. No debe contener imágenes del original.
7. Cancela al leer el primer fragmento, reintenta, libera el modelo y comprueba que un archivo de modelo ausente/dañado no produce protección completada. Prueba PDF >30 páginas escaneadas y OCR vacío: deben producir error, no salida parcial aprobable.
8. Repite en modo avión. Prueba teclado, rotación, Atrás, cambio de aplicación y bloqueo/desbloqueo. Para simular muerte del proceso con un documento abierto: `adb shell am force-stop com.bluedoor.desktop`; vuelve a abrir y comprueba el aviso y la ausencia de sesión aprobada.
9. Crea un Vault de prueba, importa/exporta un archivo sintético, bloquea y desbloquea, guarda/restaura una copia cifrada y verifica que una contraseña incorrecta falla. Conserva por separado frase de recuperación y clave local de firma.

No se han medido tiempos ni temperatura del Redmi hasta conectar ese dispositivo. Las medidas de Mac o emulador no predicen su rendimiento.

## Pulido priorizado

1. Medir memoria, latencia y temperatura en el Redmi; ajustar tamaño de fragmento, lotes y número de hilos con datos.
2. Verificar OEM/selector SAF/Compartir, OCR de cámara con orientación, accesibilidad y teclado en el teléfono real.
3. Mejorar navegación por contexto; añadir vistas de imágenes incrustadas en DOCX y OCR de tablas difíciles. El visor de fuente ya incorpora zoom y desplazamiento.
4. Evaluar guardado recuperable cifrado de borradores solo como opción explícita. Mejorar continuidad tras presión de memoria, sin persistir originales por defecto.
5. Considerar GPU únicamente después de demostrar compatibilidad y una mejora medida sobre CPU. No se cambió Qwen por un modelo menor.

Resultados reales de esta entrega: [ANDROID-VALIDATION.md](ANDROID-VALIDATION.md).
