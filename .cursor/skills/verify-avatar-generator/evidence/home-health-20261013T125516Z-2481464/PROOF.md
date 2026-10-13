# Proof: home-health

- feature: home-health
- entry: GET /healthz, GET /, GET /static/*
- origin: http://127.0.0.1:18100
- result: pass
- observed: healthz body `ok`; index HTML has generateForm + gallery + pt-BR brand;
  Set-Cookie avatar_session_id present; static assets 200.
- side effects: session cookie jar at cookies (not committed); no avatars written.
