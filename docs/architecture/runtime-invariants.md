# Runtime invariants

Status: accepted. These rules describe contracts that tests and future refactors must preserve.

## Cache identity and versioning

- Analysis cache keys include every behavior-affecting input plus the dataset schema, dataset version, and model version.
- A solved-build key uses the stable result fingerprint: weapon ID/name, affinity, AoW identity, upgrade, somber flag, and all five combat stats.
- Caches are bounded. Eviction may reduce performance but must never alter results.
- An aborted subscriber cannot populate a cache entry. When the last subscriber leaves, the pending entry is evicted immediately; backend work is also cancelled when that command exposes cancellation.

## Job lifecycle

- Search, Paths, and Affinity Watch each own a monotonically increasing frontend generation, an input signature, and at most one backend job ID.
- A response may update state only when generation, signature, and job ID all exactly match the active request.
- Input changes invalidate the active generation before dependent state is changed.
- Cancellation is cooperative and fail-closed. A cancelled job cannot replace current rows or populate retained analysis caches.
- Broad running work targets cancellation within 250 ms on the reference machine. Cancelled multi-lane jobs publish no partial success payload.
- Polling has one in-flight request at a time and backs off while progress is unchanged.

## Result identity

- Selection follows the result fingerprint, not row index or visual rank.
- New rankings retain selection only when the exact fingerprint still exists.
- Results retained while inputs change are explicitly stale. Stale rows cannot launch Compare, Paths, or Affinity Watch.
- Saved solved rows are trusted only when schema, dataset, and model versions match the active catalog; otherwise only normalized inputs are loaded.

## Data snapshot

- Runtime data is one immutable manifest snapshot. Every required runtime file must be listed exactly once with its byte length and SHA-256 hash.
- External loading is all-or-nothing. Missing, modified, duplicate, unlisted, mixed-version, or path-traversing entries fail startup; files never fall back individually to embedded data.
- The embedded snapshot is validated against the same manifest contract before parsing.
- Schema version describes storage compatibility, dataset version identifies extracted content, and model version identifies calculation semantics. They change independently.

## Change checklist

Any cache, async job, result DTO, preset, or snapshot change must update the closest invariant test and run the frontend, core, Tauri packaged-data, and release metadata gates appropriate to that boundary.
