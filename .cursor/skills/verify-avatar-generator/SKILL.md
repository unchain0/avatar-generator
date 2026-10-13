# verify-avatar-generator

Verify the fullstack avatar app (pt-BR/en, server-rendered, no CDN).

1. `cargo test`: unit (hash/seed, validation) + integration with axum router + wiremock/fake upstream (no robohash in tests).
2. `cargo fmt` and `clippy -D warnings`; `cargo test` green.
3. Run locally: `PORT=8080 DATA_DIR=./data cargo run`. `curl /healthz` -> ok.
4. POST generate (count, seed, set/bg, size) -> PNG returned; check gallery renders; per-avatar download works; download-all produces zip; expiry cleanup deletes old files.
5. Prod check against `https://avatar.unchain0.com`.
