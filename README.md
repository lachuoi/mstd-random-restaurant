# Mastodon Random Cafe ☕️🌍

[![Test](https://github.com/seungjin/mstd-random-cafe/actions/workflows/build.yml/badge.svg)](https://github.com/seungjin/mstd-random-cafe/actions/workflows/build.yml)

A Mastodon bot that picks a random city, searches for a nearby cafe using the Google Places API, generates an AI description for alt-text using Google Gemini, and posts the results with photos to Mastodon.

Built as a modern **WASI P2 (WebAssembly System Interface Preview 2)** component.

## Features

- **Random City Selection:** Weighted selection from a global database of cities.
- **Google Places Integration:** Finds cafes with high ratings and user engagement.
- **AI Alt-Text:** Uses Google Gemini (via `gemini-1.5-flash`) to generate descriptive alt-text for accessibility.
- **WASI P2 Architecture:** Fully sandboxed execution using the latest WebAssembly standards.
- **Multi-Image Support:** Uploads and attaches up to 4 photos per post.

## Architecture

This project was recently migrated from a native Rust application to a **WASI P2 Component**. It uses:
- `wasi-http` for all outbound network requests.
- `futures` executor for async/await support in WASM.
- `cargo-component` for building the WASM component.

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Just](https://github.com/casey/just) (task runner)
- [Wasmtime](https://wasmtime.dev/) (WASM runtime)
- `cargo-component`: `cargo install cargo-component`

## Setup

1. **Clone the repository:**
   ```bash
   git clone https://github.com/seungjin/mstd-random-cafe.git
   cd mstd-random-cafe
   ```

2. **Configure environment variables:**
   Create a `.env` file or export the following:
   - `GOOGLE_API_KEY`: Your Google Cloud API key (Places & Gemini).
   - `MSTDN_ACCESS_TOKEN`: Your Mastodon application access token.
   - `MSTDN_URI`: Your Mastodon instance domain (e.g., `mastodon.social`).
   - `GEMINI_API_KEY`: (Optional) Defaults to `GOOGLE_API_KEY`.

## Development

Use `just` to manage common tasks:

- **Check:** `just check`
- **Build:** `just build`
- **Build Release:** `just build-release`
- **Run:** `just run` (runs the component using `wasmtime`)

## Deployment

The project includes a `Containerfile` to build and package the WASM component into an OCI image.

```bash
docker build -t mstd-random-cafe -f Containerfile .
```

To run as a scheduled task, you can use the provided `.service` and `.timer` files (note: these may require updates to match your specific container runtime).

## License

This project is dual-licensed under the **MIT License** and the **Apache License (Version 2.0)**.

- [LICENSE-MIT](LICENSE-MIT)
- [LICENSE-APACHE](LICENSE-APACHE)

Copyright (c) 2026 Seungjin Kim
