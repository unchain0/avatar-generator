# Avatar generation without robohash.org

## Problem

`https://avatar.unchain0.com` answered `502` on every generation. The app was
healthy (`/healthz` 200), the container was running and the last deploy passed
its health check. The failure sat in the dependency: the app fetched images from
the public `robohash.org`, which returned `525` (Cloudflare could not complete
TLS with its origin) or timed out. Logs showed one `WARN ... failed: upstream
returned status 525` per request, then `502 Bad Gateway`.

The health check only proves the app process is alive, so a deploy would have
looked healthy with generation broken.

## Goal

Generation and previews return real images with no call to a third party.

## Options

**A. Official RoboHash container as an internal sidecar (chosen).**
`ghcr.io/e1ven/robohash` pinned by digest on the private `avatar-net`, reached as
`http://robohash:8080`. Same artwork and same algorithm; the app keeps its
`ROBOHASH_BASE_URL` seam and needs no code change. Measured on the box: about
22 MB RAM idle, 0.18-0.35 s per 640 px image.

**B. Port the compositor to Rust and bundle the artwork.**
Removes the extra runtime, but adds the `image` crate, about 40 MB of
CC-BY artwork in Git with its attribution obligations, and a hand-ported
composition algorithm. Rejected for now as larger risk for no user-visible gain.

**C. Retry with a static fallback.**
Covers the symptom with a placeholder image. Rejected: still dependent, and the
product loses its point.

## Decision

A, plus two follow-ups in the app:

- `/preview` proxies the UI thumbnails and the live stage through the same
  upstream, so the browser no longer needs `robohash.org` and the CSP drops it.
- The preview size range is validated server-side (`16..=1024`) instead of the
  stored-avatar size list, so thumbnails at 120 px work.

## Open risk

The sidecar is on the same host, so a host outage still stops generation; the
app's own health check does not include upstream reachability. Both are noted for
a later change, not required to restore the service.
