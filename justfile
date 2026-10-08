mod dist

set unstable := true

default: watch

release:
    just  release-info
    just  release-build

release-info:
    just dist::info

release-build:
    just dist::tag
    just dist::build
    just dist::artifacts
    just dist::checksum
    just dist::upload

error:
    cargo run --bin gareth -- "hi" > error.txt 2>&1

commit hint="":
    gareth commit "{{ hint }}" | git commit -F - --edit

install:
    cargo install --path . --bin gareth

install-dev:
    cargo install --path . --bin gareth-dev

test-envrc hint="true":
    @direnv allow
    direnv exec . cargo run --bin gareth  -- -v  "{{ hint }}"
    @direnv disallow

show-code:
    find src -type f -exec sh -c 'for f; do echo "--- $f ---"; cat "$f"; done' sh {} + | larry "tree -I 'docs|target'" 

watch task="check":
    cargo watch -c -w src -x {{ task }}
