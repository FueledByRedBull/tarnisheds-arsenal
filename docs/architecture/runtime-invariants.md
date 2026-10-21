# Runtime invariants

Use this page as the contract for cache identity, asynchronous jobs, result
selection, snapshot loading, and changes that cross those boundaries.

Status: accepted. These rules describe contracts that tests and future refactors must preserve.

**Navigation:** [Home](../../README.md) · [Optimizer overview](../design/optimizer-overview.md) ·
[Optimizer math](../design/optimizer-math.md) · [Performance](../performance.md) ·
[Extracting game data](../../tools/phase1/README.md)

**Find a contract:** [Cache identity](#cache-identity-and-versioning) ·
[Job lifecycle](#job-lifecycle) · [Result identity](#result-identity) ·
[Data snapshots](#data-snapshots) · [Change checklist](#change-checklist)

## Cache identity and versioning

- Analysis cache keys include the profile ID, every behavior-affecting input, and the dataset schema, dataset version, and model version.
- The active snapshot and scoring identities are recorded in the [model reference](../model-reference.md).
  Storage, dataset, and calculation identities are independent; incompatible snapshots
  or persisted results fail closed rather than being relabeled as a migration.
- A solved-build key uses the stable result fingerprint: weapon ID/name, affinity,
  AoW name, upgrade, and all five combat stats. Reinforcement type belongs to the
  profile-bound weapon identity rather than a separate fingerprint field.
- Caches are bounded. Eviction may reduce performance but must never alter results.
- An aborted subscriber cannot populate a cache entry. When the last subscriber leaves, the pending entry is evicted immediately; backend work is also cancelled when that command exposes cancellation.

## Frontend and native types

- `npm run test:contracts` in `apps/desktop` compares public DTO declarations in
  `src-tauri/src/dto.rs` with `src/lib/types.ts`, including nested fields, arrays,
  nullability and native enum variants. API request construction uses those named
  interfaces. New Rust/serde syntax must be explicitly supported by the checker.
- Deliberate differences are narrowly checked: the frontend sends explicit
  upgrade-policy values and omits older upgrade inputs; older persisted builds
  may lack three optional display fields. Other missing/optional fields fail.
- The same check verifies that every `#[tauri::command]` is registered once in
  `lib.rs`, listed in `build.rs`, allowed by the default capability, and called
  from `api.ts` with matching argument names. It is not a serializer or
  numeric-range check. Rust validators and serialization tests remain required.
  Native String fields can have narrower frontend literal types; permitted values
  still belong to native validation.
- React Hooks rules, including the React Compiler checks, are lint errors. Async effects
  must retain generation/cancellation guards when dependencies change. Comparison
  persistence and preset imports share the solved-build shape validator in
  `src/lib/stored-build.ts`, with explicit profile or archive upgrade bounds.
  Structural validity does not establish that stored calculations are current.

## Job lifecycle

- Search, Paths, and Affinity Watch each own a monotonically increasing frontend generation, an input signature containing the profile ID, and at most one backend job ID.
- Paths and Affinity Watch use separate native queues. Cancellation or status
  uncertainty in one queue retains that queue's native ownership without blocking
  the other queue.
- A response may update state only when generation, signature, and job ID all exactly match the active request.
- Cancellation replies and errors obey the same generation/job ownership rule;
  a late acknowledgement cannot clear or report an error on a replacement search.
- Input changes invalidate the active generation before dependent state is changed.
- Hiding a workspace (switching tabs) does not cancel its Paths, Affinity Watch or trade-off work; Stop, input, selection and profile changes do.
- Cancellation is cooperative and fail-closed. A cancelled job cannot replace current rows or populate retained analysis caches.
- Broad running work targets cancellation within 250 ms on the reference machine. Cancelled multi-lane jobs publish no partial success payload.
- Polling has one in-flight request at a time and backs off while progress is unchanged.
- A finished native job keeps its terminal status until the next job in that queue
  starts, so a lost status reply can be read again. A page load, including a
  reload, cancels every running native job, because the new page cannot own them.
- Rankings, custom comparisons, and CSV export share one search queue. The workflow owns polling through native completion, including cancellation; superseded queued requests never start. Cancel aborts whichever search owns the current generation; an aborted queued request rejects at once and never starts. Rankings UI components do not run a second poller.
- Cancellation or status communication errors may end the caller's request, but do not establish that its native worker is idle. Retain uncertain job ownership, bound recovery attempts, and reconcile that worker before starting replacement work.
- Direct loadout and upgrade-series calculations run off the native main thread, with bounded concurrency and cooperative cancellation. Shared cache work is cancelled only when its last subscriber leaves or its profile/data identity is invalidated.
- A Compare operation requires fresh Rankings results. Failed required solves
  abort their remaining siblings and report the original error. Verified lanes
  and the target publish before optional upgrade charts; a chart failure leaves
  those lanes usable and reports that lane's chart error. Aborting one subscriber
  does not cancel shared cached work needed by another subscriber.
- Compare evaluates all stored pins (up to eight), excluding the selected baseline.
  A ninth pin is refused with a notice; the oldest pin is never dropped.
  Exact requests retain their requested upgrade or reject it. Pinned comparisons
  explicitly request and label +0 for a weapon whose verified profile has no
  reinforcement levels; other exact levels cannot silently become a lower level.
- Native job queues resolve only successful terminal payloads; cancellation and
  errors reject. This type-level guarantee does not release uncertain native
  ownership or replace generation/signature checks at publication.
- Numeric input edits do not launch exact optimizer preparation; the query strip's
  tokens and stat ribbon show a constant-time scope summary and exact preparation
  begins only on Search.
  `PreparedSearchPlan::estimate` reports the prepared plan's logical candidate
  count; there is no standalone estimate API or frontend estimate job. A future
  asynchronous estimate needs a generation, signature, and job ID.
- A profile switch invalidates every job generation before changing inputs, requests cancellation for all active backend jobs, clears profile-bound results, and cannot accept a completion from the previous profile.
- Each profile keeps its last query (request and stat-lock mode) in memory and gets it back when the player returns to that profile. A query is never carried into another profile, and results are not restored: the returning query is searched again on demand.
- Each profile's query is also saved on the device under its own profile key, written only while that profile is loaded, and restored at startup or on return after a restart. It is validated like an imported preset request; an unreadable query or one naming another profile is removed with a notice, never repaired or carried across profiles.
- Release builds swallow WebView2's reload and print keys (F5, Ctrl+R, Ctrl+Shift+R,
  Ctrl+F5 and Ctrl+P) so they cannot discard the session; development builds keep
  reload.
- CSV export owns a cancellable search until the backend reports completion, including after cancellation. Normal searches and comparison searches wait for that slot. Input/profile changes and leaving Rankings cancel export; late results cannot download or populate its cache.

## Result identity

- Selection follows the result fingerprint, not row index or visual rank.
- New rankings retain selection only when the exact fingerprint still exists.
- Results retained while inputs change are explicitly stale. Stale rows cannot launch
  Compare, Paths, or Affinity Watch; Compare clears its visible series and target until
  fresh Rankings results exist.
- A preset's starting class must match the active catalog before activation can
  mutate workspace or storage. An unambiguous case-insensitive match is
  canonicalized; unknown classes are rejected rather than replaced.
- Saved solved rows are archives. Even when their schema, dataset and model match,
  selected rows and comparison snapshots are recalculated through native fixed
  loadout solving with their exact equipment, upgrade and stats before publication.
  The selected build and comparison target must fit the saved inputs' level; a
  pinned comparison uses the level its own stats require.
  Rows saved on other game data are recalculated the same way on the current data,
  so their saved numbers are never shown; when recalculation fails, only the inputs
  load. Failed or superseded verification preserves the original saved record and
  cannot publish late results.
- Weapon metadata belongs to its exact profile/weapon/affinity key. Loading,
  failure and retry never expose a previous key's profile as current. Effective
  Strength includes forced two-handing and weapon-specific bonus suppression.
  Only a manual weapon selection requests its native default skill; restored
  and starter inputs retain their explicit Automatic policy as metadata arrives.
- A restored comparison target remains available in Compare and Paths until the
  user changes its comparison or build context. Opening a workspace does not
  discard that target or silently replace it with unrelated comparison controls.
  Compare re-resolving the same target (same content) is not a change: it does
  not invalidate or cancel a Paths run.
- Manual comparison controls preserve stored pins. Adding, removing or clearing
  pins invalidates derived targets and Paths, including work already in flight.
  Loading a saved build without pins keeps the current pins; a build that carries
  pins replaces them and says so. Stored pins saved on other game data are
  recalculated on the current data before they are shown, and storage is replaced
  only with the recalculated pins; a pin that cannot be recalculated is dropped
  with a notice. A catalog or profile change, or a change to the pins, during that
  recalculation discards its result.
- Affinity Watch points contain only level and nullable metric. Endpoint summaries
  use the actual boundary points; missing endpoints remain unavailable and cannot
  inherit an interior build. Only an available final point supplies final stats.
- Presets have an explicit profile ID. Legacy presets migrate to Vanilla, and presets from another profile cannot be loaded, silently converted, or overwritten by a save from another profile.
- Presets save effective stat locks; disabled locks are stored as null. Loadout solving and migration preserve requested locks. Comparison callers explicitly clear locks when reoptimizing a rival's stats.
- Loading a readable preset does not depend on persisting comparison pins. Optional comparison-storage failures leave session state usable and show a warning; essential preset writes still report failure. Obsolete migration requests cancel their native calculations as well as rejecting stale results.
- CSV exports include profile/data/model provenance. Unsupported values are blank,
  never numeric zero; unified upgrade profiles do not claim Standard/Somber
  identity. Export reruns and caches use the complete normalized request including
  profile and requested row count.
- Skill availability is per result as well as per profile: a missing calculated
  route is unavailable across displays and exports. A route with calculated zero
  damage remains a supported zero, not an absent value.
- Missing or malformed saved-build indexes are distinct from an empty library.
  Essential writes refuse to replace an unreadable index. Recovery previews bind
  the exact source records/index, reject changed previews, preserve the original
  index before replacement, and leave unreadable records untouched. Storage read
  failures must not throw through component rendering.
- Bulk build backups use `tarnisheds-arsenal.build-backup` version 1, containing
  existing versioned presets, bounded to 500 builds/10 MiB. Validate the entire
  backup before writing, each build against the same size limit as a save; restore with new IDs and commit the index last. Failed
  cleanup leaves new records recoverable as orphans.
- Preset, backup and recovery validation rejects null comparison pins and requires
  request enum values to be strings before they can reach the UI or native commands.
- Reproduction reports project known request/result fields, including nested
  structures, and omit raw storage, logs, and private manifest source paths.
  Report text redaction, listed in the [security policy](../../SECURITY.md#reproduction-reports),
  is an additional precaution; users preview before sharing.
  Captures describe current inputs and displayed results, never imply a failed
  request was captured or a saved result independently recalculated.

## Data snapshots

- Each runtime profile is one immutable manifest snapshot. Every required runtime file must be listed exactly once with its byte length and SHA-256 hash.
- External loading is all-or-nothing. Missing, modified, duplicate, unlisted, mixed-version, or path-traversing entries fail startup; files never fall back individually to embedded data.
- The embedded snapshot is validated against the same manifest contract before parsing.
- CSV headers are resolved once per table. Required numeric, handling and
  compatibility columns must exist even in empty tables; optional descriptions
  cannot supply defaults for missing calculation inputs.
- The runtime profile registry contains an independently validated snapshot for every shipped profile. Commands select one explicit profile and never combine rows, jobs, lanes, caches, or metadata across profiles.
- Every manifest binds its profile ID, display name, capability flags, mechanics rules, source hashes, and whether each source is bundled. Upgrade caps, upgrade-path shape, Scadutree availability, extended grades, status-scaling semantics, and attack-element fallback behavior are profile data and are enforced in both UI and core. Unsupported model areas use explicit capabilities and schema-only tables, never data copied from another profile.
- Native skill identity remains anchored to the source `native_skill_id` when a
  localized label or motion rows are unavailable. The runtime may expose a null
  label and no damage rows, but it must not erase the ID or invent damage data.
- Class budgets come only from the selected profile: Vanilla's built-in classes or
  a profile's manifest-listed `classes.csv`. A request naming another profile's
  class is rejected, and a non-Vanilla profile that declares class budgets without
  a class catalog fails at the manifest boundary. Such a profile also offers
  Custom stats, which keeps its exact-stat budget and cannot start Compare, Paths,
  Affinity Watch, or AR / Bleed tradeoffs.
- Convergence ammunition rows remain in the immutable source snapshot but are not
  exposed to catalog/search/export until an arrow/bolt projectile model exists. They
  must never compete using weapon-only or duplicated damage components; current profile
  coverage is listed in the [model reference](../model-reference.md).
- A mod profile with a version-bound availability reference extracts only exact referenced configuration IDs. Offline validation compares every common modeled weapon field and status family; a missing, stale, or mismatched reference fails the release gate.
- Required mounting/affinity permissions, attack-provenance fields
  (`is_bullet_attack`, `is_throw_attack`), and weapon `critical_damage_percent` are
  validated before use. Incompatible schema, model or extractor versions fail at the
  manifest boundary. A profile with weapon passives needs a passive row for every
  weapon, and status modeling requires weapon passives, so missing passive data fails
  loading instead of reading as zero buildup; no per-pair compatibility CSV or runtime pair set is retained. The versioned file
  contract is maintained in [Extracting game data](../../tools/phase1/README.md#snapshot-contract).
- Ash compatibility is exactly mounting permission AND affinity membership AND weapon-type intersection. Native-only skills are handled separately and cannot bypass infusion restrictions.
- A fixed-loadout Paths evaluation pins weapon, affinity, Ash, and upgrade, then clears
  discovery filters before preparing its evaluator. Discovery constraints therefore
  cannot leak into a selected path.
- Reusable loadout and upgrade evaluators retain equipment across validated stat
  budget changes and check eligibility against each evaluation's budget. Cancellation
  is honored even when a locked evaluation has no eligible weapons.
- Schema version describes storage compatibility, dataset version identifies extracted content, and model version identifies calculation semantics. They change independently.
- A healthy profile shows nothing in the header corner; “Snapshot loaded” is
  announced to screen readers only, while loading and the experimental model stay
  visible. Either way the state reports a manifest-bound snapshot that passed
  loading checks and declares capabilities; it does not independently verify every
  formula.

## Change checklist

Any cache, async job, result DTO, preset, or snapshot change must update the closest invariant test and run the frontend, core, Tauri packaged-data, and release metadata gates appropriate to that boundary.

## Fixed-loadout tradeoffs

AR/bleed tradeoffs use the shared cancellable analysis worker. Changing the
request/profile/selected loadout cancels the subscriber and prevents a late result
from appearing. Leaving Compare does not: an in-flight frontier keeps running and a
finished one, with its selected point, is still shown on return while the context
is unchanged. The returned frontier belongs to one
fixed equipment, upgrade, stat-budget, constraint, and handling context.
Presentation controls read the completed result without more optimizer work.
Inspecting an option preserves its exact allocation; it does not feed the option
through the pinned-loadout reoptimizer. Applying it pins the equipment, upgrade,
and all five combat stats through lock/search actions. Returned and saved/reloaded
builds must preserve that setup, allocation, AR, and bleed; matching a weapon name
alone is insufficient. No new persisted format is introduced.
