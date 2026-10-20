# Proof: generate-avatars

- feature: generate-avatars
- entry: POST /generate (same JSON body as static/app.js), gallery reload, download routes, clear-all
- origin: http://127.0.0.1:18100
- result: pass
- payload: count=2 seed=verify-skill-seed set=set2 background=bg1 size=100 ttl_days=1
- ids: b3eea1af-1871-4a9a-8883-e8785baec181, b5a8e210-935d-4212-8f5b-c8c179003d32
- observed: invalid count → 4xx; valid generate → count 2; gallery cards; PNG magic on /avatar; attachment on /download; ZIP on /download-all; delete + clear-all empty the gallery.
- side effects: ≥2 PNG files under DATA_DIR during the run; gallery empty HTML after clear-all.
