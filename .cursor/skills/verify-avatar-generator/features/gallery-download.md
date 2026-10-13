# Gallery and downloads

Gallery actions let a user download one avatar, download the whole session as a
ZIP, or delete a single card without leaving the page shell.

## Sub-features

- `download-one` returns a PNG attachment named `<id>.png`.
- `download-all` returns a ZIP of every avatar in the session.
- `delete-one` removes a session-owned card and returns `{"success":true}`.
- `download-empty` returns 404 when the session has no avatars.

## How to get to it (user POV)

- On a card, choose **Baixar** (`a[href="/download/<id>"]`).
- Above the gallery, choose **Baixar tudo (ZIP)** (`#downloadAllBtn`).
- On a card, choose **Excluir** (`button[data-delete="<id>"]`), confirm the dialog.
- With an empty gallery, download-all stays disabled in the UI.

## Driving it with curl-helpers

Preconditions:

- Doctor is green.
- The session already has at least two avatars (run generate first, or use `drive-generate.sh` which covers these paths).

- **Single download.** `GET /download/<id>` with the session cookie. Status 200, `content-disposition` contains `filename="<id>.png"`, body is PNG.
- **Download all.** `GET /download-all` with the session cookie. Status 200, body starts with ZIP magic `PK`.
- **Delete one.** `DELETE /avatar/<id>` with the session cookie. Status 200 and `{"success":true}`. A later `GET /` no longer lists that `data-id`.
- **Empty zip.** After `POST /clear-all`, `GET /download-all` returns 404 with body `no avatars in this session`.
- **Proof.** Covered inside `evidence/generate-avatars-<run-id>/` (`download-*.png`, `download-all.zip`, `delete-*.json`) or a dedicated drive that copies those artifacts.

## Gotchas

- Delete is session-scoped; another cookie gets 404 (see session-isolation).
- `#downloadAllBtn` is disabled in HTML when the server rendered zero avatars; curl can still hit the route and observe 404.
- ZIP uses stored compression; assert magic bytes, not a particular compressor version.
