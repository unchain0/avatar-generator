# Language switch

The topbar PT/EN toggle switches all UI strings between Portuguese (default)
and English by setting a `lang` cookie and reloading.

## Sub-features

- `lang-default-pt` serves Portuguese copy with no `lang` cookie.
- `lang-en` serves English copy when `lang=en`.
- `lang-pt-restore` returns to Portuguese with `lang=pt-br`.

## How to get to it (user POV)

- Open `/` (default pt-BR).
- Click **EN** in the topbar (`button[data-lang="en"]`).
- Click **PT** (`button[data-lang="pt-br"]`).

## Driving it with curl-helpers

Preconditions:

- Doctor is green for `$SCRATCH`.

- **Default.** Run `scripts/drive-language.sh "$SCRATCH"`. `GET /` contains `Gerador de Avatares RoboHash` and the button label `Gerar`.
- **English.** `GET /` with request cookie `lang=en`. HTML contains `RoboHash Avatar Generator`, button `Generate`, and `html lang="en"`.
- **Restore.** `GET /` with `lang=pt-br` restores the Portuguese brand string.
- **Proof.** `evidence/language-switch-<run-id>/` with `index-pt.html`, `index-en.html`, `index-pt2.html`, `transcript.txt`, `PROOF.md`.

## Gotchas

- The JS sets `document.cookie = "lang=…; Path=/; Max-Age=31536000"` then reloads. Curl must send the cookie on the request; there is no separate language API.
- Unknown `lang` values fall back to pt-BR (`handlers.rs`).
- Doctor asserts the pt-BR brand string; run language drive after doctor, not instead of it.
