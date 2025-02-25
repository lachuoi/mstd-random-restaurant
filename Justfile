default:
    just --list

build:
    #!/usr/bin/env bash
    cargo clean
    cargo build
    
test:
    #!/usr/bin/env bash
    #wasmtime wast test.wast
    cargo test -- --nocapture

run:
    #!/usr/bin/env bash
    #export OPENAI_API_KEY="hello_I_am_lisa"
    cargo run

clean:
	cargo clean


