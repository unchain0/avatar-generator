---
name: verify-avatar-generator
description: >-
  Launch and drive the Avatar Generator Axum web app (gallery, generate,
  download, clear-all, language) over HTTP the way a user does, then capture
  proof. Use when a change to the Rust fullstack app needs a real user-path
  check, a feature in the map needs evidence, or a local instance must be
  proven healthy before driving.
---

# Verify Avatar Generator

Primary surface: the server-rendered web UI (`avatar-generator`, Axum + askama +
vanilla JS). A person opens `/`, fills the generate form, and works the gallery
(per-avatar download/delete, download-all ZIP, clear-all, PT/EN language). The
browser only talks to this origin; RoboHash is fetched server-side.

Secondary surfaces this skill does not drive:

- Unit and integration tests (`cargo test`, wiremock fake upstream).
- The Docker image / production host `https://avatar.unchain0.com`.

Two instances can run side by side. Each launch picks a free port on loopback
and its own `DATA_DIR` under `/tmp/avatar-generator-verify/<run-id>/`. Never
drive a process this skill did not start, and never point two drives at the
same scratch directory.

## Launch

From the repository root, with stable Rust on `PATH` (`rustc` 1.80+; CI and
`Cargo.toml` expect 1.80):

```bash
cargo build
.cursor/skills/verify-avatar-generator/scripts/launch.sh
```

`launch.sh`:

1. Creates scratch `/tmp/avatar-generator-verify/<run-id>/` with `data/`,
   `cookies.txt`, `server.log`, and `state.env`.
2. Picks a free TCP port on `127.0.0.1` in 18100–18200.
3. Starts `target/debug/avatar-generator` (or `cargo run --quiet` if the binary
   is missing) with a clean env:
   - `PORT=<port>`
   - `DATA_DIR=<scratch>/data`
   - `STATIC_DIR=<repo>/static`
   - `RUST_LOG=info,avatar_generator=debug`
   - `TTL_DAYS=1`
   - `TRUST_PROXY=false`
   - `ROBOHASH_BASE_URL` only when already set in the launching shell (default
     upstream is `https://robohash.org`)
4. Waits until `GET http://127.0.0.1:<port>/healthz` returns body `ok` and the
   process is still alive (up to 30s).

The last line of stdout is `SCRATCH=/tmp/avatar-generator-verify/<run-id>`.
Pass that path to doctor, drive, and cleanup. State file mode is 600.

Ready means healthz is `ok` and the pid recorded in `state.env` still owns the
listen socket. The server log line is `listening` with the chosen port.

If launch fails after starting the process, the script runs cleanup for that
scratch path.

## Doctor

Read-only. Run before driving, and again whenever anything looks off:

```bash
.cursor/skills/verify-avatar-generator/scripts/doctor.sh "$SCRATCH"
```

It exits 0 only when all of these hold:

- The pid in `$SCRATCH/state.env` is alive and its command line contains
  `avatar-generator`.
- That pid (or a direct child) owns the listening socket for `PORT`.
- `GET /healthz` returns HTTP 200 and body `ok`.
- `GET /` returns HTML containing `generateForm`, `id="gallery"`, and the
  default brand string `Gerador de Avatares RoboHash`.
- `GET /static/app.js` and `GET /static/style.css` return 200.
- `DATA_DIR` from state exists and is writable by the current user.

Doctor prints `doctor: ok origin=… pid=…` on success. A failure means do not
drive this process — launch a new instance. Do not attach to an ambient server
on port 8080.

## Drive

There is no Playwright suite in this repo. Drive the real HTTP surface the UI
uses (same paths and JSON the vanilla JS posts), with a cookie jar so the
session matches a browser. Prefer the helpers over ad-hoc curl.

```bash
.cursor/skills/verify-avatar-generator/scripts/drive-generate.sh "$SCRATCH"
```

That follows `features/generate-avatars.md` end to end: invalid payload → 4xx,
valid generate → JSON avatars, gallery HTML lists cards, `/avatar/:id` is PNG,
`/download/:id` is an attachment, `/download-all` is a ZIP, single delete and
`/clear-all` empty the session. Evidence lands under the skill's `evidence/`
tree (see Evidence).

Other mapped features:

```bash
.cursor/skills/verify-avatar-generator/scripts/drive-home.sh "$SCRATCH"
.cursor/skills/verify-avatar-generator/scripts/drive-session.sh "$SCRATCH"
.cursor/skills/verify-avatar-generator/scripts/drive-language.sh "$SCRATCH"
```

Or one feature by name:

```bash
.cursor/skills/verify-avatar-generator/scripts/drive.sh "$SCRATCH" generate-avatars
```

Stable handles (from `templates/index.html` + `static/app.js`):

| Handle | Role |
| --- | --- |
| `POST /generate` JSON | `{count, seed, set, background, size, ttl_days}` |
| `GET /avatar/:id` | cached PNG, `content-type: image/png` |
| `GET /download/:id` | PNG attachment `filename="<id>.png"` |
| `GET /download-all` | ZIP of the session |
| `DELETE /avatar/:id` | session-owned delete → `{"success":true}` |
| `POST /clear-all` | wipe session → `{"success":true,"removed":N}` |
| Cookie `avatar_session_id` | session scope (HttpOnly, set on `/`) |
| Cookie `lang` | `pt-br` (default) or `en` |
| `#generateForm`, `#gallery`, `#generateBtn` | DOM ids the UI binds |

Do not call RoboHash from the client. Do not write files into `DATA_DIR` by
hand to fake a gallery. Do not drive production.

## Evidence

Write proof under:

```text
.cursor/skills/verify-avatar-generator/evidence/<feature>-<run-id>/
```

Cleanup never deletes that tree. Each run should include:

- `transcript.txt` — every request method/path, status, and key response fields
- `healthz.txt` — raw `ok` body
- `index-before.html` / `index-after.html` — gallery HTML around the action
- for generate: `generate.json`, one `avatar-<id>.png`, `download-<id>.png`,
  `download-all.zip`, and `clear-all.json`
- `PROOF.md` — feature id, entry point, pass/fail, and the side effect checked
  (files under `DATA_DIR`, cookie values without secrets)

Standards:

- Exercise the real user path (`/generate`, gallery routes), not internal store
  helpers or test-only hooks.
- Capture the action and the resulting state (JSON count + HTML card + PNG
  bytes + empty gallery after clear).
- Verify side effects: PNG files appear under `$DATA_DIR`, and disappear after
  delete/clear for that session.
- Upstream RoboHash is a real boundary; a missing network fails generate
  honestly. Optional `ROBOHASH_BASE_URL` may point at a compatible stand-in,
  but default verification uses the live upstream.

## Cleanup

```bash
.cursor/skills/verify-avatar-generator/scripts/cleanup.sh "$SCRATCH"
```

Stops only the pid recorded in `$SCRATCH/state.env` after confirming the
command line still contains `avatar-generator` and that pid still owns `PORT`
(`SIGTERM`, then `SIGKILL`). Deletes the scratch directory. Does not delete
`.cursor/skills/verify-avatar-generator/evidence/`. Does not `pkill` by name.
Does not touch `./data` in the repo.

Run cleanup after a failed launch or drive too, once any log lines you need
are copied into the evidence directory.

## Helpers

| Script | Invocation |
| --- | --- |
| `scripts/launch.sh` | `.cursor/skills/verify-avatar-generator/scripts/launch.sh` |
| `scripts/doctor.sh` | `.cursor/skills/verify-avatar-generator/scripts/doctor.sh "$SCRATCH"` |
| `scripts/drive.sh` | `.cursor/skills/verify-avatar-generator/scripts/drive.sh "$SCRATCH" <feature>` |
| `scripts/drive-home.sh` | `.cursor/skills/verify-avatar-generator/scripts/drive-home.sh "$SCRATCH"` |
| `scripts/drive-generate.sh` | `.cursor/skills/verify-avatar-generator/scripts/drive-generate.sh "$SCRATCH"` |
| `scripts/drive-session.sh` | `.cursor/skills/verify-avatar-generator/scripts/drive-session.sh "$SCRATCH"` |
| `scripts/drive-language.sh` | `.cursor/skills/verify-avatar-generator/scripts/drive-language.sh "$SCRATCH"` |
| `scripts/cleanup.sh` | `.cursor/skills/verify-avatar-generator/scripts/cleanup.sh "$SCRATCH"` |

`scripts/common.sh` is sourced by the others; do not invoke it directly.

Feature map: `features/README.md` and the files it lists. Keep the map honest
with `/maintain-verification-skill` as the app changes.
