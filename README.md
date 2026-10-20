# Gareth

Gareth is a command line tool for language models. It sends a prompt to an
OpenAI-compatible endpoint and prints the answer.

It is meant to be used in pipelines: the answer goes to the standard output,
and the prompt can be written inline or piped in.

**Also in Spanish:** [Read the Spanish version](./docs/es/README.md)

---

## Environment variables

The endpoint is given through the environment:

| Variable | Required | Description |
| --- | --- | --- |
| `GARETH_URL` | yes | URL of the chat completions endpoint. |
| `GARETH_MODEL` | yes | Name of the model to use. |
| `GARETH_API_KEY` | no | API key, sent in the `Authorization: Bearer` header when the endpoint asks for one. |
| `GARETH_TRACE_SOCKET` | no | Path of the trace socket. Defaults to `$XDG_RUNTIME_DIR/gareth.trace.sock`, or `/tmp/gareth.trace.sock`. |

---

## License

BSD 2-Clause. See [LICENSE](./LICENSE).
