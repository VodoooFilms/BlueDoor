# Postentrenamiento de BeLegal PII v1

Ejecución local autorizada el 3 de octubre de 2026. La ejecución se interrumpió en el microstep 1.740 de 21.320; no completó la época ni la evaluación externa final. Se conserva el checkpoint de validación, pero no se promovió ningún modelo. Este documento describe la configuración ejecutada; **no afirma que haya terminado ni que el modelo ya sea mejor**. El estado real se guarda en `artifacts/posttraining/status.json` y `pipeline-status.json`; la comparación final aparecerá en `artifacts/posttraining/RESULTADO.md`.

## Configuración ejecutada

- Base: Qwen/Qwen3-1.7B, revisión `70d244cc86ccca08cf5af4e1e306ecf908b1ad5e`, Apache 2.0. Es la familia y revisión de origen del modelo instalado.
- MLX-LM 0.32.0 / MLX 0.32.3. Dependencias exactas: `artifacts/posttraining/requirements.lock.txt`.
- QLoRA sobre la base cuantizada a 4 bits, grupos de 64. Rank 16, escala 16, dropout 0,05. Ajuste de las proyecciones q/k/v/o de atención de todas las capas. Esto es postentrenamiento de adaptadores, no actualización de todos los parámetros del modelo base.
- Una época completa sobre 21.315 fragmentos originales; cinco repeticiones explícitas completan el último lote acumulado, sin descartar ejemplos. 21.320 microsteps, batch 1 y acumulación 8: 2.665 actualizaciones de optimizador. El primer intento se interrumpió durante la validación inicial por presión de memoria; todavía no había actualizaciones de pesos. Su log se conserva en `artifacts/posttraining/attempt-1`.
- AdamW; calentamiento de 100 actualizaciones, máximo de learning rate 0,0001 y descenso coseno hasta 0,00001; weight decay 0,01. Semilla 20261003.
- Contexto máximo 2.048 tokens; máximo observado 1.510. El texto se divide antes de cualquier límite y no se corta una entidad por la mitad. Ningún fragmento se trunca al entrenar.
- La función Rust de producción exportó el prompt exacto. El entrenamiento usa ese mismo formato, con pérdida solo sobre el JSON de respuesta y el token de cierre. No se mezcla una mejora de prompt con la mejora de pesos.
- Validación completa de 1.182 fragmentos cada 1.000 microsteps, al inicio y sobre los pesos finales. Se conserva el checkpoint con menor pérdida de validación; el test externo nunca elige checkpoints.
- Para reducir memoria, se evita proyectar al vocabulario las primeras 384 posiciones de instrucciones, cuya pérdida está enmascarada a cero; permanecen en la atención. Antes de actualizar pesos se comprueba que la pérdida y el número de targets coinciden con la función original de MLX en ejemplos reales cortos y largos.

## Datos y particiones

| Fuente | Entrenamiento ES | Entrenamiento EN | Validación ES | Validación EN |
|---|---:|---:|---:|---:|
| OpenPII 1M, split train oficial | 5.683 | 5.661 | 317 | 339 |
| Gretel financiero, splits train oficiales | 3.813 | 4.615 | 207 | 228 |
| MEDDOCAN, split train oficial | 881 | 0 | 51 | 0 |
| DialogPII, transcripciones | 339 | 323 | 20 | 20 |
| Total de fragmentos | 10.716 | 10.599 | 595 | 587 |

Los conteos son fragmentos, no documentos independientes. Se partió de 12.000 registros OpenPII (6.000 por idioma), 8.000 Gretel, 500 casos MEDDOCAN y 294 versiones lingüísticas de diálogos. DialogPII aporta transcripciones, **no entrenamiento de reconocimiento o redacción de ondas de audio**.

Se separan familias de diálogos antes de combinar español/inglés; las plantillas normalizadas duplicadas quedan en la misma partición. Se excluyeron 11 coincidencias con los tests congelados y 8 duplicados exactos. Quince documentos MEDDOCAN presentaron offsets que no coinciden con sus valores y se pusieron fuera del entrenamiento; no se corrigieron etiquetas suponiendo su significado. Gretel conserva calidad/conformidad ≥85 y groundedness ≥80 publicados por el dataset. Esas puntuaciones automáticas no equivalen a revisión humana.

El manifiesto de preparación contiene conteos, reglas, hashes, taxonomía y revisiones. Los 52 documentos evaluados antes permanecen congelados fuera de entrenamiento y validación. No se usó el documento privado de PayPal como dato de entrenamiento.

La política de la app conserva fechas, importes, edades, género y títulos genéricos; los targets se ajustan a ese contrato. La evaluación externa seguirá contando **todas** las etiquetas de los datasets, incluidas las categorías que la política conserva. Las limitaciones y tamaños de muestra del informe de línea base siguen aplicando.

## Cadena que continúa después del entrenamiento

1. Guardar el adaptador final y seleccionar el mejor checkpoint por validación.
2. Fusionar los adaptadores con los pesos base originales, convertir a GGUF y cuantizar Q4_K_M para el runtime real de la app. Así se evita volver a cuantizar una base que ya fue degradada a Q4.
3. Ejecutar OpenPII y Gretel con las mismas entradas y el motor Rust de producción. Verificar integridad de pesos, datos y código de inferencia.
4. Generar comparación antes/después con todos los fallos, incluidos documentos incompletos.
5. Incorporar el modelo al catálogo local solo si ambos corpus reducen residuos sin empeorar análisis incompletos, errores ni ninguna categoría cubierta por la política actual: nombres, contactos, direcciones, organizaciones e identificadores. Ese criterio de mejora **no demuestra anonimato automático**.
6. Compilar/verificar el bundle y preparar la instalación conservando el modelo y app anteriores. El modelo nuevo aparece como una opción separada al volver a abrir la app; el modelo elegido actualmente no cambia automáticamente.

Si un paso falla, se conserva el log y se detiene la cadena. Si el modelo no mejora, se conservan sus pesos y resultados, sin promoverlo. El entrenamiento y la evaluación son procesos locales; no se suben documentos, pesos ni telemetría de entrenamiento.

## Archivos y operación

- Descarga pública con versiones fijas: `scripts/fetch-posttraining.py`; preparación: `scripts/prepare-posttraining.py`.
- Entrenamiento: `scripts/train-privacy.py`.
- Conversión/evaluación/incorporación: `scripts/finish-posttraining.py`.
- Continuación de este único trabajo: `scripts/watch-posttraining.py`; sus PID están en `artifacts/posttraining/job.json`.
- Métricas: `artifacts/posttraining/metrics.jsonl`; progreso y memoria máxima quedan registrados cada diez microsteps.
- Logs completos: `artifacts/posttraining/train.log` y un log por etapa posterior.
- Adaptadores: `artifacts/posttraining/adapters` y `best-adapters`.

La ejecución mantiene despierto el sistema mientras trabaja y permite que la pantalla duerma. Puede detenerse terminando el PID de entrenamiento registrado; no se reanuda automáticamente. Los checkpoints conservados permiten continuar después, teniendo en cuenta que el optimizador necesitaría reinicializarse si no se conserva su estado.
