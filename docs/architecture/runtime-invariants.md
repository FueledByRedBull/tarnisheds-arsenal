# Runtime invariants

Status: accepted. These rules describe contracts that tests and future refactors must preserve.

## Cache identity and versioning

- Analysis cache keys include the profile ID, every behavior-affecting input, and the dataset schema, dataset version, and model version.
- A solved-build key uses the stable result fingerprint: weapon ID/name, affinity, AoW identity, upgrade, somber flag, and all five combat stats.
- Caches are bounded. Eviction may reduce performance but must never alter results.
- An aborted subscriber cannot populate a cache entry. When the last subscriber leaves, the pending entry is evicted immediately; backend work is also cancelled when that command exposes cancellation.

## Job lifecycle

- Search, Paths, and Affinity Watch each own a monotonically increasing frontend generation, an input signature containing the profile ID, and at most one backend job ID.
- A response may update state only when generation, signature, and job ID all exactly match the active request.
- Input changes invalidate the active generation before dependent state is changed.
- Cancellation is cooperative and fail-closed. A cancelled job cannot replace current rows or populate retained analysis caches.
- Broad running work targets cancellation within 250 ms on the reference machine. Cancelled multi-lane jobs publish no partial success payload.
- Polling has one in-flight request at a time and backs off while progress is unchanged.
- Numeric input edits do not launch exact optimizer preparation; the command rail shows a constant-time scope summary and exact candidate preparation begins only when Search is pressed. Search-space estimation has no job lifecycle to preserve: it is a cancellable core API with no command or frontend caller, so nothing can publish an estimate into frontend state. Reintroducing a user-facing estimate means giving it a generation, signature, and job ID like any other async request.
- A profile switch invalidates every job generation before changing inputs, requests cancellation for all active backend jobs, clears profile-bound results, and cannot accept a completion from the previous profile.

## Result identity

- Selection follows the result fingerprint, not row index or visual rank.
- New rankings retain selection only when the exact fingerprint still exists.
- Results retained while inputs change are explicitly stale. Stale rows cannot launch Compare, Paths, or Affinity Watch.
- Saved solved rows are trusted only when schema, dataset, and model versions match the active catalog; otherwise only normalized inputs are loaded.
- Presets have an explicit profile ID. Legacy presets migrate to Vanilla, and presets from another profile cannot be loaded or silently converted.
- CSV exports include profile/data/model provenance. Unsupported values are blank,
  never numeric zero; unified upgrade profiles do not claim Standard/Somber
  identity. Export reruns and caches use the complete normalized request including
  profile and requested row count.

## Data snapshots

- Each runtime profile is one immutable manifest snapshot. Every required runtime file must be listed exactly once with its byte length and SHA-256 hash.
- External loading is all-or-nothing. Missing, modified, duplicate, unlisted, mixed-version, or path-traversing entries fail startup; files never fall back individually to embedded data.
- The embedded snapshot is validated against the same manifest contract before parsing.
- The runtime profile registry contains an independently validated snapshot for every shipped profile. Commands select one explicit profile and never combine rows, jobs, lanes, caches, or metadata across profiles.
- Every manifest binds its profile ID, display name, capability flags, mechanics rules, source hashes, and whether each source is bundled. Upgrade caps, upgrade-path shape, Scadutree availability, extended grades, status-scaling semantics, and attack-element fallback behavior are profile data and are enforced in both UI and core. Unsupported model areas use explicit capabilities and schema-only tables, never data copied from another profile.
- Convergence ammunition rows remain in the immutable source snapshot but are not
  exposed to catalog/search/export until an arrow/bolt projectile model exists.
  They must never compete using weapon-only or duplicated damage components.
- A mod profile with a version-bound availability reference extracts only exact referenced configuration IDs. Offline validation compares every common modeled weapon field and status family; a missing, stale, or mismatched reference fails the release gate.
- Schema version describes storage compatibility, dataset version identifies extracted content, and model version identifies calculation semantics. They change independently.

## Change checklist

Any cache, async job, result DTO, preset, or snapshot change must update the closest invariant test and run the frontend, core, Tauri packaged-data, and release metadata gates appropriate to that boundary.
