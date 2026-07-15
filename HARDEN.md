I’ll use these defaults:

- Repeatable AoWs: one normalized loop cycle followed by the selected legal finisher.
- Invalid runtime snapshot: fail closed with an actionable error; never mix runtime and embedded files.
- Physical attributes: AoW hits only.
- Dependencies: approved, including Rust hashing/manifest crates and Vitest for frontend race tests.

## Completion status — v0.7.0

All twelve implementation sections below are complete. The acceptance evidence is
recorded in [`docs/validation/v0.7.0-correction-report.md`](docs/validation/v0.7.0-correction-report.md).
The verified gates are 75 core tests, 13 real Tauri/backend tests, 13 frontend
tests, strict Clippy, Rust formatting, Ruff, Pyright, TypeScript production build,
zero-warning Phase 4 validation, exact snapshot validation, and independent
non-browser calculator checks.

New findings resolved while implementing the plan:

- `SwordArtsParam` does not directly identify every executable attack/effect path;
  numeric Behavior/Atk/Bullet/SpEffect traversal plus explicit persistent roots is
  required.
- Native-only skill IDs can legitimately be absent from the generic AoW workbook;
  coverage now distinguishes that case from an unresolved player weapon.
- Fifteen conditional replacement/chained effects cannot safely be interpreted as
  immediate buildup. They are explicit unsupported warnings with source IDs.
- The PARAM reader now rejects definition version, endian, row-size, pointer, and
  duplicate-ID mismatches instead of accepting a partial parse.
- The retired `aow_buffs.csv` is not loaded by the app and is retained only as a
  historical diagnostic artifact pending explicit deletion approval.
- External calculators are useful cross-checks but not canonical inputs: the T.
  Clark bundle is App 1.14, while the local snapshot is App 1.16.1.
- The frontend audit found Vite 8.0.12 affected by two Windows dev-server
  advisories. The lockfile now resolves Vite 8.1.4 and `npm audit` reports zero
  vulnerabilities, including zero production vulnerabilities.

## Implementation plan

### 1. Harden weapon extraction

- Establish an explicit allowlist of player-usable weapon families from the workbook.
- Reject `[NPC]` weapon entries from the public dataset.
- Reject nonstandard affinity configurations when `disableGemAttr=1`. A standard
  fixed-skill weapon may legitimately set this flag; the invalid case is a
  nonstandard affinity row whose own regulation row disables gem attributes.
- Validate that affinity compatibility and exported weapon configurations agree.
- Keep excluded rows available only in extraction diagnostics, not optimizer data.
- Add regression checks for Great Club, Treespear, Troll’s Hammer, and other known invalid combinations.

Acceptance criteria:

- No NPC weapons are rankable.
- No disabled affinity configuration is rankable.
- Exported player weapon coverage matches the expected source set exactly.

### 2. Correct fixed native-skill resolution

Use a deterministic resolution order:

1. Exact weapon-specific native-skill rows.
2. Generic AoW rows with the same skill ID.
3. Filter generic rows by weapon category, behavior variant, and applicable restrictions.
4. If multiple candidates remain, classify the skill as unresolved instead of combining them.

Add fixtures covering the 28 known affected weapons, including Carian Knight’s Sword, Great Club, Troll’s Hammer, Meteoric Ore Blade, and Inseparable Sword.

Acceptance criteria:

- Every supported fixed skill resolves to one unambiguous damage definition.
- Generic fallback never combines unrelated variants.
- Remaining unsupported skills are visible and excluded from ranking.

### 3. Replace “full sequence” with legal AoW routes

Load and model the existing `sequence_variant`, `hit_kind`, and `hit_order` data.

Add a checked-in route definition dataset containing:

- Route ID and display label.
- Ordered actions and hits.
- Startup, continuation, loop, and finisher relationships.
- Mutually exclusive branches.
- Repeatability information.
- Buff activation points.
- Stamina charged once per action rather than once per collision.
- Explicit exclusions and reasons for source rows that are not executable attacks.

The optimizer will evaluate each legal route independently and automatically select the highest-damage route. Deterministic tie-breaking will use route priority and then route ID.

Examples:

- Wild Strikes: startup → one loop cycle → R1 or R2 finisher.
- Ghostflame Call: base, R1, and R2 branches remain separate.
- Barbaric Roar: one-/two-handed and charged/uncharged branches remain separate.

Acceptance criteria:

- Mutually exclusive branches are never summed.
- Every damaging source row belongs to a route or has a documented exclusion.
- Results report the selected route label and ordered breakdown.
- Route validation must cover skills that expose only one follow-up button (for
  example, R2-only roar follow-ups); a button token is a branch only when the
  same skill actually contains both R1 and R2 alternatives.

### 4. Implement `is_add_base_atk` correctly

Split damage calculation into explicit components:

- Weapon damage multiplied by ordinary motion value.
- Fixed attack component.
- Added base-attack component controlled by `is_add_base_atk`.
- Bullet/projectile-specific behavior.
- Reinforcement and attack-element correction scaling.

The exact reinforcement curve must come from extracted source data. It will not be replaced with guessed interpolation. Rows that cannot be calculated from verified source semantics will be marked unsupported rather than returning misleading zero damage.

Acceptance criteria:

- The 151 currently zero-valued damaging rows no longer silently produce zero.
- Tests cover enabled/disabled `is_add_base_atk`, bullets, reinforcement levels, and mixed components.
- Unsupported calculations are excluded and reported.

### 5. Model physical attack attributes

Add typed support for:

- Standard
- Strike
- Slash
- Pierce
- Adaptive primary (`253`)
- Adaptive secondary (`252`)

Extract `atkAttribute` and `atkAttribute2` from weapon data and resolve adaptive attributes against the candidate weapon.

This will be exposed in AoW hit/route details. It will remain descriptive until enemy defense and negation modeling is separately implemented.

Poise/stance damage is not included.

### 6. Correct status, weapon-buff, and stamina semantics

Status motion values:

- Calculate per-hit status buildup as weapon buildup × status MV.
- Support bleed, frost, poison, rot, sleep, and madness where applicable.
- Keep buildup separate from proc damage.
- Aggregate only hits belonging to the selected route.

Weapon-buff motion values:

- Apply `weaponBuffMV` only to hits where the corresponding buff is active.
- Preserve separate ordinary damage and weapon-buff scaling.
- Model activation timing so setup hits do not receive their own future buff.

Stamina:

- Apply weapon `staminaConsumptionRate` to generic AoW rows and exceptions such as Spinning Chain.
- Treat verified unique-skill costs as already calculated where documented.
- Charge stamina once per action/input, not once per multihit collision.
- Return total route stamina without introducing a stamina-based optimization objective.

### 7. Replace name-based buff extraction

Remove text-prefix matching as the mechanism for relating buffs to attacks.

Build an ID-based effect graph through the available regulation relationships:

- EquipParamGem
- SwordArtsParam
- BehaviorParam
- AtkParam
- Bullet
- SpEffect

Classify each effect as:

- Persistent weapon buff
- Self buff
- Per-hit status/effect
- Projectile effect
- Replacement or chained effect
- Visual/non-gameplay effect
- Unsupported with an explicit reason

Names can remain display labels but cannot determine joins or aggregation.

This prevents:

- Chilling Mist and Poisonous Mist variants being added together.
- Hoarfrost Stomp spike and shatter branches being conflated.
- Ghostflame Call branches being combined.
- Poison Moth Flight’s chained effect becoming a permanent +250 buff.

### 8. Eliminate unresolved and silently discarded data

- Add explicit aliases or verified mappings for the 15 unmatched native rows involving Smithscript Dagger, Forked-Tongue Hatchet, Nightrider Flail, and Chainlink Flail.
- Make unresolved native rows a validation failure unless explicitly classified as unsupported.
- Audit the three missing SpEffect IDs affecting 27 passive rows.
- Require every referenced SpEffect to be extracted, deliberately excluded with a documented reason, or treated as a validation failure.
- Generate a machine-readable coverage report for weapons, skills, attacks, effects, and passives.

Acceptance criteria:

- Zero silently discarded source references.
- Zero unresolved rows accepted merely as warnings.
- Every exclusion has an ID, affected records, and reason.

### 9. Extend results and UX

Add an AoW route breakdown to result DTOs while retaining the existing scalar fields for compatibility.

The UI will show:

- Selected route name.
- Ordered actions and hits.
- Damage by hit.
- Resolved Standard/Strike/Slash/Pierce attribute.
- Status buildup by type.
- Buff activation and affected hits.
- Stamina by action and route total.
- Warnings for unsupported or partially modeled data.

The main ranking table will stay compact. Detailed information belongs in the Inspector and Compare views, with corresponding CSV export fields.

### 10. Make runtime datasets atomic

Expand the manifest with:

- Schema version.
- Dataset/model version.
- File names, sizes, and SHA-256 hashes.
- Source regulation/workbook hashes.
- Extractor version and provenance.

Loading becomes all-or-nothing:

1. Read the manifest.
2. Validate its schema and paths.
3. Verify every expected file’s size and hash.
4. Reject missing, modified, or unlisted runtime CSVs.
5. Parse all files from that one snapshot.
6. Never fall back per file to embedded data.

An explicitly selected invalid runtime snapshot will fail with a clear recovery message. Release builds will validate the embedded snapshot before packaging.

Extraction will write into a staging snapshot, validate it, write the manifest last, and only then promote it.

### 11. Finish asynchronous hardening

Introduce a common request-generation token and input signature for:

- Search
- Paths
- Affinity Watch
- Compare
- Weapon profiles
- Relevant exports

A completion is accepted only when its generation, signature, and job ID exactly match the active request.

Polling will use:

- One in-flight poll at a time.
- Initial 200 ms interval.
- Gradual backoff up to approximately one second when progress is unchanged.
- Reset to 200 ms when progress changes.
- Immediate termination on cancellation, unmount, request replacement, completion, or IPC failure.

Backend cancellation will propagate into:

- Search preparation.
- Stat enumeration.
- Parallel candidate batches.
- Nested optimizer calls used by Paths and Affinity Watch.
- Route evaluation.

Caches will use subscriber-aware in-flight entries:

- Duplicate active requests can share work.
- Rejected and cancelled entries are evicted.
- An abandoned request cannot commit a result after all subscribers cancel.
- Cache keys include dataset/schema version and all result-affecting inputs.

### 12. Verification and release gates

Add Vitest and focused frontend tests for:

- Out-of-order Compare/profile responses.
- Filter changes during execution.
- Rapid start–cancel–restart.
- Slow polls without overlap.
- Polling backoff and reset.
- Unmount cancellation.
- Shared-cache cancellation and rejection.

Add extraction and Rust tests for every semantic correction above, including snapshot corruption and mixed-version datasets.

Release validation will require:

- Python extraction validation.
- Rust formatting, tests, and Clippy.
- TypeScript type checking and Vitest.
- Existing E2E tests.
- Exact embedded snapshot validation.
- A ranking-diff report documenting expected changes caused by corrected AoW calculations.

This is a medium-to-large correctness project, so I would implement it as independently reviewable milestones in the order above. The highest-risk sections are legal AoW route construction, the ID-based effect graph, and `is_add_base_atk`; the async and snapshot work can proceed independently after the data contracts are established.
