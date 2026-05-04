# Mastodon Random Restaurant 🍕🌍

[![Build & Test](https://github.com/seungjin/mstd-random-restaurant/actions/workflows/build.yml/badge.svg)](https://github.com/seungjin/mstd-random-restaurant/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

A sophisticated Mastodon bot that discovers and shares charming restaurants from around the world. It picks a random global city, finds a highly-rated restaurant nearby via Google Places, generates accessible AI descriptions for images using Google Gemini, and posts a beautifully formatted status to Mastodon.

This project is a modern **WASI P2 (WebAssembly System Interface Preview 2)** component, showcasing the power of sandboxed, cross-platform WebAssembly for cloud-native automation.

## 🚀 How it Works

1.  **Global Search:** Selects a city from a curated database of 10,000+ locations, weighted by population and specific regions.
2.  **Restaurant Discovery:** Queries the Google Places API for "restaurants" within a 50km radius, filtering for those with high ratings (3.0+) and at least 10 reviews.
3.  **Visual Enrichment:** Fetches up to 4 high-quality photos of the selected restaurant.
4.  **AI Accessibility:** Uses the `gemini-1.5-flash` model to analyze the images and generate meaningful alt-text descriptions, ensuring the bot is accessible to everyone.
5.  **Mastodon Dispatch:** Formats a post with the restaurant name, address, star rating, and a Google Maps link, then uploads the images with their AI-generated alt-text.

## 🛠 Prerequisites

-   **Rust:** Latest stable version.
-   **Wasmtime:** The recommended WASM runtime.
-   **cargo-component:** Required to build WASI P2 components.
    ```bash
    cargo install cargo-component
    ```
-   **Just:** A handy task runner used for all build/run commands.

## ⚙️ Configuration

The bot requires several environment variables to function. You can provide these in a `.env` file:

| Variable | Description |
| :--- | :--- |
| `GOOGLE_API_KEY` | Your Google Cloud API key with Places and Gemini API access. |
| `MSTDN_ACCESS_TOKEN` | Access token for your Mastodon bot account. |
| `MSTDN_URI` | The domain of your Mastodon instance (e.g., `mastodon.social`). |
| `GEMINI_API_KEY` | (Optional) Separate key for Gemini if different from `GOOGLE_API_KEY`. |

## 💻 Development

The project uses `just` to simplify development workflows:

-   **Build:** `just build` (targets `wasm32-wasip2`)
-   **Run:** `just run` (executes locally via `wasmtime`)
-   **Lint:** `cargo clippy` & `cargo fmt`
-   **Test:** `just test`

## 📦 Deployment

### OCI / Docker
Build a tiny, secure WASM-based container image:
```bash
docker build -t mstd-random-restaurant -f Containerfile .
```

### Systemd (Linux)
To run the bot on a schedule (e.g., every hour), you can use the provided systemd units:

1.  Copy `mstd-random-restaurant.service` and `mstd-random-restaurant.timer` to `/etc/systemd/system/`.
2.  Update the `WorkingDirectory` and `ExecStart` paths in the service file.
3.  Enable and start the timer:
    ```bash
    systemctl enable --now mstd-random-restaurant.timer
    ```

## 📄 License

This project is dual-licensed under the **MIT License** and the **Apache License (Version 2.0)**.

Copyright (c) 2026 Seungjin Kim
