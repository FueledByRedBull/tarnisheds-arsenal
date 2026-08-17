# Optimizer Mathematics

This document states the model the Rust optimizer implements and the properties that
make its search exact. It is the companion to
[`optimizer-overview.md`](optimizer-overview.md): the overview describes the shape of
the system, this document describes why its results are correct.

Functions are cited by name rather than by line number so the references survive
edits. Everything here lives in `core/er_optimizer_core/src/`.

## 1. Notation

There are eight character stats. Five of them affect combat and are searched:

$$i \in \lbrace \mathrm{STR}, \mathrm{DEX}, \mathrm{INT}, \mathrm{FAI}, \mathrm{ARC} \rbrace,
\qquad n = 5$$

| Symbol | Meaning |
|---|---|
| $L$ | requested character level |
| $B_c, L_c$ | starting class stat total and starting level |
| $x_j$ | current value of character stat $j$ (all eight) |
| $P$ | free points available to allocate |
| $m_i, u_i$ | lower and upper bound for searched stat $i$ |
| $T$ | points the search must distribute among *active* stats |
| $c_i = u_i - m_i$ | remaining capacity of stat $i$ |

## 2. Point budget

`compute_free_points` (`math.rs`) derives the allocatable budget from the class and
the requested level:

$$P = B_c + (L - L_c) - \sum_{j=1}^{8} x_j$$

The request is rejected if any current stat is below its class minimum, or if $P < 0$.

`build_combat_constraints` (`optimizer.rs`) then converts request constraints into
box bounds. For each searched stat:

$$m_i = \max(x_i,\; \text{floor}_i), \qquad u_i = 99$$

where $\text{floor}_i$ is the caller's minimum. A locked stat sets $m_i = u_i = \ell_i$.
Note that $m_i \ge x_i$: **the optimizer never proposes lowering a stat the character
already has.** Raising the floors consumes budget:

$$R = P - \sum_i \bigl(m_i - x_i\bigr)$$

The request is rejected if the mandatory raise exceeds $P$, or if $R$ exceeds the total
remaining capacity $\sum_i c_i$.

### Weapon requirements

`RelevantStatSearch::new` raises $m_i$ again to the selected weapon's requirements,
deducting from $R$, and discards the weapon if they cannot be met. Two-handing is
folded in here rather than treated as a separate case. `effective_str` computes

$$\mathrm{effSTR}(s) = \left\lfloor \tfrac{3s}{2} \right\rfloor$$

so the smallest displayed Strength satisfying a requirement $r$ while two-handing is

$$s_{\min}(r) = \left\lceil \tfrac{2r}{3} \right\rceil$$

`minimum_str_for_requirement` obtains this by upward scan rather than by the closed
form; the two agree for all $r \le 99$.

### Active stats and the spend target

For each (weapon, Ash of War, objective), `active_stats_for_choice` marks the stats
that can change the score. With $A$ the active set and
$C_A = \sum_{i \in A} c_i$ its capacity:

$$\text{inactiveFill} = \max(0,\; R - C_A), \qquad
T = R - \text{inactiveFill} = \min(R,\; C_A)$$

Points that active stats cannot absorb go to inactive stats by
`fill_inactive_stats`, which is deterministic and — because inactive stats cannot by
definition change the score — does not affect optimality.

Let $m^\star$ denote $m$ after that deterministic inactive fill. Active components
are unchanged.

Since $T \le C_A$ by construction, a distribution spending exactly $T$ always exists.
This is what guarantees the dynamic program below terminates with a solution.

The searched region for a weapon is therefore

$$\mathcal{X}_w = \left\lbrace \, x \in \mathbb{Z}^n \;:\;
m_i \le x_i \le u_i\ \text{for } i \in A,\;
x_i = m_i^\star\ \text{for } i \notin A,\;
\sum_{i \in A} (x_i - m_i) = T \, \right\rbrace$$

## 3. Attack rating

`calculate_ar_for_type` and `calculate_ar` (`math.rs`) compute, for each damage type
$d \in \lbrace \text{physical}, \text{magic}, \text{fire}, \text{lightning}, \text{holy} \rbrace$:

$$AR_d(x) = b_d\, r_d \Bigl( 1 + \sum_{i} I_{i,d}\, s_i\, q_i\, \gamma_d(x_i') \Bigr)$$

| Symbol | Meaning | Depends on |
|---|---|---|
| $b_d$ | weapon base damage | weapon |
| $r_d$ | reinforcement damage multiplier | weapon, upgrade |
| $I_{i,d}$ | whether stat $i$ scales damage type $d$ | attack-element correction row |
| $s_i$ | weapon scaling coefficient | weapon, affinity |
| $q_i$ | reinforcement scaling multiplier | weapon, upgrade |
| $\gamma_d$ | calc-correct curve for $d$'s curve id | weapon, damage type |
| $x_i'$ | effective stat ($\mathrm{effSTR}$ applied to STR) | request |

$$AR_{\text{total}}(x) = \sum_d AR_d(x)$$

Ash-of-War flat attack buffs and the world-scaling multiplier are applied after this,
by `apply_aow_attack_buffs` and `DamageBreakdown::scale`.

## 4. Separability — the property the search depends on

Write $\beta_d = b_d r_d$, which is constant in $x$. Then

$$AR_d(x) = \beta_d + \sum_i \beta_d\, I_{i,d}\, s_i\, q_i\, \gamma_d(x_i')$$

Summing over damage types and collecting terms:

$$AR_{\text{total}}(x) = \underbrace{\sum_d \beta_d}_{\text{constant}}
\;+\; \sum_i \underbrace{\Bigl( \sum_d \beta_d\, I_{i,d}\, s_i\, q_i\, \gamma_d(x_i')
\Bigr)}_{\textstyle g_i(x_i),\ \text{depends on } x_i \text{ alone}}$$

> **Lemma (separability).** $AR_{\text{total}}$ and $AR_{\text{physical}}$ are
> additively separable in the searched stats: each is a constant plus a sum of
> single-variable terms, with no cross-stat products and no intermediate rounding.

This holds because `calculate_ar_for_type` multiplies a stat-independent base by
$(1 + \text{sum of independent contributions})$ and **rounds nowhere**. It is the
load-bearing assumption of the entire optimizer.

> **Proof obligation.** Any change that introduces a cross-stat term, or that floors
> or truncates a damage component the way the in-game display does, breaks this lemma
> and invalidates the exactness proof. The DP-versus-exhaustive regression test covers
> representative data, but cannot prove arbitrary formula changes correct. Treat such
> a change as a redesign of the search, not a refinement of the formula.

## 5. Dynamic program for the AR objectives

Let $F$ be the objective's AR function. `best_ar_combat_stats` precomputes, for each
active stat $i$ and each
$a \in [0, \min(c_i, T)]$:

$$\Delta_i(a) = F(m^\star + a\,e_i) - F(m^\star)$$

By the lemma, $\Delta_i(a) = g_i(m_i + a) - g_i(m_i)$ — independent of what the other
stats hold. The recurrence over $D_i(p)$, the best value using the first $i$ active
stats and spending exactly $p$, is therefore exact:

$$D_i(p) = \max_{0 \le a \le \min(c_i,\, p)} \bigl[\, D_{i-1}(p - a) + \Delta_i(a) \,\bigr],
\qquad D_0(0) = F(m^\star)$$

The answer is $D_{|A|}(T)$.

> **Why one state per spend level suffices.** Suppose allocations $\alpha$ and $\beta$
> both spend $p$ over the first $i$ stats, with $\alpha$ preferred. Any completion
> $\delta$ over the remaining stats adds the same amount to both, because $\Delta$ terms
> do not depend on the current partial allocation. So $\alpha + \delta$ is preferred to
> $\beta + \delta$ for every $\delta$: dominance is preserved under extension, and
> discarding $\beta$ cannot lose the optimum. This exchange argument is valid *only*
> under the separability lemma.

### Comparison order

`better_ar_allocation` compares states lexicographically:

1. primary score descending — $AR_{\text{physical}}$ for Max Physical AR, $AR_{\text{total}}$ otherwise (`ar_primary`);
2. total AR descending;
3. the stat vector itself ascending.

Rule 3 makes the retained allocation the lexicographically smallest among exact ties,
so the result does not depend on iteration order.

### Cost

Let $c_{\max} = \max_i \min(c_i, T)$. Since $u_i = 99$, we have $c_{\max} \le 99$
regardless of level.

| Stage | Cost | Notes |
|---|---|---|
| Precompute $\Delta_i$ | $\sum_i (\min(c_i,T) + 1)$ AR evaluations | the expensive part; curve lookups |
| Recurrence | $O(\lvert A \rvert \cdot T \cdot c_{\max}) \subseteq O(n \cdot T \cdot c_{\max})$ float additions | cheap; **linear in $T$**, not quadratic |
| Working memory | $O(T)$ states | |

The comparison worth making is against naive enumeration. Without binding per-stat
caps it visits $\binom{T + n - 1}{n - 1}$ allocations — roughly
$4.6 \times 10^6$ at $T = 100$ and $n = 5$ — each requiring a full AR evaluation.
Caps can only reduce that count. The dynamic program needs a few hundred AR
evaluations for the same exact answer.

### Floating-point boundary

$\Delta_i$ values and the accumulated $D_i(p)$ are `f32`, and summing them is not
bit-identical to evaluating $F$ directly at the same stats. **The accumulated totals
select an allocation and nothing else.** `search_ar_work_unit` recomputes AR from the
chosen stat vector via `calculate_base_weapon_metric` before scoring, ranking, or
display.

The consequence is that rounding drift can select a near-tied allocation if the
accumulation changes their ordering, but it can never produce a reported number that
disagrees with `calculate_ar`. Preserve this: do not promote a $D_i(p)$ value into a
result metric.

## 6. The remaining objectives

Bleed and the two Ash-of-War objectives do not use the AR-only dynamic program: their
primary score is status or skill damage, with AR used only as a tie-break. Therefore
`search_work_unit_exhaustive` enumerates $\mathcal{X}_w$ directly — but only over the
active set, which is usually far smaller than five stats. The number of distributions
spending exactly $T$ is the generating-function coefficient

$$N(T) = [z^T] \prod_{i \in A} \bigl( 1 + z + z^2 + \cdots + z^{c_i} \bigr)$$

`count_relevant_distributions` evaluates this, and identical $(m, u, A, R)$ keys share
one cached count across weapons.

Scores are:

```math
S(x) = \begin{cases}
AR_{\text{total}}(x) & \text{Max AR} \\
AR_{\text{physical}}(x) & \text{Max Physical AR} \\
\mathrm{Bleed}(x) & \text{Bleed, then AR} \\
AoW_{\text{first}}(x) & \text{AoW First Hit} \\
AoW_{\text{sequence}}(x) & \text{AoW Full Sequence}
\end{cases}
```

**Bleed, then AR maximizes bleed alone.** AR is a tie-break, never a summand. One
consequence is worth stating because it is not obvious: `active_stats_for_choice`
marks a stat active if it can raise AR **or** bleed, so this objective enumerates the
full AR-relevant space even though its score depends only on Arcane. That breadth is
required for the AR tie-break to be exact, and it makes this the most expensive
objective in the app.

During the broad search, bleed is computed by `calculate_bleed_buildup` and
`apply_aow_bleed_buffs` — an exact bleed-only path. The full seven-status calculation
runs only for tie-breaks and retained rows. `bleed_only_calculator_matches_full_status_for_open_choices`
pins the two paths together across both profiles and both settings of the
`status_buildup_scales` rule.

## 7. Ranking

Candidates are scored across every legal (weapon, affinity, Ash of War, upgrade, stat
allocation) and reduced to the best $K$ by `push_scored_top_k`.

Ranking is staged so expensive metrics are computed only when cheaper comparisons tie.
`compare_known_candidate_metrics` compares on whatever is already known — score, then
AR, then AoW damage, then bleed — and `complete_scored_candidate_tie_breaks` fills in
the next metric only when the comparison comes back equal. `could_enter_scored_top_k`
is a cheap admission gate; when a metric is absent it reports *equal* rather than
*less*, so it can only admit extra candidates, never discard a real one.

Final ordering falls through to weapon id, upgrade, skill id, and the combat-stat
vector. Parallel work is split by individual Ash-of-War choice; each worker builds a
local top-$K$ and the merge applies the same comparison rules, so **parallelism changes
throughput and not results.**

## 8. Scope of the exactness claim

The search is exact with respect to the implemented combat model: for the searched
space it returns a true optimum, not an approximation or a sampled estimate.

The model itself is deliberately narrower than the game. Enemy defense, negation,
resistance growth, proc explosion damage, poise and stance damage are not modeled;
route stamina is reported but is not an objective; temporary buff stacking is not
modeled as a universal layer. See the boundaries list in the README for the current
set.
