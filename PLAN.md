# Elden Ring Build Optimizer

## What It Does

Given a level budget and any combination of locked/unlocked variables, find the optimal combination of:

**WEAPON × AFFINITY × AoW × STAT DISTRIBUTION**

that maximizes AR for a given character level and upgrade constraint.

---

## The Core Idea

The user has 5 variables they can either **lock** or **leave open**:

| Variable | Locked Example | Open = we search over |
|---|---|---|
| Weapon | Uchigatana | every weapon in the game |
| Affinity | Bleed | every valid affinity for that weapon |
| AoW | Seppuku | every valid AoW (passive effects only) |
| Upgrade level | max +4 | +0 to +4 (or +25) |
| Stat distribution | 40dex/45arc locked | every combo within level budget |

**Whatever is left open, we brute force.**

---

## User Inputs

```
Starting Class: [ dropdown ]
  Vagabond / Warrior / Hero / Bandit / Astrologer /
  Prophet / Samurai / Prisoner / Confessor / Wretch

Character Level: [ ]  (u16, max 713)

  total_stat_points = class_base_total + (level - class_base_level)
  free_points = total_stat_points - sum(current 8 stats)
  computed as signed i32 before constructing StatIter — see below

Current Stats (what you have now):
  VIG [ ] MND [ ] END [ ] STR [ ] DEX [ ] INT [ ] FAI [ ] ARC [ ]
  (must be >= class base stats, must sum <= total_stat_points)

  VIG / MND / END → fixed by user, not touched by optimizer
  STR / DEX / INT / FAI / ARC → optimizer distributes free points across these

Upgrade Constraint: max [ ]
  e.g. +4 means we only consider +0 to +4 for all weapons

Two Handing: [ checkbox ]
  multiplies effective STR by 1.5
  affects BOTH requirement checks AND STR contribution to AR
  hard capped at 99 before any lookup

Weapon:   [ dropdown or LEAVE OPEN ]
Affinity: [ dropdown or LEAVE OPEN ]
AoW:      [ dropdown or LEAVE OPEN ]

Optimize For:
( ) Max AR
( ) Max AR + Bleed Buildup
```

### Starting Class Base Stats

| Class | LVL | VIG | MND | END | STR | DEX | INT | FAI | ARC | Total |
|---|---|---|---|---|---|---|---|---|---|---|
| Vagabond | 9 | 15 | 10 | 11 | 14 | 13 | 9 | 9 | 7 | 88 |
| Warrior | 8 | 11 | 12 | 11 | 10 | 16 | 10 | 8 | 9 | 87 |
| Hero | 7 | 14 | 9 | 12 | 16 | 9 | 7 | 8 | 11 | 86 |
| Bandit | 5 | 10 | 11 | 10 | 9 | 13 | 9 | 8 | 14 | 84 |
| Astrologer | 6 | 9 | 15 | 9 | 8 | 12 | 16 | 7 | 9 | 85 |
| Prophet | 7 | 10 | 14 | 8 | 11 | 10 | 7 | 16 | 10 | 86 |
| Samurai | 9 | 12 | 11 | 13 | 12 | 15 | 9 | 8 | 8 | 88 |
| Prisoner | 9 | 11 | 12 | 11 | 11 | 14 | 14 | 6 | 9 | 88 |
| Confessor | 10 | 10 | 13 | 10 | 12 | 12 | 9 | 14 | 9 | 89 |
| Wretch | 1 | 10 | 10 | 10 | 10 | 10 | 10 | 10 | 10 | 80 |

```
total_stat_points = class_base_total + (character_level - class_base_level)
free_points = total_stat_points - sum(all 8 current stats)
```

Example: Samurai level 150
```
total_stat_points = 88 + (150 - 9) = 229
if current stats sum to 180, free_points = 229 - 180 = 49
```

---

## Two Handing

Two handing multiplies effective STR by 1.5 for **both** requirement checks and AR calculation.
Hard cap at 99 to prevent out-of-bounds curve lookup.

```rust
let effective_str = if two_handing {
    ((stats.str as f32 * 1.5) as u8).min(99)
} else {
    stats.str
};

// flows into BOTH
if !meets_requirements(weapon, effective_str, &stats) { continue }
let ar = calculate_ar(weapon, upgrade, &stats, effective_str, data);
```

---

## Minimum Stat Requirements

```rust
fn meets_requirements(weapon: &Weapon, effective_str: u8, stats: &Stats) -> bool {
    effective_str  >= weapon.req_str &&
    stats.dex      >= weapon.req_dex &&
    stats.int      >= weapon.req_int &&
    stats.fai      >= weapon.req_fai &&
    stats.arc      >= weapon.req_arc
}
```

---

## Free Points Calculation

Compute as signed i32 before constructing StatIter.
If the user enters stats that exceed their level budget, u16 subtraction
silently wraps to ~65000 and the optimizer runs forever.

```rust
let total_stat_points =
    class.base_total as i32 + (level as i32 - class.base_level as i32);
let current_sum: i32 = stats.sum_all_8() as i32;
let free = total_stat_points - current_sum;

if free < 0 {
    return Err("Current stats exceed level budget");
}

let free: u16 = free as u16;
// now safe to construct StatIter
```

---

## Stat Distribution Search

We distribute free points across 5 combat stats (STR, DEX, INT, FAI, ARC).
VIG, MND, END are fixed by the user and never touched by the optimizer.

### Constrained Generation — not filtered enumeration

A naive odometer over 5 stats each 1-99 has ~9 billion states.
Filtering by sum afterwards is not acceptable.

The correct approach is **constrained generation**: at each advance, decrement
the rightmost stat that is above its minimum, then repack all freed points onto
the stats to its right. Every yielded combination sums exactly to free_points
by construction.

```rust
struct StatIter {
    mins: [u8; 5],    // floors for STR/DEX/INT/FAI/ARC
    free: u16,        // total free points to distribute
    current: [u8; 5], // current iterator state
    done: bool,
}

impl StatIter {
    fn new(mins: [u8; 5], free: u16) -> Self {
        // pack points left-to-right (STR first, then DEX, etc.)
        // always valid regardless of how large free is
        // example: mins=[10,10,10,10,10], free=50
        //   fill STR to min(99, 10+50)=60, remainder=0
        //   current=[60,10,10,10,10]
        // never overflows any stat past 99
        let mut current = mins;
        let mut remaining = free;
        for i in 0..5 {
            let can_add = (99u16 - mins[i] as u16).min(remaining) as u8;
            current[i] = mins[i] + can_add;
            remaining -= can_add as u16;
            if remaining == 0 { break; }
        }
        Self { mins, free, current, done: false }
    }
}

impl Iterator for StatIter {
    type Item = [u8; 5];

    fn next(&mut self) -> Option<[u8; 5]> {
        if self.done { return None; }
        let result = self.current;

        // advance:
        // 1. scan right-to-left from index 3 (not 4 — last stat has nowhere to donate)
        // 2. find the first stat above its min
        // 3. decrement it by 1
        // 4. repack all freed points onto stats to the right, left-to-right, capped at 99
        // 5. if no stat found above its min, set done = true
        //
        // boundary conditions:
        // - never decrement below mins[i]
        // - never fill a stat above 99
        // - repacking may spread across multiple stats if one fills to 99

        Some(result)
    }
}
```

For typical builds (50-80 free points across 5 stats) this is hundreds of thousands
of valid states at most — microseconds in Rust.

---

## Data Layer

**5 CSV files, dumped once from regulation.bin, shipped with the app.**

### Normalization convention (document in Phase 1, verify before writing any Rust)

- All scaling values stored as **0.0–1.0** (divide raw param values by 100.0 during dump)
- calc_correct multipliers stored as **0.0–1.0**
- damage_mult and scaling_mult from reinforce stored as-is (already multipliers)

### calc_correct indexing convention (document in Phase 1)

stat_value in calc_correct.csv ranges 1–99.
Back each curve with a Vec<f32> of length 100.
Index 0 is stored as 0.0 and never read.
stat_value maps directly to index without any offset.

```rust
// correct — stat_value 1..=99 maps to index 1..=99
let curve_mult = data.calc_correct[curve_id][stat_value as usize];

// never subtract 1, never use index 0
```

Document this once in Phase 1. Do not mix conventions anywhere in the codebase.

```
weapons.csv  (one row per weapon × affinity combination)
  → weapon_id
  → name
  → affinity
  → base_physical, base_magic, base_fire, base_lightning, base_holy
  → str_scaling, dex_scaling, int_scaling, fai_scaling, arc_scaling
    (normalized 0.0-1.0)
  → req_str, req_dex, req_int, req_fai, req_arc
  → reinforce_type
  → attack_element_correct_id
  → curve_id_str    ← how STR value converts to scaling multiplier
  → curve_id_dex    ← how DEX value converts to scaling multiplier
  → curve_id_int    ← how INT value converts to scaling multiplier
  → curve_id_fai    ← how FAI value converts to scaling multiplier
  → curve_id_arc    ← how ARC value converts to scaling multiplier
  → is_somber

  NOTE: curve_id is per-stat not per-damage-type.
  STR contributing to physical uses curve_id_str at effective_str.
  DEX also contributing to physical (quality) uses curve_id_dex at stats.dex.
  Both happen in the same physical pass using their own curves.

reinforce.csv  (one row per reinforce_type × level)
  → reinforce_type
  → level  (0-25, or 0-10 for somber)
  → physical_damage_mult
  → magic_damage_mult
  → fire_damage_mult
  → lightning_damage_mult
  → holy_damage_mult
  → str_scaling_mult
  → dex_scaling_mult
  → int_scaling_mult
  → fai_scaling_mult
  → arc_scaling_mult

calc_correct.csv  (pre-expanded during dump to 100 rows per curve)
  → curve_id
  → stat_value  (0-99, index 0 = 0.0 unused, valid range 1-99)
  → multiplier  (normalized 0.0-1.0)
  raw param is 5 control points + exponents
  expand during Phase 1, never at runtime

attack_element_correct.csv
  → attack_element_correct_id
  → str_scales_physical,  str_scales_magic,  str_scales_fire,  str_scales_lightning,  str_scales_holy
  → dex_scales_physical,  dex_scales_magic,  dex_scales_fire,  dex_scales_lightning,  dex_scales_holy
  → int_scales_physical,  int_scales_magic,  int_scales_fire,  int_scales_lightning,  int_scales_holy
  → fai_scales_physical,  fai_scales_magic,  fai_scales_fire,  fai_scales_lightning,  fai_scales_holy
  → arc_scales_physical,  arc_scales_magic,  arc_scales_fire,  arc_scales_lightning,  arc_scales_holy

aow.csv  (passive status effects only, active skill damage out of scope)
  → aow_id
  → name
  → bleed_buildup_add
  → frost_buildup_add
  → poison_buildup_add
  → valid_weapon_types
```

Users **never need regulation.bin or WitchyBND**.

---

## AR Calculation

### The Formula

For each damage type:

```
AR_type = actual_base * (1.0 + sum of all contributing stat scaling)

actual_base         = base_damage * damage_mult
each contributing stat adds:
  scaling_for_stat * scaling_mult_for_stat * curve_mult_for_stat

curve_mult_for_stat = calc_correct[curve_id_<stat>][stat_value as usize]
                      STR uses effective_str, all others use raw stat value
```

Base is applied once. All stat contributions summed before multiplying.
Correctly handles quality weapons (STR + DEX both contribute to physical)
without double-counting base.

### calculate_ar

`reinforce`, `aec`, and all 5 `curve_mults` are hoisted once above the damage type loop.
5 curve lookups total, not 25.

```rust
fn calculate_ar(
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    effective_str: u8,   // pre-computed, capped at 99
    data: &GameData,
) -> DamageBreakdown {
    // hoisted — looked up once
    let reinforce = &data.reinforce[weapon.reinforce_type][upgrade as usize];
    let aec = &data.attack_element_correct[weapon.attack_element_correct_id as usize];

    // 5 curve lookups total — reused across all 5 damage type passes
    let curve_mults = [
        data.calc_correct[weapon.curve_id_str][effective_str as usize],
        data.calc_correct[weapon.curve_id_dex][stats.dex as usize],
        data.calc_correct[weapon.curve_id_int][stats.int as usize],
        data.calc_correct[weapon.curve_id_fai][stats.fai as usize],
        data.calc_correct[weapon.curve_id_arc][stats.arc as usize],
    ];

    DamageBreakdown {
        physical: calculate_ar_for_type(
            weapon.base_physical * reinforce.physical_damage_mult,
            &build_contributions(weapon, reinforce, aec, &curve_mults, DamageType::Physical),
        ),
        magic: calculate_ar_for_type(
            weapon.base_magic * reinforce.magic_damage_mult,
            &build_contributions(weapon, reinforce, aec, &curve_mults, DamageType::Magic),
        ),
        fire: calculate_ar_for_type(
            weapon.base_fire * reinforce.fire_damage_mult,
            &build_contributions(weapon, reinforce, aec, &curve_mults, DamageType::Fire),
        ),
        lightning: calculate_ar_for_type(
            weapon.base_lightning * reinforce.lightning_damage_mult,
            &build_contributions(weapon, reinforce, aec, &curve_mults, DamageType::Lightning),
        ),
        holy: calculate_ar_for_type(
            weapon.base_holy * reinforce.holy_damage_mult,
            &build_contributions(weapon, reinforce, aec, &curve_mults, DamageType::Holy),
        ),
    }
}
```

### build_contributions

Takes pre-computed `curve_mults` — no lookups inside.

```rust
fn build_contributions(
    weapon: &Weapon,
    reinforce: &ReinforceLevel,
    aec: &AttackElementCorrect,
    curve_mults: &[f32; 5],        // [str, dex, int, fai, arc] — pre-computed
    damage_type: DamageType,
) -> [ScalingContribution; 5] {
    [
        ScalingContribution {
            scaling:      weapon.str_scaling,
            scaling_mult: reinforce.str_scaling_mult,
            curve_mult:   curve_mults[0],
            contributes:  aec.str_scales(damage_type),
        },
        ScalingContribution {
            scaling:      weapon.dex_scaling,
            scaling_mult: reinforce.dex_scaling_mult,
            curve_mult:   curve_mults[1],
            contributes:  aec.dex_scales(damage_type),
        },
        ScalingContribution {
            scaling:      weapon.int_scaling,
            scaling_mult: reinforce.int_scaling_mult,
            curve_mult:   curve_mults[2],
            contributes:  aec.int_scales(damage_type),
        },
        ScalingContribution {
            scaling:      weapon.fai_scaling,
            scaling_mult: reinforce.fai_scaling_mult,
            curve_mult:   curve_mults[3],
            contributes:  aec.fai_scales(damage_type),
        },
        ScalingContribution {
            scaling:      weapon.arc_scaling,
            scaling_mult: reinforce.arc_scaling_mult,
            curve_mult:   curve_mults[4],
            contributes:  aec.arc_scales(damage_type),
        },
    ]
}
```

### calculate_ar_for_type

```rust
struct ScalingContribution {
    scaling:      f32,   // from weapons.csv, normalized 0.0-1.0
    scaling_mult: f32,   // from reinforce.csv
    curve_mult:   f32,   // from calc_correct.csv, normalized 0.0-1.0
    contributes:  bool,  // from attack_element_correct.csv
}

fn calculate_ar_for_type(
    actual_base: f32,
    contributions: &[ScalingContribution; 5],
) -> f32 {
    let bonus: f32 = contributions.iter()
        .filter(|c| c.contributes)
        .map(|c| c.scaling * c.scaling_mult * c.curve_mult)
        .sum();
    actual_base * (1.0 + bonus)
}
```

---

## Output

```
#1  Uchigatana | Bleed | Seppuku | +25
    STR:18  DEX:40  INT:9  FAI:9  ARC:55
    AR: 721  →  { physical: 721 }
    Bleed Buildup: 98

#2  Uchigatana | Occult | Seppuku | +25
    STR:18  DEX:45  INT:9  FAI:9  ARC:50
    AR: 698  →  { physical: 698 }
    Bleed Buildup: 86

#3  Nagakiba | Bleed | Seppuku | +25
    STR:16  DEX:40  INT:9  FAI:9  ARC:57
    AR: 695  →  { physical: 695 }
    Bleed Buildup: 105
```

Split damage always shows full breakdown.
User decides if split is worth it — tool does not make that call.

---

## Upgrade Comparison Table

```
              +0     +1     +2    ...   +10   ...   +25
Bleed Uchi   412    445    478         634         821
Occult Uchi  389    421    453         601         789
```

---

## Out of Scope

- Active AoW skill damage (motion values, hit type multipliers)
- Poise / hyper armor
- Stamina costs
- Spell scaling
- Enemy resistances

---

## Tech Stack

```
Data dump  → Python  (extend existing stat_randomizer_gui.py pipeline)
Math core  → Rust    (brute force optimizer)
UI         → PyQt
Glue       → PyO3 / maturin
```

---

## Build Phases

### Phase 1 — Data Dump (1-2 days)
- Extend regulation.bin pipeline for all 5 params
- Pre-expand CalcCorrectGraph to 100 rows per curve (index 0 = 0.0, unused)
- **Indexing convention: stat_value 1–99 → index 1–99, no offset, ever**
- **Normalization convention: all scaling values ÷ 100.0 → stored as 0.0–1.0**
- One row per weapon × affinity in weapons.csv
- curve_id columns are per-stat (curve_id_str/dex/int/fai/arc)
- Validate AR against fextralife for:
  - pure DEX weapon (e.g. Uchigatana keen)
  - quality weapon (e.g. Lordsworn's Greatsword) — validates multi-stat physical
  - arcane weapon (e.g. Reduvia or blood-affinity dagger) — validates curve_id_arc + normalization
- **Do not proceed until all three match exactly**

### Phase 2 — Rust Math Core (2-3 days)
- Free point calculation as signed i32, error on negative
- `calculate_ar()` hoisting reinforce, aec, and curve_mults above damage type loop
- `build_contributions()` taking pre-computed curve_mults, no lookups inside
- `calculate_ar_for_type()` summing all stat contributions before multiplying
- `StatIter` with left-to-right init and correct advance:
  - scan right-to-left from index 3
  - decrement first stat above its min
  - repack freed points left-to-right onto remaining stats, cap at 99
  - cascade if a stat fills to 99
  - set done when no stat above min exists
- `meets_requirements()` with effective_str capped at 99
- Class-aware free point calculation
- Brute force optimizer loop
- Expose to Python via PyO3

### Phase 3 — UI (2 days)
- Starting class dropdown
- Character level input (u16) + derived free points display
- Weapon / affinity / AoW dropdowns (blank = open, selected = locked)
- Stat inputs
- Upgrade constraint + two handing checkbox
- Results table with full damage breakdown
- Upgrade comparison table

### Phase 4 — Validate & Ship (1 day)
- Test against known meta builds
- Confirm quality and arcane weapon results match fextralife
- Package with pre-dumped CSVs
- Done

**Total: ~1 week**

---

## What Makes This Different

Every existing tool is either:
- A static wiki (fextralife)
- A manual damage calculator (you input everything, it outputs one number)
- A spreadsheet

This is the first tool that says:

**"tell me what you have, tell me what you want, i'll find the best combination"**