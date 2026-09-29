# Optimizer mathematics

Use this page to review the searched domain, recurrence, and exact-arithmetic
claim behind the Rust optimizer. Its conclusions still require the separability,
first-hit identity, and active-set obligations below. The companion
[`optimizer-overview.md`](optimizer-overview.md) covers implementation structure,
ranking, and parallel work; verification and release procedures live in their
linked workflow docs. Functions named here live in `core/er_optimizer_core/src/`.

**Navigation:** [Home](../../README.md) · [Optimizer overview](optimizer-overview.md) ·
[Performance](../performance.md) · [Runtime invariants](../architecture/runtime-invariants.md) ·
[Model reference](../model-reference.md)

**Find a topic:** [Symbols](#symbols) ·
[Point budget](#1-notation-and-point-budget) ·
[Feasible domain](#2-feasible-spend-domain) ·
[Separability](#3-attack-rating-and-separability) ·
[Recurrence](#4-generalized-lexicographic-dynamic-program) ·
[Numerical contract](#numerical-contract) · [Claim boundaries](#7-scope-of-the-claims)

The equations are a fan-made model reconstructed from version-bound regulation data.
They are not an official FromSoftware specification. The model boundary is summarized
in [Section 7](#7-scope-of-the-claims).

## 1. Notation and point budget

The optimizer distributes points among the five combat stats STR, DEX, INT, FAI,
and ARC. Index $i$ ranges over those stats in that order, $j$ over all eight
character stats, and $d$ over the damage types physical, magic, fire, lightning,
and holy.

### Symbols

| Symbol | Meaning | Defined in |
|---|---|---|
| $L$, $L_c$ | requested character level; starting class level | §1 |
| $N_c$ | starting class stat total | §1 |
| $a_j$ | current value of character stat $j$ | §1 |
| $f_i$ | requested floor for combat stat $i$ | §1 |
| $P$ | free points before floors and weapon requirements | §1 |
| $m_i$, $u_i$ | lower and upper bounds for combat stat $i$ | §1 |
| $c_i=u_i-m_i$ | remaining capacity of combat stat $i$ | §1 |
| $R$ | points left after mandatory raises | §1 |
| $\tau(s)$ | effective Strength with the two-handed bonus | §1 |
| $A$ | active stats: those any numeric comparison component depends on | §2 |
| $C_A$, $C_I$ | total capacity of active and inactive stats | §2 |
| $p$, $p_{\min}$, $p_{\max}$ | points spent on active stats, and their feasible range | §2 |
| $h(q)$ | canonical inactive-stat values after spending $q$ inactive points | §2 |
| $\mathcal X_w$ | searched allocations for one work unit | §2 |
| $b_d$, $r_d$, $\beta_d$ | base damage, reinforcement damage multiplier, and $\beta_d=b_dr_d$ | §3 |
| $I_{i,d}$ | attack-element routing flag | §3 |
| $s_i$, $q_i$ | weapon scaling and reinforcement scaling of stat $i$ | §3 |
| $\sigma_{i,d}$ | correction coefficient: overwrite rate, or $s_i$ times influence rate | §3 |
| $\gamma_d$, $x_i'$ | calc-correct curve for damage type $d$; effective stat value | §3 |
| $\kappa_{i,d}$ | fixed coefficient $\beta_dI_{i,d}\sigma_{i,d}q_i$ | §3 |
| $\delta_d$, $\omega$ | configured Ash attack-power buff; world damage multiplier | §3 |
| $\iota$, $s_i'$, $t_i$ | influence rate, scaling, and contribution under an AoW correction record | §3 |
| $\lambda_0$, $\lambda$ | bleed constant and its ARC-only term | §4 |
| $H_{r,k}$, $H^0_{r,k}$, $\phi_{r,k,i}$ | damage of hit $k$ on route $r$, its constant, its stat-$i$ term | §4 |
| $M_o$, $K_o$, $S_o$ | numeric comparison vector, full key, objective score | §2, §4 |
| $Q_o$ | leading pair of $M_o$ shared across Ash choices | §4 |
| $\Delta_i(v)$, $D_i(p)$, $e_i$ | per-stat delta, DP state, unit change in stat $i$ | §4 |
| $T$, $c_{\max}$ | DP cost parameters | §4 |

### Point budget

`compute_free_points` (`math.rs`) computes

$$P = N_c + (L-L_c) - \sum_{j=1}^{8} a_j.$$

The request is rejected if a current stat is below its class minimum or $P<0$.
With class budgets, the desktop sends each combat stat at its class base value
and derives the level from the entered stats. Every point above the class base is
therefore free to move; floors and exact locks carry the user's constraints.

`build_combat_constraints` (`optimizer.rs`) applies requested floors and locks:

$$m_i=\max(a_i,f_i), \qquad u_i=99,$$

with $m_i=u_i$ for a locked stat. Mandatory raises consume budget, so initially

$$R=P-\sum_i(m_i-a_i).$$

`RelevantStatSearch::new` raises the minima again when needed to satisfy the
selected weapon's requirements and deducts those raises from $R$. The request or
weapon is rejected if a mandatory raise exceeds the budget or if $R>\sum_i c_i$.
All later capacities $c_i$ use the final bounds after floors, locks, and
requirements. Every retained bound satisfies $a_i\le m_i\le u_i\le99$.

Resolved weapon handling determines effective Strength for weapon requirements and
weapon AR. When the weapon receives the Strength bonus,

$$\tau(s)=\left\lfloor\frac{3s}{2}\right\rfloor.$$

Forced bow-family handling and paired/no-bonus weapon exceptions are resolved
before both uses. Each AoW attack row then independently uses that resolved value
or raw STR for its own scaling curve. Row-level suppression changes only that
row's damage; it does not change whether the weapon meets its requirements. All
choices are fixed for the work unit and remain functions of STR alone. Generated
and validated calc-correct curves cover every reachable effective value through
148, the maximum effective Strength $\tau(99)$ for the declared stat domain.

## 2. Feasible spend domain

For a fixed weapon, affinity, upgrade, Ash of War, route, and objective, let
$M_o(x)$ be the numeric comparison vector in Section 4. The full key is
$K_o(x)=(M_o(x),-x)$, with ascending combat stats breaking numeric ties.

The active-set soundness obligation is: for every numeric component $r$, every
inactive stat $i$, and every legal value $s$,

$$i\notin A\ \Longrightarrow\ g_{r,i}(s)=g_{r,i}(m_i),\qquad m_i\le s\le u_i,$$

where $g_{r,i}$ is that stat's contribution. Over-including stats is safe;
omitting a changing contribution is not. `active_stats_for_choice` conservatively
combines weapon AR, bleed, and AoW-row dependencies. Its implementation must
satisfy this obligation independently of the DP. Let

$$C_A=\sum_{i\in A}c_i, \qquad C_I=\sum_{i\notin A}c_i.$$

If $p$ points are spent on active stats, the remaining $R-p$ must fit in inactive
stats. Therefore the complete feasible active-spend interval is

$$p_{\min}=\max(0,R-C_I), \qquad p_{\max}=\min(R,C_A).$$

The total-capacity check guarantees $p_{\min}\le p_{\max}$, so at least one
feasible final state exists. Termination follows separately from the finite stat
and budget loops.

For each integer $0\le q\le C_I$, $h(q)$ contains the lexicographically smallest
final inactive stat values with $m_i\le h_i(q)\le u_i$ and
$\sum_{i\notin A}(h_i(q)-m_i)=q$. These are final values, not increments.
`fill_inactive_stats` computes $h$ by giving each inactive stat, in stat order,
only what later inactive stats cannot hold. Under active-set soundness, inactive
stats affect only the stat-vector tie-break, so this completion loses no preferred
result.

The searched region is the union

```math
\mathcal X_w=
\bigcup_{p=p_{\min}}^{p_{\max}}
\lbrace
x\in\mathbb Z^5:
m_i\le x_i\le u_i,\
\sum_{i\in A}(x_i-m_i)=p,\
x_i=h_i(R-p)\text{ for }i\notin A
\rbrace.
```

Restricting the search to $p_{\max}$ needs a separate dominance argument, including
tie handling. Searching the full feasible interval avoids that requirement: an
interior spend remains eligible when a curve decreases or a numeric plateau leaves
it with the preferred combat-stat vector.

## 3. Attack rating and separability

`exact_ar` (`math/exact.rs`) evaluates the ranking formula below for each damage
type. Public `calculate_ar` uses the same exact evaluator and projects its result
to `f32` only for display/API fields. The lower-level `calculate_ar_for_type`
helper is used by `estimate_ar`, whose float approximation exists only to schedule
work; it never ranks or prunes a candidate.

$$
AR_d(x)=\beta_d\left(1+\sum_i I_{i,d}\,\sigma_{i,d}\,q_i\,\gamma_d(x_i')\right).
$$

The coefficient $\sigma_{i,d}$ is the attack-element overwrite rate when present;
otherwise it is weapon scaling $s_i$ times the attack-element influence rate.
Ordinary records have no overwrite and an influence rate of 1. An explicit zero
overwrite suppresses scaling, while a positive overwrite can supply scaling even
when $s_i=0$. Only STR changes under two-handing: $x_{\mathrm{STR}}'=\tau(x_{\mathrm{STR}})$.

With $\kappa_{i,d}=\beta_dI_{i,d}\sigma_{i,d}q_i$,

$$
AR_d(x)=\beta_d+\sum_i\kappa_{i,d}\,\gamma_d(x_i'),
\qquad
AR_{\mathrm{total}}(x)=\sum_d AR_d(x).
$$

Both physical AR and total AR are therefore a constant plus a sum of single-stat
terms, with every coefficient fixed within the work unit. Two-handing does not
alter this: its floor operation is wholly inside the STR term. Base weapon AR and
ordinary skill hits use the weapon's attack-element record; an explicit per-hit
AoW override selects its own correction record. Routing, overwrite, and influence
coefficients remain fixed in either case.

The AR comparison fields include configured AoW attack-power buffs and the
request's world-damage multiplier: $\widetilde{AR}_d=\omega(AR_d+\delta_d)$.
These constants preserve separability. This is the configured loadout's AR, not a
simulation of AR after executing the full route.

### Guaranteed signs

Exact evaluation converts every loaded base, multiplier, motion value, scaling,
overwrite, influence, buff, and curve value through `rational` (`math/exact.rs`),
which rejects negative and non-finite values. A snapshot that violates this fails evaluation
instead of being ranked. Every $\beta_d$, $\kappa_{i,d}$, and $\gamma_d$ is
therefore nonnegative at runtime, which the upper bound in Section 4 and the
first-hit identity rely on.

### Non-additive corrections

An AoW row with its own correction record applies each routed stat's influence
rate $\iota$ to that stat's whole weighted contribution, including a constant
offset:

$$t_i=(\iota-1)+\iota\,s_i'\,q_i\,\gamma_d(x_i'),$$

where $s_i'$ is the overwrite rate or weapon scaling. If any $t_i$ is negative,
the most negative $t_i$ replaces the sum instead of adding positive and negative
terms. That minimum couples stats, so it is not separable. Compiled routes reject
it, and [Section 5](#5-compiled-routes-oracle-and-fallback) describes its
exhaustive path.

> **Separability obligation.** A nonlinear operation applied after contributions from
> multiple decision variables have been combined may break separability. Independent
> per-stat rounding remains separable. Any formula change that introduces a cross-stat
> term must either prove the recurrence still valid or replace it with exhaustive
> evaluation over the affected variables.

## 4. Generalized lexicographic dynamic program

All five objectives use `best_objective_allocation`. For a fixed work unit, the
implementation stores the numeric vector

$$
M_o(x)=(S_o(x),\widetilde{AR}_{\mathrm{total}}(x),AoW_{\mathrm{full}}(x),
AoW_{\mathrm{first}}(x),\mathrm{Bleed}(x)),
$$

ordered lexicographically. $S_o$ is buffed/scaled total AR, buffed/scaled physical
AR, bleed, first-hit damage, or full-sequence damage for the requested objective.
Repeated fields are harmless: deleting a later duplicate gives the equivalent
objective-specific key. Keeping this common representation matches the comparator.
Internal primary probes may retain only the one or two leading components needed by
that pass; final completion restores the full five-component key.

### Bleed and route contributions

Modeled bleed has the form $\mathrm{Bleed}(x)=\lambda_0+\lambda(x_{\mathrm{ARC}})$.
Profile-specific buildup, upgrade factors, and configured status buffs are
constants or ARC-only terms. Their local floors do not combine different decision
variables. The public bleed evaluator and the external AR/passive report use this
same exact production path: base weapon bleed is floored before Ash additions,
followed by the existing final floor when a scaling status addition is present.
Current profile corrections and external comparison notes are maintained in the
[model reference](../model-reference.md).

For routes admitted to the additive compiler, route $r$, hit $k$ has the scalar
damage formula

$$H_{r,k}(x)=H^0_{r,k}+\sum_i \phi_{r,k,i}(x_i).$$

Each damage component multiplies a fixed weapon-motion/fixed-attack base by
$1+\sum_i\kappa_{i,d}\gamma_d(x_i')$. Motion values, reinforcement factors,
override coefficients, and curve identities are fixed. Route order fixes buff
activation; active flat weapon buffs add constants, then world scaling multiplies
the result by a fixed positive factor. No modeled proc threshold or stat-dependent
route transition is part of this scalar expression. The non-additive correction in
Section 3 is outside this form.

First-hit means the first **positive-damage** hit, not necessarily row 1. With the
signs guaranteed in Section 3, each component is either identically zero or
strictly positive throughout the domain: its multiplier is at least 1. Thus the
first positive index $k_r^*$ is stat-independent, and

$$AoW_{\mathrm{first},r}=H_{r,k_r^*},\qquad
AoW_{\mathrm{full},r}=\sum_k H_{r,k}.$$

A route with no positive hit has both metrics zero. This first-hit identity must be
rechecked if formulas change; a sum of separable hits alone does not prove it. Both
metrics refer to the same route, optimized separately from other routes.

### Recurrence in exact arithmetic

For each active stat $i$ and addition $v\in[0,c_i]$, define
$\Delta_i(v)=M_o(m+v e_i)-M_o(m)$, where $e_i$ changes only stat $i$.
Separability makes these independent vector deltas.

Let $D_i(p)$ be the preferred partial allocation using the first $i$ active stats
and spending exactly $p$ under $K_o$; a state contains both stats and numeric
metrics. Active stats are processed in stat order, the same order the stat-vector
tie-break reads.

Initialization is

$$D_0(0)=(m,M_o(m)), \qquad D_0(p)=\bot\quad(p>0),$$

where $\bot$ is unreachable and is represented by `None`. For each legal addition,

$$
D_i(p)=\mathrm{best}_{0\le v\le\min(c_i,p)}
\left(D_{i-1}(p-v)\oplus\Delta_i(v)\right).
$$

The operator $\oplus$ adds the metric deltas and records the selected stat value.
The state retains the allocation itself; the stat vector is not treated as an
additive numeric score.

In exact arithmetic, one state per $(i,p)$ is sufficient. If partial allocation
$\alpha$ is preferred to $\beta$ at the same state, every common completion adds the
same metric vector. A lexicographic order is translation-invariant, and processing
stats in tie-break order means a later completion cannot reverse an earlier
stat-vector difference. Discarding $\beta$ therefore cannot remove the optimum.

After the last active stat, every reachable $D_{|A|}(p)$ for
$p\in[p_{\min},p_{\max}]$ receives its canonical inactive completion $h(R-p)$.
`better_objective_allocation` then compares those completed states. Filling first
matters because the full stat vector is the final tie-break.

No recurrence or terminal-selection step assumes monotonicity, concavity,
smoothness, or universal soft caps.

### Cost

If $A$ is empty, all numeric metrics are constant and the result is $m$ with
inactive values $h(R)$. Otherwise let $T=p_{\max}$ and
$c_{\max}=\max_{i\in A}\min(c_i,T)$. Precomputation uses
$1+\sum_{i\in A}(\min(c_i,T)+1)$ scalar evaluations per route. The recurrence costs

$$O(|A|(T+1)(c_{\max}+1))$$

with $O(T+1)$ working states and a final $O(T+1)$ spend scan, including
zero-budget cases. For positive budgets/capacities this is conventionally written
$O(|A|Tc_{\max})$, effectively linear in $T$ under the fixed in-game stat cap.
Optimizing $\rho$ legal routes multiplies the per-route work by $\rho$; the cost of
each scalar evaluation also depends on that route's hit count. These bounds count
transitions and stored states. Arbitrary-precision operations also depend on
integer bit lengths; coefficient construction and normalization are separate
preprocessing costs. The exhaustive path in Section 5 is combinatorial instead.

### Sharing primary work across Ash choices

For Max AR, Max Physical AR, and Bleed then AR, the shared prefix is
$Q_o(x)=(S_o(x),\widetilde{AR}_{\mathrm{total}}(x))$, the first two numeric key
components.

**Shared-plan obligation.** Participating choices use the same state graph:
bounds, budget, active stat identities/order, and feasible terminal spends. Their
prefix baseline and per-stat contributions represent identical functions throughout
that domain; equality at the baseline alone is insufficient. The implementation
groups identical `RelevantStatSearch` values before matching primary-effect
signatures, so choices with different active masks do not share a plan.

The shared plan retains **every** predecessor addition tied on those components
at each stat/spend state, without using route metrics or the combat-stat tie-break
to prune those transitions. A route-specific recurrence then evaluates only these
transitions and selects the complete lexicographic key, including skill damage,
bleed, and canonical combat stats. Selecting one primary-optimal allocation first
would be incorrect: secondary metrics can prefer a different tied allocation.

Each state also keeps its smallest combat-stat representative without dropping any
tied predecessor. After filling inactive stats, the smallest completed vector across
winning terminal spends can be reused when there is no skill route and the remaining
numeric components cannot distinguish primary ties. This holds for Bleed then AR,
and for AR objectives when bleed is stat-invariant. Other cases still evaluate the
retained transitions for their secondary metrics.

Under exact arithmetic and the separability obligations above, every full-key
optimum follows prefix-optimal transitions. Otherwise, replacing a nonoptimal
partial prefix at the same stat/spend state leaves the remaining choices unchanged
and adds the same remaining contributions, producing a strictly better final prefix.
That contradicts full-key optimality. Retaining all prefix-optimal transitions thus
lets the route recurrence select the preferred secondary metrics and stat vector.

The primary pass costs `O(|A| (T+1) (c_max+1))` once per effect signature/upgrade.
If $E_i(p)$ is the retained transition set at layer $i$, spend $p$, and
$E=\sum_{i,p}|E_i(p)|$, additional shared-plan storage is

$$O(E+|A|(T+1)),\qquad E\le |A|(T+1)(c_{\max}+1).$$

This includes the transition lists, per-state containers, and cached per-stat
contributions, in addition to rolling recurrence states. Subsequent route
recurrences traverse the retained transitions. If all primary values tie, the
original per-route bound still applies; sharing can add bookkeeping without
reducing that traversal.

### Unique retained allocations

The implementation skips route-specific DP when the best feasible primary rank
has exactly one terminal spend and one retained predecessor path. Ties at strictly
lower ranks do not matter. With a unique best prefix, secondary route metrics cannot
improve a losing prefix, so each route evaluates that allocation directly. Otherwise,
the route recurrence retains all primary-optimal transitions and selects the full
key.

Terminal ranks are computed from normalized integer keys. Each component's positive
scale preserves its order, so only the winning allocation needs direct evaluation.

### Bounds and scheduling

For AR objectives, an upper bound maximizes each stat's curve independently over
its allowed interval and ignores the joint spend constraint. Because Section 3
guarantees nonnegative coefficients, the sum is an upper bound even for
non-monotonic curves. Add the largest permitted Ash buff and apply the damage
multiplier using exact arithmetic. A work unit is skipped only when this bound is
strictly below the score of the worst retained result in a full top-K buffer; ties
are always evaluated. For weapon grouping, a full buffer contains K distinct output
groups, each represented by its preferred candidate. Worker-local buffers obey the
same rule, so scheduling affects pruning effectiveness but not the result order.

Within one weapon group, skills with strictly worse primary maxima cannot win
weapon grouping. All tied primary maxima remain eligible for route tie-breaks. A
floating-point estimate schedules promising weapons first; it never decides
whether to discard one.

For weapon-grouped output, a retained result also bounds later configurations of
that same weapon even before the global top-K fills. Only strictly lower primary
scores are discarded. Flat-buff dominance compares choices with identical non-buff
primary formulas over the same feasible domain, in the objective's primary-pair
order: physical AR before total AR for Max Physical AR, and bleed before total AR
for Bleed then AR. Total buff magnitude alone is not sufficient for Max Physical AR.
Equal pairs retain route and stat tie evaluation.

### Numerical contract

The `exact-v3` scoring contract treats each finite, validated loaded `f32` damage
coefficient as its exact binary rational. Products, sums, and percentage division
in damage ranking formulas are then evaluated exactly.

"Exact" is relative to those loaded values. It is not the game's own `f32`
arithmetic, and it does not recover precision lost while extracting or loading the
source data. Its value is determinism and sound pruning: no intermediate rounding
decides a close winner, and every comparison, bound, and merge sees the same
number. Ranked winners near display-rounded ties can therefore differ.

Status buildup is the exception, because gameplay floors make that difference
visible. Status scaling uses the shared `f32` evaluator before its gameplay floors:
weapon bleed is floored before Ash additions, followed by the existing final floor
when scaling status additions are present. The final status value is converted to
an exact binary rational for every optimizer comparison. These are one-dimensional
ARC lookups, so they do not break additivity across stats. For example, Heavy
Bloodfiend's Fork +25 at ARC50 evaluates `50 * (1 + 0.25 * 1.8 * 0.8)` to 68 in
this status sequence; the exact binary rationals of the same `f32` inputs give
67.99999979138374, which would floor to 67. No epsilon is used. DP accumulation,
pruning, route damage, and tie comparisons remain exact. Two-handing uses the
integer effective-STR rule.

Each fixed-loadout metric component is normalized to a common integer scale, with
common numerator factors removed. Before using `i128`, the solver bounds every
partial sum by the absolute baseline plus the sum of each stat's largest absolute
delta. If that bound does not fit, it uses `BigInt`. No coefficient is rounded to
make it fit. The same recurrence serves both representations.

Coefficient arithmetic likewise uses checked `i128` rationals and promotes to
arbitrary precision on overflow. Within a fixed-loadout DP, constant baselines and
the common positive world damage multiplier can be omitted: translation and
positive scaling preserve each component's ordering. Normalized integers are
compared directly only within their shared normalization domain. Cross-loadout,
cross-route, and pruning comparisons use complete exact metrics with scales and
baselines restored. Switching between `i128` and `BigInt` must preserve both
equality and order.

Exact keys survive terminal evaluation, route choice, top-K retention, parallel
merges, and final grouping. The chosen route identity is retained during
materialization. Public floating-point fields are projections for display, not
inputs to ranking. Public `calculate_ar` and `calculate_bleed_buildup` use the
exact production evaluators before projecting display values; `estimate_ar` is the
float scheduling approximation. The active snapshot and scoring identities are
maintained in the [model reference](../model-reference.md); cache and
persisted-result consumers apply them as specified by the
[runtime invariants](../architecture/runtime-invariants.md).

## 5. Compiled routes, oracle, and fallback

Route choice is a finite outer maximum over fixed-route problems, so it preserves
the exact-arithmetic argument. Implementation, progress accounting, and test
details belong in the overview.

Before scoring, canonical one-handed/two-handed route alternatives are restricted
to the weapon's resolved handling. This eligibility constraint is separate from
the Strength bonus: paired weapons can use a two-handed route without that bonus.
Unconditioned skills retain their own attack-specific scaling rules. This route
policy changes compiled result identity even for an older external snapshot.

A route with the non-additive correction from Section 3 cannot use the DP. When no
unique primary allocation is available, the optimizer enumerates every allocation
in $\mathcal X_w$ and compares each with the full key, so additive DP cannot prune
coupled stat contributions. That costs one full evaluation per allocation, and
$\mathcal X_w$ holds at most $\binom{R+4}{4}$ allocations: about 22.5 million per
route at $R=150$.

The existing exhaustive path shares the active set and cannot independently detect
an omitted dependency. Separate small-budget tests enumerate all five bounded stats
without its mask, distribution counter, or inactive-fill helper.

For `PerHitAttackPower` rows marked `is_supported` in the effect data, the scalar
route compiler declines compilation and the materialized evaluator reports that
the mechanic is unimplemented. The data flag alone does not establish evaluator
support. The compiler is not a general nonseparability detector. A future
cross-stat product, multi-stat clamp, damage-dependent transition, or proc
threshold requires a new proof or an explicitly supported exhaustive path. Routing
to exhaustive search does not itself implement an unsupported evaluator mechanic.

## 6. Result ordering

Inside a fixed-route DP, only $M_o$ and the combat vector vary. Across routes for
one loadout, the effective order is $M_o$, ascending combat stats, ascending route
priority, then ascending route ID. Priority and ID apply only after the numeric key
and stats tie.

Scored candidates use numeric metrics, ascending weapon ID, descending upgrade,
ascending skill ID, ascending combat stats, then internal indices. Materialized
rows retain a private exact key and use the same numeric, weapon, upgrade, skill,
and stat order. Serial and parallel merges share the scored comparator.
Determinism is distinct from equivalence to a canonical exhaustive optimum.

## 7. Scope of the claims

**Combinatorial exactness.** Under exact arithmetic, separability, fixed first-hit
identity, and active-set soundness, the recurrence returns the preferred allocation
over the declared integer domain. Exact integer accumulation removes the former
floating-point pruning limitation. Differential regressions test the
implementation; they do not establish gameplay fidelity or discharge the other
model assumptions for arbitrary future mechanics.

**Model fidelity.** The evaluator is a fan reconstruction tied to selected profile,
dataset, model, source, and manifest hashes. It does not model enemy defense,
negation, resistance growth, proc explosion damage, or universal temporary-buff
stacking. Profile coverage and correction notes are maintained in the
[model reference](../model-reference.md). Workbook-derived PvE stance/poise damage
and route stamina are reported, but are not objectives; enemy stagger thresholds,
recovery, and timing are not simulated.

**Data validity.** The shared runtime CSV parser rejects NaN and infinities. Runtime
loading also fails on missing curve entries, and offline validation requires every
used curve value over its reachable effective-stat domain. Exact evaluation rejects
negative values (Section 3), and shipped-data regressions check that modeled bases,
curves, coefficients, and buffs used by the first-hit argument are finite and
nonnegative. Monotonicity is checked separately as a data-quality invariant, but
the recurrence does not assume it.

## 8. Fixed-loadout AR / bleed frontier

Fix the weapon, affinity, skill, upgrade, budget, floors/locks, non-combat stats,
and handling/world settings. In the modeled status formula, only ARC can change
bleed within that context. For each feasible ARC value, maximize AR over the
remaining integer allocations, keeping the complete numeric/stat tie order.
Any allocation with lower AR at the same ARC is dominated or has the same metric
pair as the retained representative. Exact comparison of those representatives
therefore gives the complete nondominated AR/bleed pairs for this domain. This is
not a frontier across weapons, affinities, or skill choices.

Equal pairs retain one canonical allocation. A point is removed only if another
has at least as much AR and bleed, with a strict improvement in one. This argument
does not require monotone curves or interpolation between integer allocations.

For a sacrifice control in hundredths of a percent, an option is eligible at
integer limit `b` exactly when `10_000 * (maximum_ar - ar) <= b * maximum_ar`.
Each point carries the ceiling of that exact ratio; the desktop compares integers
instead of rounded displayed AR. Zero maximum AR has zero loss. The loss and
bleed-gain explanation is also subtracted exactly before display projection.
Neither dominance nor a sacrifice limit establishes proc count, DPS, or a player's
preferred tradeoff.
