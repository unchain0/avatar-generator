# Session isolation

Sessions keep each browser's gallery private via the `avatar_session_id`
cookie. A second browser does not see or delete the first browser's cards.

## Sub-features

- `session-assign` creates a session cookie on first `GET /`.
- `session-private-gallery` lists only avatars owned by that cookie.
- `session-delete-denied` returns 404 when another session deletes an id.
- `session-clear-own` clears only the calling session.

## How to get to it (user POV)

- Open the site in two fresh browser profiles (or one normal and one private window).
- Generate avatars in profile A; reload profile B.
- Try to delete A's card from B (only possible if B knows the id).

## Driving it with curl-helpers

Preconditions:

- Doctor is green for `$SCRATCH`.
- Two separate cookie jars (do not share `cookies.txt`).

- **Two jars.** Run `scripts/drive-session.sh "$SCRATCH"`. It bootstraps jar A and jar B with `GET /`.
- **Generate in A.** `POST /generate` with jar A, count 1. Note `avatars[0].id`.
- **B gallery.** `GET /` with jar B. HTML must not contain `data-id="<id>"`.
- **B delete denied.** `DELETE /avatar/<id>` with jar B returns 404.
- **A still owns it.** `GET /` with jar A still lists the card.
- **Clear A.** `POST /clear-all` with jar A returns `success: true`.
- **Proof.** `evidence/session-isolation-<run-id>/` with both index HTML files, delete response, `PROOF.md`.

## Gotchas

- `GET /avatar/:id` is id-based and returns the PNG without a session cookie. That is the public proxy path; gallery listing and delete remain session-scoped. Do not treat a bare image fetch as a session leak.
- Reusing one cookie jar across "two users" invalidates the proof.
- `clear-all` only removes the calling session's rows; it is not a global wipe.
