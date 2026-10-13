# Generate avatars

Generate lets a user request a batch of RoboHash avatars with count, optional
seed, set, background, size, and TTL, then see them in the session gallery.

## Sub-features

- `generate-validate` rejects out-of-range count and bad options.
- `generate-batch` creates 1..50 avatars for the current session.
- `generate-gallery` shows a card per avatar after reload.
- `generate-proxy` serves each image from `/avatar/:id` on this origin.
- `generate-clear` removes the batch with clear-all when the user is done.

## How to get to it (user POV)

- Fill `#count`, optional `#seed`, `#set`, `#background`, `#size`, `#ttl_days` on `/` and press **Gerar** (`#generateBtn`).
- The page `POST`s JSON to `/generate` (see `static/app.js`) and reloads.
- Operators may POST the same JSON with curl and the session cookie.

## Driving it with curl-helpers

Preconditions:

- Doctor is green for `$SCRATCH`.
- Upstream RoboHash (or `ROBOHASH_BASE_URL`) is reachable from the server process.
- Cookie jar is empty or the session is under the 100-avatar cap.

- **Invalid count.** POST `{"count":0,...,"size":200,"ttl_days":1}`. Run `scripts/drive-generate.sh "$SCRATCH"` (first step). Expect HTTP 4xx and an `error` field.
- **Valid batch.** POST `{"count":2,"seed":"verify-skill-seed","set":"set2","background":"bg1","size":100,"ttl_days":1}` to `/generate` with the session cookie. Status 200, JSON `count` is 2, `avatars[].id` present.
- **Gallery reload.** `GET /` with the same cookie. HTML contains `data-id="<id>"` and `src="/avatar/<id>"` for each id.
- **Proxied PNG.** `GET /avatar/<id>` returns `content-type: image/png` and PNG magic bytes (`89 50 4E 47`).
- **Side effect.** `$DATA_DIR` holds at least two `.png` files while the batch lives.
- **Clear.** `POST /clear-all` then `GET /` shows the empty-gallery copy and no `avatar-card`.
- **Proof.** `evidence/generate-avatars-<run-id>/` with `generate.json`, PNGs, `index-after.html`, `index-cleared.html`, `clear-all.json`, `transcript.txt`, `PROOF.md`.

## Gotchas

- Count must be 1..50; size must be one of 50,100,200,300,400,500,640,800,1024.
- Set is `set1`..`set5`; background is `bg1`, `bg2`, or `none`.
- Rate limit is per IP (~30/min). Burst drives can 429; wait or use a fresh instance.
- Generate needs network to the upstream unless `ROBOHASH_BASE_URL` points at a stand-in.
- The UI disables `#generateBtn` until reload; curl does not need that.
