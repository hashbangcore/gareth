# Gareth

Gareth es una herramienta de línea de comandos para modelos de lenguaje.
Envía un prompt a un servidor compatible con OpenAI e imprime la respuesta.

Está pensada para usarse en tuberías: la respuesta va a la salida estándar y
el prompt se puede escribir en línea o por tubería.

**También en inglés:** [Leer el README en inglés](../../)

---

## Variables de entorno

El servidor se indica por entorno:

| Variable | Obligatoria | Descripción |
| --- | --- | --- |
| `GARETH_URL` | sí | URL del punto de acceso de *chat completions*. |
| `GARETH_MODEL` | sí | Nombre del modelo a usar. |
| `GARETH_API_KEY` | no | Clave API, que se envía en la cabecera `Authorization: Bearer` cuando el servidor la pide. |
| `GARETH_TRACE_SOCKET` | no | Ruta del socket de traza. Por defecto `$XDG_RUNTIME_DIR/gareth.trace.sock`, o `/tmp/gareth.trace.sock`. |

---

## Licencia

BSD 2-Clause. Ver [LICENSE](../../LICENSE).
