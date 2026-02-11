set unstable := true

default: run

run:
    cargo run 

install:
    cargo install

fmt:
    just --fmt
    cargo fmt

commit hint="":
    netero commit "{{ hint }}" | git commit --edit -F -
