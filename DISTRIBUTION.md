# Distribución comercial — 12 Sep 2026

- Repositorio `VodoooFilms/Belegal _ Local demo`: privado, confirmado por GitHub API.
- Compra: https://www.prompthea.com/projects/voicetopdf — USD 19 mediante PayPal.
- Entrega: endpoint privado de Prompthea `/api/paypal/belegal-local-demo/download`. Requiere cookie firmada de la sesión de compra y verificación del pedido/captura con PayPal en cada descarga.
- El instalador se incluye fuera de `public/` en el repositorio privado `VodoooFilms/Prompthea`; commit de despliegue `b73164c`.
- No publicar enlaces de GitHub como entrega al comprador. No volver a hacer público este repositorio: expondría nuevamente releases e historial.
- El acceso dura siete días en el navegador comprador. Para pérdida de sesión u otra descarga, el cliente contacta Prompthea con el recibo; no debe pagar de nuevo. Recuperación automática pendiente.

## Verificación realizada

- Build Next.js y TypeScript correctos.
- Pruebas de sesión manipulada/caducada, producto incorrecto, importe/moneda incorrectos, pago pendiente y reembolso.
- Prueba de integración de rutas reales con PayPal simulado: descarga completa de 18.510.601 bytes, SHA-256 `84185606140f3fbcb9e9ea224850f50a47cb441720138533fbaf3288db8866e0`.
- Producción: descarga anónima 403, ruta estática 404, enlaces de código/release retirados de la página comercial. Inicio de checkout LIVE y comprobación de integridad del instalador correctos. Orden no aprobada ni pagada; no cuenta como venta.
- GitHub sin autenticación: repositorio, release ARM64, EXE, MSI y API de releases devuelven 404.
- No se verificó una compra real pagada de extremo a extremo en este cambio.

## Licencia y límites

Esta migración protege la distribución desde nuestros servidores. No es DRM y no impide que alguien que ya descargó un archivo conserve o comparta su copia. El instalador 0.1.0 conserva los avisos y licencia con que se distribuyó; privatizar el repositorio no revoca permisos anteriores. Las licencias de dependencias y modelos se conservan. No se ha aplicado una nueva licencia propietaria ni reconstruido los instaladores en esta migración.
