# Proof: session-isolation

- feature: session-isolation
- entry: two cookie jars, POST /generate, DELETE /avatar/:id, POST /clear-all
- origin: http://127.0.0.1:18100
- result: pass
- observed: avatar created in session A is absent from session B gallery; B gets 404 on
  DELETE of A's id; A still lists it until clear-all. Image proxy GET /avatar/:id is
  id-based (200 without session cookie) — documented, not a session leak of the gallery.
- side effects: clear-all on A removes the batch.
