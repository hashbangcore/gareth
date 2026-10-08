# Introducción

Gareth es un CLI para LLMs, orientado a usuarios avanzados de GNU/Linux.
Funciona desde la línea de comandos, se integra en pipelines y
ofrece un chat minimalista con expansión de comandos.

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

* `chat`
  Abre una sesión de chat minimalista.

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

## Chat interactivo

### Comandos integrados

* `/help`
  Muestra la ayuda.

* `/clean`
  Limpia el historial del chat.

* `/add`
  Adjunta archivos al contexto.

* `/eval`
  Evalúa una expresión aritmética.

* `/save`
  Guarda la solicitud del usuario en un archivo.

* `/stream`
  Activa o desactiva el modo *streaming* en la respuesta del modelo.

---

### Ejecución de comandos en línea

Puedes ejecutar cualquier comando válido en la shell usando:

```
#!(...)
```

El resultado del comando se adjuntará en el siguiente prompt enviado al modelo.

Ejemplo:

```sh
➜ #!(sudo nmap scanme.nmap.org) analiza
```

El prompt generado sería similar a:

```text
:: END HISTORIAL CHAT (SYSTEM) ::
:: COMMAND OUTPUT (SYSTEM) ::
[section]
[command]
sudo nmap scanme.nmap.org

[stdout]
...
[end section]
:: END COMMAND OUTPUT (SYSTEM) ::
:: USER MESSAGE ::
analiza
:: END USER MESSAGE ::
:: RESPONSE ::
El comando `sudo nmap scanme.nmap.org` completó un escaneo exitoso del host `scanme.nmap.org` (IPv4: 45.33.32.156, IPv6: 2600:3c01::f03c:91ff:fe18:bb2f). Se detectaron 4 puertos abiertos y 1 puerto filtrado:

- **Puertos abiertos**:
  - 22/tcp (SSH)
  - 80/tcp (HTTP)
  - 9929/tcp (nping-echo)
  - 31337/tcp (Elite)

- **Puerto filtrado**:
  - 646/tcp (LDP)

El escaneo tardó 4.89 segundos. No se encontraron errores críticos.
```

⚠ Los comandos se ejecutan en la shell del usuario sin sandbox.

---

## Uso de rutas en prompts

Gareth detecta automáticamente rutas absolutas y relativas en el prompt.

Cualquier palabra que:

* Comience con `/` (ruta absoluta)
* Comience con `./` o `../` (ruta relativa)
* Y corresponda a un **archivo existente**

será interpretada como ruta válida y su contenido se adjuntará automáticamente al contexto enviado al modelo.

No es necesario usar comandos especiales.

---

### En modo `chat`

Dentro de una sesión interactiva:

```sh
gareth chat
```

Puedes escribir:

```
Revisa este archivo y sugiere mejoras: ./src/main.rs
```

Si `./src/main.rs` existe y es un fichero, su contenido se adjuntará al prompt automáticamente.

---

### En modo prompt directo

También funciona desde la línea de comandos:

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

### 11. Iniciar sesión interactiva

```sh
gareth chat
```

---

### 12. Ejecutar comando dentro del chat

Dentro de la sesión:

```
Analiza esta salida:
#!(free -h)
```

---

## Licencia

BSD 2-Clause
