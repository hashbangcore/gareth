# Introducción

Gareth es un CLI para LLMs, orientado a usuarios avanzados de GNU/Linux.
Funciona desde la línea de comandos y se integra en pipelines.

**También en inglés:** [Ver README en inglés](../../)

---

## Instalación

Gareth se puede instalar de varias formas:

### Usando Cargo

```bash
cargo install gareth
```

### Usando Nix Flakes

```bash
nix profile add github:hashbangcore/gareth
```

---

## Variables de entorno

Gareth es agnóstico respecto al proveedor: habla con cualquier endpoint compatible con OpenAI y
no incluye ningún proveedor por defecto, así que el destino se indica por configuración.

* `GARETH_URL` (obligatoria)
  URL del endpoint de *chat completions*.

* `GARETH_MODEL` (obligatoria)
  Nombre del modelo a usar.

* `GARETH_API_KEY` (opcional)
  Clave API, que se envía como bearer token cuando el endpoint la pide.

* `GARETH_TRACE_SOCKET` (opcional)
  Ruta del socket de depuración de `--trace`. Por defecto
  `$XDG_RUNTIME_DIR/gareth.trace.sock`, o `/tmp/gareth.trace.sock`.

---

## Uso

```
Uso: gareth [OPCIONES] [PROMPT] [COMANDO]
```

Si hay entrada por `stdin`, se agrega como contexto adicional.

---

### Comandos

* `commit`
  Genera un mensaje de commit a partir de los cambios en *staging*.

* `completion`
  Genera scripts de autocompletado para la shell.

* `prompt`
  Envía un prompt al modelo y muestra la respuesta.

---

### Argumentos

* `[PROMPT]`
  Prompt enviado al modelo.

---

### Opciones

* `-v, --verbose`
  Habilita la salida detallada.

* `-t, --trace`
  Muestra los prompts enviados y las respuestas recibidas (modo depuración).

* `-h, --help`
  Muestra la ayuda.

* `-V, --version`
  Muestra la versión.

Puedes usar la opción `--trace` para revisar los prompts que se envían:

```
gareth --trace
```

---

## Rutas en prompts

Gareth detecta automáticamente rutas absolutas y relativas en el prompt.

Cualquier palabra que:

* Comience con `/` (ruta absoluta)
* Comience con `./` o `../` (ruta relativa)
* Y corresponda a un **archivo existente**

será interpretada como ruta válida y su contenido se adjuntará automáticamente al contexto enviado al modelo.

No es necesario usar comandos especiales.

---

### En un prompt

```sh
gareth "Traduce este fichero ./README.en.md"
```

Si `./README.en.md` existe, su contenido se incluirá automáticamente en el contexto antes de enviar la solicitud al modelo.

---

### Comportamiento

* Solo se adjuntan archivos existentes.
* No se adjuntan directorios.
* Se pueden incluir múltiples rutas en un mismo prompt.
* Las rutas se resuelven desde el directorio actual.

---

## Ejemplos

### 1. Prompt directo

```sh
gareth "Explica cómo funciona io_uring en Linux"
```

---

### 2. Prompt sin comillas

```sh
gareth Explica el modelo de ownership en Rust
```

---

### 3. Usando `stdin` como contexto

```sh
cat Cargo.toml | gareth "Describe las dependencias principales"
```

---

### 4. Analizando salida de un comando

```sh
dmesg | tail -n 50 | gareth "¿Hay errores relevantes en estos logs?"
```

---

### 5. Generar mensaje de commit desde staging

```sh
gareth commit | git commit -F -
```

---

### 6. Generar mensaje con convención personalizada

```sh
gareth commit -c .repo/convencion.txt
```

---

### 7. Modo verbose

```sh
gareth -v "Explica qué es un mutex en Rust"
```

---

### 8. Inspeccionar prompts enviados (modo depuración)

Gareth ejecuta un socket de depuración que escucha todos los prompts enviados
por instancias en ejecución.

```sh
gareth --trace
```

---

### 9. Usar proveedor personalizado

```sh
export GARETH_URL="https://api.example.com/v1/chat/completions"
export GARETH_MODEL="modelo"
export GARETH_API_KEY="api-key"

gareth "Describe el algoritmo de scheduling CFS"
```

---

### 10. Generar autocompletado para la shell

```sh
gareth completion bash
```

---

## Licencia

BSD 2-Clause
