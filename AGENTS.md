# Repository Guidelines

## Project Overview

This repository contains `mstd-random-restaurant`, a single-crate Rust
application that posts random restaurant suggestions (photos, address,
rating) to Mastodon. It compiles to a WASI P2 WebAssembly component via
`cargo component` and runs under `wasmtime`. No test directory exists yet;
see Testing below.

## Project Structure & Module Organization

- `src/main.rs` — entry point and core application logic (city selection,
  Google Places / Gemini API calls, Mastodon posting).
- `src/wasi_http.rs` — HTTP client helper built on the `wasi` bindings.
- `src/geopoints.csv` — seed data of geo points used for random selection.
- `Cargo.toml` / `Cargo.lock` — dependencies and WASI component metadata.
- `justfile` — task runner recipes; `sample.env` — template for required
  credentials; `Containerfile` — multi-stage build producing a scratch OCI
  image of the `.wasm` component; `.circleci/config.yml` — CI pipeline
  (build + multi-arch image push on `main` only).

## Build, Test, and Development Commands

Common tasks are wrapped by `just` (run `just` or `just help` to list):

- `just check` — fast type check for `wasm32-wasip2`.
- `just build` / `just build-release` — build the component with
  `cargo component`.
- `just run` / `just run-release` — build then execute under `wasmtime` with
  HTTP and network permissions enabled.
- `just clean` — remove build artifacts.

Copy `sample.env` to `.env` and fill in `MSTDN_URI`,
`MSTDN_ACCESS_TOKEN`, and `GOOGLE_API_KEY` before running.

## Coding Style & Naming Conventions

- Rust edition 2024; standard rustfmt with `max_width = 80`
  (`.rustfmt.toml`). Run `cargo fmt --check` before committing.
- Snake_case functions/variables, CamelCase types, and `thiserror`-derived
  error enums (e.g. `MyError`). Prefer `anyhow::Result` in application code.

## Testing Guidelines

Tests, when added, live in `#[cfg(test)]` modules beside the code they test
or under `tests/`. Name them `fn it_does_x()` to match existing style. Keep
external API calls (Mastodon, Google) mocked or skip-able in CI.

## Commit & Pull Request Guidelines

History mixes short imperative messages (`justfile for random-restaurant`,
`filter with ratings...`) with Conventional Commits (`docs: update
README...`). Prefer the latter: `feat:`, `fix:`, or `docs:` prefixes with a
short imperative summary. PRs should describe the change, link related
issues, and note any new environment variables or API scope changes; CI
builds the multi-arch container image only on `main`.

## Security & Configuration Tips

Never commit `.env` or real `MSTDN_ACCESS_TOKEN` / `GOOGLE_API_KEY` values;
`sample.env` is the canonical reference. Document any new secret there with
a placeholder and comment.
