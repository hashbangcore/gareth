# Introduction

Gareth is a CLI for LLMs, oriented toward advanced GNU/Linux users.
It runs from the command line, integrates into pipelines,
and offers a minimal chat with command expansion.

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

* `chat`
  Opens a minimal chat session.

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

## Interactive Chat

### Built-in Commands

* `/help`
  Displays help.

* `/clean`
  Clears chat history.

* `/add`
  Attaches files to the context.

* `/eval`
  Evaluates an arithmetic expression.

* `/save`
  Saves the user request to a file.

* `/stream`
  Enables or disables streaming mode in the model response.

---

### Inline Command Execution

You can execute any valid shell command using:

```
#!(...)
```

The command output will be attached to the next prompt sent to the model.

Example:

```sh
➜ #!(sudo nmap scanme.nmap.org) analyze
```

The generated prompt would be similar to:

```text
:: END CHAT HISTORY (SYSTEM) ::
:: COMMAND OUTPUT (SYSTEM) ::
[section]
[command]
sudo nmap scanme.nmap.org

[stdout]
...
[end section]
:: END COMMAND OUTPUT (SYSTEM) ::
:: USER MESSAGE ::
analyze
:: END USER MESSAGE ::
:: RESPONSE ::
The command `sudo nmap scanme.nmap.org` completed a successful scan of the host `scanme.nmap.org` (IPv4: 45.33.32.156, IPv6: 2600:3c01::f03c:91ff:fe18:bb2f). Four open ports and one filtered port were detected:

- **Open ports**:
  - 22/tcp (SSH)
  - 80/tcp (HTTP)
  - 9929/tcp (nping-echo)
  - 31337/tcp (Elite)

- **Filtered port**:
  - 646/tcp (LDP)

The scan took 4.89 seconds. No critical errors were found.
```

⚠ Commands are executed in the user's shell without sandboxing.

---

## Using Paths in Prompts

Gareth automatically detects absolute and relative paths in the prompt.

Any word that:

* Starts with `/` (absolute path)
* Starts with `./` or `../` (relative path)
* And corresponds to an **existing file**

will be interpreted as a valid path and its contents will be automatically attached to the context sent to the model.

No special commands are required.

---

### In `chat` Mode

Within an interactive session:

```sh
gareth chat
```

You can write:

```
Review this file and suggest improvements: ./src/main.rs
```

If `./src/main.rs` exists and is a file, its contents will be automatically attached to the prompt.

---

### In Direct Prompt Mode

It also works from the command line:

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

### 11. Start interactive session

```sh
gareth chat
```

---

### 12. Execute command inside chat

Within the session:

```
Analyze this output:
#!(free -h)
```

---

## License

BSD 2-Clause
