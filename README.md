# Avatar Generator

A fullstack Rust web app that generates seeded robot avatars from the
[RoboHash](https://robohash.org/) library and serves them from your own domain.

The server renders the gallery with [askama](https://crates.io/crates/askama),
proxies and caches every avatar under `DATA_DIR`, and never loads assets from a
CDN. The UI ships in Portuguese (pt-BR, default) and English (en).

## Features

- Generate 1 to 50 avatars per batch from a custom seed or a random SHA-256 seed.
- Choose the RoboHash set (`set1`..`set5`), background (`bg1`, `bg2`, or none),
  and size (50 to 1024).
- Gallery per browser session, with per-avatar download and a "download all" ZIP.
- Per-session "clear all" and per-avatar delete.
- Server-side proxy and cache with an upstream timeout and a concurrency limit.
- Per-IP rate limit on generation.
- Expiring avatars: each batch gets a TTL in days; a background task deletes
  expired files every 10 minutes.
- `GET /healthz` returns `200 ok` for probes.

## Requirements

- Rust stable (1.80+).
- Network access to `https://robohash.org` at runtime (or point `ROBOHASH_BASE_URL`
  at a compatible server).

## Run locally

```bash
PORT=8080 DATA_DIR=./data RUST_LOG=info cargo run
```

Open <http://localhost:8080>, pick your options, and press **Gerar**.

## Configuration

All configuration is read from the environment.

| Variable | Default | Description |
| --- | --- | --- |
| `PORT` | `8080` | HTTP listen port. |
| `DATA_DIR` | `./data` | Directory for cached avatars and the session index. |
| `STATIC_DIR` | `./static` | Directory served under `/static`. |
| `ROBOHASH_BASE_URL` | `https://robohash.org` | Upstream avatar base URL. |
| `TTL_DAYS` | `1` | Default avatar lifetime in days (1..30). |
| `RUST_LOG` | `info` | `tracing` filter, for example `avatar_generator=debug`. |

## Docker

```bash
mkdir -p $PWD/data && chown 10001:$PWD/data $PWD/data
docker run --rm -p 8080:8080 -v "$PWD/data:/data" avatar-generator
```

The image builds with `cargo-chef`, runs as a non-root user (UID 10001), ships
`ca-certificates`, and exposes port `8080`. Creating the directory beforehand
and chowning it to the image's user ensures the first `Store::save` can create
the PNG and `index.json` without a 502 error.

Alternatively, a named volume can be used:

```bash
docker run --rm -p 8080:8080 -v avatar-generator-data:/data avatar-generator
```

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/healthz` | Liveness probe, returns `ok`. |
| `GET` | `/` | Rendered gallery for the current session. |
| `POST` | `/generate` | Generate a batch. JSON: `count`, `seed`, `set`, `background`, `size`, `ttl_days`. |
| `GET` | `/avatar/:id` | Cached PNG for an avatar. |
| `GET` | `/download/:id` | Download a single PNG. |
| `DELETE` | `/avatar/:id` | Delete an avatar owned by the session. |
| `GET` | `/download-all` | ZIP of the session's avatars. |
| `POST` | `/clear-all` | Delete every avatar in the session. |

Example:

```bash
curl -X POST http://localhost:8080/generate \
  -H 'content-type: application/json' \
  -d '{"count":3,"seed":"hello","set":"set2","background":"bg1","size":200,"ttl_days":1}'
```

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Unit tests cover seed hashing, request validation, and expiry cleanup with a
temporary directory. Integration tests drive the real Axum router against a
`wiremock` fake upstream, so tests never call RoboHash.
