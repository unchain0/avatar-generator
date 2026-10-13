# Home and health

Home is the first page a user sees: the generate form, an empty or populated
gallery, language toggles, and a live process that answers the health probe.

## Sub-features

- `healthz-ok` returns HTTP 200 and the plain body `ok`.
- `index-render` serves the pt-BR gallery shell with the generate form.
- `session-cookie` sets `avatar_session_id` on first visit.
- `static-assets` serves `/static/app.js` and `/static/style.css` without a CDN.

## How to get to it (user POV)

- Open `http://127.0.0.1:<port>/` in a browser.
- Hit the liveness URL `GET /healthz` (operators and probes).
- Load the page so the browser requests `/static/style.css` and `/static/app.js`.

## Driving it with curl-helpers

Preconditions:

- A verify instance is healthy at `$ORIGIN` from `$SCRATCH/state.env`.
- `scripts/doctor.sh "$SCRATCH"` prints `doctor: ok`.

- **Health probe.** Request liveness. Run `scripts/drive-home.sh "$SCRATCH"` (or `curl -sS "$ORIGIN/healthz"`). Body is exactly `ok` with status 200.
- **Open home.** Fetch the index with a cookie jar. Run the same helper (or `curl -sS -c jar -b jar "$ORIGIN/"`). HTML contains `id="generateForm"`, `id="gallery"`, and `Gerador de Avatares RoboHash`.
- **Session cookie.** Inspect response headers from `GET /`. A `Set-Cookie` for `avatar_session_id` is present (`HttpOnly`, `Path=/`, `SameSite=Lax`).
- **Static assets.** Request `/static/app.js` and `/static/style.css`. Both return 200.
- **Proof.** Artifacts under `evidence/home-health-<run-id>/`: `healthz.txt`, `index.html`, `index.headers`, `static-app.js`, `static-style.css`, `transcript.txt`, `PROOF.md`.

## Gotchas

- An ambient server on port 8080 is not this instance. Doctor must match the pid in `state.env`.
- Default copy is pt-BR. English strings appear only after the `lang=en` cookie (see language-switch).
- Empty gallery copy is `Nenhum avatar ainda…`; a previous unclean session in the same cookie jar can hide it.
