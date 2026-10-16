# Avatar Generator verification map

This directory is the maintained source for verifying the user-facing behavior
of Avatar Generator. Read the index before driving the app, then use the
matching feature file as the recipe.

## Baseline preconditions

- Launch with `.cursor/skills/verify-avatar-generator/scripts/launch.sh`.
- Use the `SCRATCH=` path it prints for doctor, drive, and cleanup.
- Each launch owns a free loopback port (18100–18200) and
  `DATA_DIR=/tmp/avatar-generator-verify/<run-id>/data`.
- Run `scripts/doctor.sh "$SCRATCH"` and require `doctor: ok`.
- Never drive an instance that was not started by this verification run.
- Never point a drive at production (`avatar.unchain0.com`) or at the ambient
  process on port 8080.

## Driving conventions

- Start every recipe from a doctor-ok instance unless its preconditions say
  otherwise.
- Prefer the HTTP paths and JSON fields the UI already uses
  (`static/app.js`, form field ids in `templates/index.html`) over ad-hoc
  internal calls.
- Keep a cookie jar so `avatar_session_id` matches a browser session.
- Treat every command in the feature files as literal.
- Restore or clear session state after a mutation when the next recipe needs
  an empty gallery. Do not remove proof artifacts during cleanup.

## Proof and skip reporting

- Capture the request and the resulting state (status, body, gallery HTML,
  file side effects under `DATA_DIR`), not only the final screen.
- Mutation proof includes a second read (gallery HTML or GET of the resource).
- Record the feature ID and entry point in `PROOF.md` next to the artifacts.
- Report an unreachable path with the attempted command and the unmet
  precondition. Do not report a skipped entry point as verified through a
  different path.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the
user-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features` lists short IDs with one line for each behavior.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with curl-helpers` starts with `Preconditions:` and uses labeled
   bullets that pair each user action with an exact command and observable
   result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation details out of the map. Name only user paths, stable
handles, required state, commands, and observable proof.

## Features

- [Home and health](./home-health.md) covers `/healthz`, the empty gallery, session cookie, and static assets.
- [Generate avatars](./generate-avatars.md) covers the generate form path, validation, gallery cards, PNG proxy, downloads, and clear-all.
- [Gallery and downloads](./gallery-download.md) covers per-avatar download, download-all ZIP, and delete.
- [Session isolation](./session-isolation.md) covers cookie sessions and cross-session delete denial.
- [Language switch](./language-switch.md) covers the PT/EN topbar cookie toggle.
