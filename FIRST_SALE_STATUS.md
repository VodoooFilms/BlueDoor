# Belegal _ Local demo — estado para la primera venta

**Fecha:** 12 de septiembre de 2026  
**Objetivo:** convertir una primera compra real de Belegal _ Local demo para Windows ARM64.

## Lo que ya está terminado y verificado

- La edición Windows ARM64 `0.1.0` está publicada en GitHub Releases:
  <https://github.com/VodoooFilms/VoicetoPDF/releases/tag/v0.1.0-windows-arm64>
- Instalador publicado: `Belegal _ Local demo_0.1.0_arm64-setup.exe` (17.65 MB).
- SHA-256: `84185606140f3fbcb9e9ea224850f50a47cb441720138533fbaf3288db8866e0`.
- El instalador fue probado en una instalación limpia de Windows 11 ARM64: se instalaron DLLs y runtime, y la aplicación abrió correctamente.
- La página comercial está publicada en <https://www.prompthea.com/projects/voicetopdf>.
- Belegal _ Local demo aparece como el primer producto del catálogo de Prompthea; utiliza el logo real y una captura completa de la aplicación.
- Precio mostrado: **USD 19**.
- El checkout PayPal está implementado en Prompthea y usa confirmación de orden en servidor antes de mostrar la descarga.
- Se completó una compra **Sandbox** de punta a punta: botón de compra → PayPal Sandbox → confirmación → página de éxito → descarga correcta del `.exe`.
- El botón de descarga final es un CTA naranja grande y ya fue publicado en Prompthea (commit `f0ae5c5`).

## Arquitectura comercial actual

1. El visitante compra desde Prompthea.
2. Prompthea crea y captura la orden contra PayPal.
3. Tras una captura confirmada, Prompthea entrega el enlace del instalador en GitHub Releases.
4. PayPal es el registro de pagos de esta primera etapa.

El instalador está públicamente disponible en GitHub Releases: todavía no hay DRM, cuentas de cliente ni enlaces temporales. Eso es aceptable para validar la primera venta, pero no constituye protección de descarga a escala.

## Qué falta para cobrar dinero real

No hace falta modificar el instalador ni la página. Solo hay que pasar PayPal de Sandbox a Live:

1. En PayPal Developer, crear o abrir la aplicación **Live** de Belegal _ Local demo (Merchant).
2. Crear el webhook Live con esta URL y el evento `PAYMENT.CAPTURE.COMPLETED`:
   `https://www.prompthea.com/api/paypal/voicetopdf/webhook`
3. En las variables de producción de Vercel, reemplazar únicamente los valores de PayPal por los de Live:
   - `PAYPAL_ENVIRONMENT=live`
   - `PAYPAL_CLIENT_ID`
   - `PAYPAL_CLIENT_SECRET`
   - `PAYPAL_WEBHOOK_ID`
4. Ejecutar un redeploy de producción en Vercel.
5. Hacer una compra real de USD 19 con una cuenta PayPal distinta a la cuenta vendedora, comprobar el pago en el panel de PayPal y la descarga en Prompthea.

No guardar ni copiar secretos de PayPal en Git, documentos públicos, capturas o chat.

## Criterio de éxito del challenge

Una venta real está concretada cuando PayPal Live muestra la captura de USD 19 en la cuenta vendedora y el comprador llega desde Prompthea a la descarga de Belegal _ Local demo. El misterio deja de ser misterio en ese momento.

