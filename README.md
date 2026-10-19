# Gareth

Gareth is a command line tool for language models. It sends a prompt to an
OpenAI-compatible endpoint and prints the answer.

It is meant to be used in pipelines: the answer goes to the standard output,
and the prompt can be written inline or piped in.

**Also in Spanish:** [Read the Spanish version](./docs/es/README.md)

---

## Installation

### Cargo

```bash
cargo install gareth
```

### Nix flakes

```bash
nix profile add github:hashbangcore/gareth
```

---

## Configuration

Gareth ships without a provider. The destination is given through the
environment:

* `GARETH_URL` (required)
  URL of the chat completions endpoint.

* `GARETH_MODEL` (required)
  Name of the model to use.

* `GARETH_API_KEY` (optional)
  API key, sent as a bearer token when the endpoint asks for one.

* `GARETH_TRACE_SOCKET` (optional)
  Path of the trace socket. Defaults to `$XDG_RUNTIME_DIR/gareth.trace.sock`,
  or `/tmp/gareth.trace.sock`.

---

## Usage

```
Usage: gareth [OPTIONS] [PROMPT]... [COMMAND]
```

---

### Commands

* `repo commit`
  Generates a commit message from the changes in the repository.

* `prompt`
  Sends a prompt to the model and prints the answer.

* `completion`
  Generates shell completion scripts.

---

### Options

* `-v, --verbose`
  Prints the prompt that was sent and the answer that came back.

* `-t, --trace`
  Starts a trace server that prints the raw traffic of every running
  instance.

* `-h, --help`
  Prints the help.

* `-V, --version`
  Prints the version.

---

### The prompt

The prompt is the text given as arguments, the text read from the standard
input, or both:

```sh
gareth "Explain how io_uring works in Linux"
```

```sh
cat Cargo.toml | gareth "Describe the main dependencies"
```

---

## Generating commit messages

`gareth repo commit` reads the changes in the repository, sends them to the
model and prints a commit message.

```sh
gareth repo commit | git commit -F -
```

The message is written to the standard output, so it can be reviewed before
being used:

```sh
gareth repo commit | less
```

---

### Selecting the changes

Without arguments, the whole index is described: the changes that are staged
are the ones the message is written for.

With file arguments, only those paths are described. The changes measured are
the ones `git commit` would commit for those paths, that is, the working tree
against `HEAD`:

```sh
gareth repo commit src/main.rs | git commit src/main.rs -F -
```

```sh
gareth repo commit src tests | git commit src tests -F -
```

Directories are accepted and select everything under them.

---

### Adding context

Two options add context to the prompt. Both accept a text or the path to a
text file, and both are optional:

* `--hint`
  Extra context about the change, written by the person making it.

* `--context`
  A description of the project the changes belong to.

```sh
gareth repo commit --hint "removing the code that was left over"
```

```sh
gareth repo commit --context ./AGENTS.md
```

A file that does not hold text is refused, and a file that does not exist is
reported as such.

The options may come before or after the file arguments, but the file
arguments themselves have to be given together.

---

### The convention

The commit types and scopes are defined by a convention file. Without one, the
built-in convention is used.

```sh
gareth repo commit -c .repo/convention.txt
```

---

## Tracing

`gareth --trace` starts a socket that receives the prompts and the answers of
every running instance, and prints them as they arrive.

```sh
gareth --trace
```

---

## Shell completion

```sh
gareth completion bash
```

---

## License

BSD 2-Clause
