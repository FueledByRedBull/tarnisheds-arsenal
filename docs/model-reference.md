# Calculation model reference

[Home](../README.md) · [Architecture](design/optimizer-overview.md) ·
[Mathematics](design/optimizer-math.md) · [Extraction](../tools/phase1/README.md)

Use this page to interpret results, check supported mechanics, and understand
known limits. For how to use each workspace, see the [interface guide](../README.md#interface).
Version-specific changes and measurement results belong in the
[release notes](release-notes/README.md).

## Reading results

| Value | Meaning |
| --- | --- |
| AR / physical AR | Modeled weapon attack rating before enemy defenses |
| Status | Buildup, not status-proc damage or resistance growth |
| Raw skill first hit / sequence | Damage from the selected supported PvE route, before enemy defenses |
| Stance / poise | Raw PvE stance damage from supported weapon moves or skill hits, independent of AR |
| Stamina | Route action cost; reported, not optimized |

Enemy defense, negation, resistance growth, and proc explosions are outside the
model. Temporary effects are modeled only where explicitly supported; there is no
general buff-stacking simulation. An unavailable value is not zero damage.

### Numerical contract and identity

Damage ranking evaluates loaded binary coefficients exactly. Status scaling uses
the existing `f32` operation sequence and gameplay floors; its completed value is
then represented exactly in ranking. Display values are rounded afterward. This prevents
intermediate floating-point rounding from changing close winners; it does not
recover original decimal precision or establish in-game accuracy. The
[mathematical contract](design/optimizer-math.md#numerical-contract) defines the
arithmetic, tie order, and proof obligations.

The current snapshot model is `aow-routes-effects-v8`; runtime results and caches
use `aow-routes-effects-v8/exact-v2`. Storage schema and calculation semantics are
separate versions. The [extraction guide](../tools/phase1/README.md#snapshot-contract)
owns the storage format and regeneration requirements. Incompatible saved results
must be recalculated; reusable saved inputs are retained.

The former floating-point canonical-tie counterexample is now checked by
`exact_dp_matches_exhaustive_on_the_former_f32_counterexample`, which requires
agreement on both the complete exact key and combat stats.
`exact_completion_preserves_sub_ulp_order` checks ordering across completion;
the separate all-five-stat oracle checks active-stat relevance. These regressions
live in the [optimizer tests](../core/er_optimizer_core/src/optimizer/tests.rs).
They establish implementation agreement for their fixtures, not gameplay fidelity.

## Vanilla

The bundled snapshot targets **Elden Ring 1.17**. Objectives are **Max AR**,
**Max Physical AR**, **Bleed, then AR**, **AoW First Hit (PvE)**, and
**AoW Full Sequence (PvE)**.

- Starting class, current stats, level, minimums, exact locks, and weapon
  requirements constrain the stat budget.
- Upgrade searches use either exact caps or every available level from `+0` to
  the caps. Standard and Somber paths have separate caps; a weapon's actual
  reinforcement series can end earlier.
- Resolved handling controls the Strength bonus for AR and requirements, including
  paired-weapon exceptions. Each skill attack independently determines whether it
  uses that bonus for scaling.
- Selecting a weapon starts with its native skill. Changeable weapons also offer
  **Automatic (best legal skill)**. Transferable Ashes must satisfy mounting,
  weapon-type, and affinity permissions. Native skills use their separate rules;
  an absent localized name does not discard a known native ID.
- Supported Scadutree Blessing scaling is optional. Frostbite, Scarlet Rot, and
  Death Blight do not scale with attributes; reinforcement and explicit supported
  skill additions still apply.

### Skill damage and effects

Routes resolve weapon-motion and fixed/projectile components, per-hit scaling,
status motion values, supported weapon-buff timing, action stamina, and attack
attributes. Unique-weapon attacks are kept separate from transferable versions.
Repeated contacts explicitly marked in the source workbook, plus Glintblade
Phalanx's four independently hitting blades, count once per contact. First-hit
damage stays one contact, and stamina is charged once per action. Charge variants
and the one-to-three Thundercloud Form loops are separate routes. These sequence
totals assume all modeled contacts land; overlapping bullet hitboxes are not
automatically multiplied by their raw spawn count.
Stance damage includes both `weapon base poise * poise MV / 100` and the attack's
fixed `atkSuperArmor` term, for each contact.
Stamina retains the source action maximum; repeated contact counts alone do not
establish whether held loops consume stamina per tick. Those loop costs are not
independently verified.
Weapon-specific attack-element overrides and influence rates apply to each
component. Negative influence corrections select the strongest penalty instead
of adding positive contributions to it.

The following special cases depend on raw attack provenance:

| Skill | Current interpretation |
| --- | --- |
| Carian Retaliation | Vanilla sword bullets `300000682` and `300000683` use fixed base 270, independent of stats and upgrades |
| Ice Lightning Sword | Bullet attacks retain the 149 lightning and 69 Water AoE bases even without `is_add_base_atk` |
| Lifesteal Fist | Grab rows marked by raw throw flag `2` receive the weapon's critical multiplier; initial contact rows do not |

Carian Retaliation's numeric 270 comes from the raw parameters. The
[official Patch 1.04 notes](https://en.bandainamcoent.eu/elden-ring/news/elden-ring-patch-notes-104)
support removal of weapon/status scaling, not that numeric value. Katar, Pata,
and Raptor Talons apply 110% to Lifesteal Fist's grab rows.

Chilling Mist and Poisonous Mist retain separate weapon/on-hit effect records,
but their overlapping status increments remain unmodeled: available sources
conflict on engine correction and stacking. They show warnings in AR results and
are excluded from skill-damage objectives. No combined +90 status buff is claimed.

Other unsupported conditional effects are also explicit warnings. Skills with
unsupported effects on evaluated attack rows cannot compete in skill-damage
objectives. Choosing an exhaustive evaluator does not implement a missing effect;
`PerHitAttackPower` remains unsupported.

## Convergence

The **Convergence 3.0.0.1** profile is experimental and independent of Vanilla.
A version-bound reference checks weapon availability, base attack, requirements,
raw scaling, affinities, and base status. Final AR mechanics and customization
legality still need independent verification.

| Capability | Behavior |
| --- | --- |
| Character | Exact **Custom stats**; their total is not a Rune Level |
| Upgrades | One reinforcement path, `+0` through `+15` |
| Scaling | Extended `S+`/`S++` grades; no Scadutree Blessing scaling |
| Status | Fixed across stats and upgrades |
| AoW damage / ammunition AR | Unavailable until mod-specific attack and route data is modeled |
| Class optimization / Compare / Paths / Affinity Watch | Disabled until a version-pinned class catalog is verified |

For row-0 attack-element weapons, every declared nonzero attribute scaling applies
to every nonzero damage component. `levelSyncCorrectId` is not applied as a player
AR multiplier: the version-bound reference omits it. Missing tables or class rules
are never filled with Vanilla data.

## Known reference differences

The pinned [T. Clark 1.17 calculator](https://github.com/ThomasJClark/elden-ring-weapon-calculator/tree/b8a1cf8847fe67aacc7f8fcb038a9cfd6725f19a)
uses JavaScript floating-point arithmetic and a small offset before truncating
display values. Its live 1.17 calculator and [Tarnished.dev](https://www.tarnished.dev/weapon-calculator)
both display the reference values below. `exact-v2` fixes the previous
Bloodfiend's Fork (Keen) boundary disagreements in both handling modes:

| Upgrade / Arcane | Current production floor | Reference floor |
| --- | ---: | ---: |
| `+7` / 45 | 62 | 62 |
| `+25` / 50 | 68 | 68 |

These are reference-backed arithmetic checks, not live in-game measurements.
The two sites' implementation and data independence is unverified. The corrected
status operation sequence needs no epsilon; damage and optimizer accumulation
remain exact. Complex skill comparisons still have limits in source coverage and
engine semantics; see the dated
[verification results](release-notes/v0.14.0.md#verification).

In particular, raw attack-point status corrections and overlapping SpEffect
payloads do not yet have an independent engine oracle. Their correction flag
alone does not justify multiplying all hit effects by the point MV: some frost
and bleed bullet effects have a zero point MV. Per-hit status totals are modeled
estimates, and this audit does not certify those stacking/rounding mechanics.

## Checking model coverage

Profile capabilities describe which calculations a dataset offers; they do not
certify every weapon or skill. The profile header explains these limits, and
Build Detail shows the selected build's model assumptions. Older saved capability
filters remain active until removed using **Remove saved profile filters**;
excluding a supported capability excludes all results in that profile.

Run a seeded Vanilla weapon AR/base-status comparison from the repository root:

```powershell
python tools/phase4/validate_external_calculator.py --count 100 --seed 20260918
```

The runner uses pinned T. Clark source/data, the installed frontend TypeScript
compiler, and the current Rust evaluator. Each sampled configuration covers zero,
random, and maximum upgrades in both handling modes. Use another seed for new
cases or `--count 487` for every named weapon. Every Vanilla run also includes the
32 known Bloodfiend's Fork boundary cases across eight affinities. Optional `--report` and `--csv`
paths retain results. AR tolerance is 0.001 per component; status is compared
after integer flooring. This also checks selected-skill identity, not complex
projectile formulas, buff stacking, or damage after enemy defenses.

To check every candidate weapon/Ash pair against raw permissions and evaluate
all legal transferable/native configurations at upgrade endpoints in both
handling modes:

```powershell
python tools/phase4/validate_external_calculator.py --exhaustive `
  --vanilla-raw-dir <vanilla-regulation-directory> `
  --convergence-raw-dir <convergence-regulation-directory> `
  --paramdex-dir <paramdex-definitions-directory>
```

Replace the angle-bracket arguments with local raw-input directories from the
[extraction workflow](../tools/phase1/README.md). This checks legal and rejected
pairs from raw Gem permissions and finite production evaluation. It does not
independently verify the numeric formulas. Raw inputs remain local, and
unsupported profile capabilities remain explicit exclusions.
