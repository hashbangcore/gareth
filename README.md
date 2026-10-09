# Introduction

Gareth is a CLI for LLMs, oriented toward advanced GNU/Linux users.
It runs from the command line and integrates into pipelines.

**Also in Spanish:** [View README in Spanish](./docs/es/)

---

## Installation

Gareth can be installed in several ways:

### Using Cargo

```bash
cargo install gareth
```

### Using Nix Flakes

```bash
nix profile add github:hashbangcore/gareth
```

---

## Environment Variables

Gareth is provider agnostic: it speaks to any OpenAI-compatible endpoint and ships without a
built-in provider, so the destination is provided through configuration.

* `GARETH_URL` (required)
  Chat completions endpoint URL.

* `GARETH_MODEL` (required)
  Name of the model to use.

* `GARETH_API_KEY` (optional)
  API key, sent as a bearer token when the endpoint asks for one.

* `GARETH_TRACE_SOCKET` (optional)
  Path of the `--trace` debug socket. Defaults to `$XDG_RUNTIME_DIR/gareth.trace.sock`,
  or `/tmp/gareth.trace.sock`.

---

## Usage

```
Usage: gareth [OPTIONS] [PROMPT] [COMMAND]
```

If there is input from `stdin`, it is added as additional context.

---

### Commands

* `commit`
  Generates a commit message from staged changes.

* `completion`
  Generates shell autocompletion scripts.

* `prompt`
  Sends a prompt to the model and displays the response.

---

### Arguments

* `[PROMPT]`
  Prompt sent to the model.

---

### Options

* `-v, --verbose`
  Enables verbose output.

* `-t, --trace`
  Shows the prompts sent and the responses received (debug mode).

* `-h, --help`
  Displays help.

* `-V, --version`
  Displays the version.

You can use the `--trace` option to inspect the prompts being sent:

```
gareth --trace
```

---

## Paths in Prompts

Gareth automatically detects absolute and relative paths in the prompt.

Any word that:

* Starts with `/` (absolute path)
* Starts with `./` or `../` (relative path)
* And corresponds to an **existing file**

will be interpreted as a valid path and its contents will be automatically attached to the context sent to the model.

No special commands are required.

---

### In a Prompt

```sh
gareth "Translate this file ./README.en.md"
```

If `./README.en.md` exists, its contents will be automatically included in the context before sending the request to the model.

---

### Behavior

* Only existing files are attached.
* Directories are not attached.
* Multiple paths can be included in the same prompt.
* Paths are resolved from the current directory.

---

## Examples

### 1. Direct prompt

```sh
gareth "Explain how io_uring works in Linux"
```

---

### 2. Prompt without quotes

```sh
gareth Explain the ownership model in Rust
```

---

### 3. Using `stdin` as context

```sh
cat Cargo.toml | gareth "Describe the main dependencies"
```

---

### 4. Analyzing command output

```sh
dmesg | tail -n 50 | gareth "Are there any relevant errors in these logs?"
```

---

### 5. Generate commit message from staging

```sh
gareth commit | git commit -F -
```

---

### 6. Generate message with custom convention

```sh
gareth commit -c .repo/convencion.txt
```

---

### 7. Verbose mode

```sh
gareth -v "Explain what a mutex is in Rust"
```

---

### 8. Inspect sent prompts (debug mode)

Gareth runs a debug socket that listens to all prompts sent by running instances.

```sh
gareth --trace
```

---

### 9. Use custom provider

```sh
export GARETH_URL="https://api.example.com/v1/chat/completions"
export GARETH_MODEL="model"
export GARETH_API_KEY="api-key"

gareth "Describe the CFS scheduling algorithm"
```

---

### 10. Generate shell autocompletion

```sh
gareth completion bash
```

---

## License

BSD 2-Clause
