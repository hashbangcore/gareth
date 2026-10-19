# Gareth

Gareth es una herramienta de línea de comandos para modelos de lenguaje.
Envía un prompt a un endpoint compatible con OpenAI e imprime la respuesta.

Está pensada para usarse en pipelines: la respuesta va a la salida estándar y
el prompt se puede escribir en línea o por tubería.

**También en inglés:** [Leer el README en inglés](../../)

---

## Instalación

### Cargo

```bash
cargo install gareth
```

### Nix flakes

```bash
nix profile add github:hashbangcore/gareth
```

---

## Configuración

Gareth no incluye ningún proveedor. El destino se indica por entorno:

* `GARETH_URL` (obligatoria)
  URL del endpoint de *chat completions*.

* `GARETH_MODEL` (obligatoria)
  Nombre del modelo a usar.

* `GARETH_API_KEY` (opcional)
  Clave API, que se envía como bearer token cuando el endpoint la pide.

* `GARETH_TRACE_SOCKET` (opcional)
  Ruta del socket de traza. Por defecto `$XDG_RUNTIME_DIR/gareth.trace.sock`,
  o `/tmp/gareth.trace.sock`.

---

## Uso

```
Usage: gareth [OPTIONS] [PROMPT]... [COMMAND]
```

---

### Comandos

* `repo commit`
  Genera un mensaje de commit a partir de los cambios del repositorio.

* `prompt`
  Envía un prompt al modelo e imprime la respuesta.

* `completion`
  Genera scripts de completado para el shell.

---

### Opciones

* `-v, --verbose`
  Imprime el prompt enviado y la respuesta recibida.

* `-t, --trace`
  Levanta un servidor de traza que imprime el tráfico de todas las instancias
  en marcha.

* `-h, --help`
  Imprime la ayuda.

* `-V, --version`
  Imprime la versión.

---

### El prompt

El prompt es el texto que se pasa como argumentos, el texto que se lee de la
entrada estándar, o ambos:

```sh
gareth "Explain how io_uring works in Linux"
```

```sh
cat Cargo.toml | gareth "Describe the main dependencies"
```

---

## Generar mensajes de commit

`gareth repo commit` lee los cambios del repositorio, los envía al modelo e
imprime un mensaje de commit.

```sh
gareth repo commit | git commit -F -
```

El mensaje se escribe en la salida estándar, así que se puede revisar antes de
usarlo:

```sh
gareth repo commit | less
```

---

### Seleccionar los cambios

Sin argumentos se describe el índice completo: los cambios que están en el
área de preparación son aquellos para los que se escribe el mensaje.

Con argumentos de fichero, solo se describen esas rutas. Los cambios medidos
son los que `git commit` commitearía para esas rutas, es decir, el árbol de
trabajo contra `HEAD`:

```sh
gareth repo commit src/main.rs | git commit src/main.rs -F -
```

```sh
gareth repo commit src tests | git commit src tests -F -
```

Los directorios se aceptan y seleccionan todo lo que hay debajo.

---

### Añadir contexto

Dos opciones añaden contexto al prompt. Ambas aceptan un texto o la ruta de
un fichero de texto, y ambas son opcionales:

* `--hint`
  Contexto extra sobre el cambio, escrito por quien lo hace.

* `--context`
  Descripción del proyecto al que pertenecen los cambios.

```sh
gareth repo commit --hint "se borra el código que había sobrado"
```

```sh
gareth repo commit --context ./AGENTS.md
```

Un fichero que no contiene texto se rechaza, y un fichero que no existe se
informa como tal.

Las opciones pueden ir antes o después de los argumentos de fichero, pero los
argumentos de fichero tienen que ir juntos.

---

### La convención

Los tipos y los scopes del commit los define un fichero de convención. Sin
fichero se usa la convención integrada.

```sh
gareth repo commit -c .repo/convention.txt
```

---

## Traza

`gareth --trace` levanta un socket que recibe los prompts y las respuestas de
todas las instancias en marcha, y los va imprimiendo.

```sh
gareth --trace
```

---

## Completado del shell

```sh
gareth completion bash
```

---

## Licencia

BSD 2-Clause
