# Evidence

Proof artifacts from verification runs land here as

```text
<feature>-<run-id>/
```

Cleanup removes only `/tmp/avatar-generator-verify/<run-id>/` scratch state.
It never deletes this directory. Keep `PROOF.md` and transcripts; large PNGs
may be trimmed later if the tree grows, but a fresh run must leave at least
one complete feature directory behind.
