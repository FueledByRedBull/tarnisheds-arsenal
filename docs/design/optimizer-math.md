# Optimizer Mathematics

This document states the attack-rating model, searched domain, and exactness argument
implemented by the Rust optimizer. The companion
[`optimizer-overview.md`](optimizer-overview.md) covers implementation structure,
ranking, parallel work, tests, and release engineering. Functions named here live in
`core/er_optimizer_core/src/`.

The equations are a fan-made model reconstructed from version-bound regulation data.
They are not an official FromSoftware specification. The model boundary is summarized
in [Section 7](#7-scope-of-the-claims).

## 1. Notation and point budget

The optimizer distributes points among the five offensive scaling attributes STR,
DEX, INT, FAI, and ARC. Let:

| Symbol | Meaning |
|---|---|
| $L$ | requested character level |
| $B_c, L_c$ | starting class stat total and starting level |
| $a_j$ | current value of character stat $j$, over all eight stats |
| $P$ | free points before floors and weapon requirements |
| $m_i, u_i$ | lower and upper bounds for offensive stat $i$ |
| $c_i=u_i-m_i$ | remaining capacity of offensive stat $i$ |
| $R$ | points left after mandatory offensive-stat raises |
| $A$ | stats relevant to the objective and its earlier tie-breaks |

`compute_free_points` (`math.rs`) computes

$$P = B_c + (L-L_c) - \sum_{j=1}^{8} a_j.$$

The request is rejected if a current stat is below its class minimum or $P<0$.
`build_combat_constraints` (`optimizer.rs`) applies requested floors and locks:

$$m_i=\max(a_i,\operatorname{floor}_i), \qquad u_i=99,$$

with $m_i=u_i$ for a locked stat. Mandatory raises consume budget, so initially

$$R=P-\sum_i(m_i-a_i).$$

`RelevantStatSearch::new` raises the minima again when needed to satisfy the selected
weapon's requirements and deducts those raises from $R$. The request or weapon is
rejected if a mandatory raise exceeds the budget or if $R>\sum_i c_i$.

When two-handing is legal, effective Strength is

$$\operatorname{effSTR}(s)=\left\lfloor\frac{3s}{2}\right\rfloor.$$

This affects requirement checks and curve lookup but remains a function of STR alone.
Generated and validated calc-correct curves cover every reachable effective value
through 148.

## 2. Feasible spend domain

For one weapon, Ash of War, and objective, `active_stats_for_choice` selects the set
$A$ of stats that can change the primary metric or an earlier ranking tie-break. Let

$$C_A=\sum_{i\in A}c_i, \qquad
C_I=\sum_{i\notin A}c_i.$$

If $p$ points are spent on active stats, the remaining $R-p$ must fit in inactive
stats. Therefore the complete feasible active-spend interval is

$$p_{\min}=\max(0,R-C_I), \qquad
p_{\max}=\min(R,C_A).$$

The total-capacity check guarantees $p_{\min}\le p_{\max}$, so at least one feasible
final state exists. Termination follows separately from the finite stat and budget
loops.

For every $q\le C_I$, let $h(q)$ be the lexicographically smallest inactive-stat
completion that spends exactly $q$. `fill_inactive_stats` computes $h$. Inactive stats
cannot change the primary metric or any earlier tie-break, so retaining only this one
completion loses no preferred result and avoids enumerating equivalent allocations.

The searched region is the union

$$
\mathcal X_w=
\bigcup_{p=p_{\min}}^{p_{\max}}
\left\{
x\in\mathbb Z^5:
m_i\le x_i\le u_i,\
\sum_{i\in A}(x_i-m_i)=p,\
x_i=h_i(R-p)\text{ for }i\notin A
\right\}.
$$

Searching only $p_{\max}$ would require every active contribution to be monotone.
Searching the full interval does not: an interior spend remains eligible even when a
curve decreases.

## 3. Attack rating and separability

For damage type $d$ in physical, magic, fire, lightning, and holy,
`calculate_ar_for_type` and `calculate_ar` (`math.rs`) compute

$$
AR_d(x)=b_d r_d\left(1+\sum_i I_{i,d}s_iq_i\gamma_d(x_i')\right),
$$

where $b_d$ is base damage, $r_d$ the reinforcement damage multiplier, $I_{i,d}$
the attack-element routing flag, $s_i$ weapon scaling, $q_i$ reinforcement scaling,
$\gamma_d$ the selected calc-correct curve, and $x_i'$ the effective stat. Only STR
changes under two-handing: $x_{\mathrm{STR}}'=\operatorname{effSTR}(x_{\mathrm{STR}})$.

With $\beta_d=b_dr_d$,

$$
AR_d(x)=\beta_d+
\sum_i\beta_d I_{i,d}s_iq_i\gamma_d(x_i').
$$

Thus both physical AR and

$$AR_{\mathrm{total}}(x)=\sum_d AR_d(x)$$

are a constant plus a sum of single-stat terms. Two-handing does not alter this: its
floor operation is wholly inside the STR term.

> **Separability obligation.** A nonlinear operation applied after contributions from
> multiple decision variables have been combined may break separability. Independent
> per-stat rounding remains separable. Any formula change that introduces a cross-stat
> term must either prove the recurrence still valid or replace it with exhaustive
> evaluation over the affected variables.

## 4. Generalized lexicographic dynamic program

All five objectives use `best_objective_allocation`. For a fixed weapon, upgrade,
Ash of War, and legal AoW route, the retained key is

$$
K(x)=(S(x),AR_{\mathrm{total}}(x),AoW_{\mathrm{full}}(x),
AoW_{\mathrm{first}}(x),Bleed(x),-x),
$$

ordered lexicographically. $S$ is total AR, physical AR, bleed, first-hit damage, or
full-sequence damage according to the requested objective. The final $-x$ notation
means that the lexicographically smaller combat-stat vector wins an otherwise exact
tie.

Weapon AR, modeled bleed, and every scalar AoW hit are a constant plus independent
single-stat contributions. A complete route is the sum of its hits, so first-hit and
full-sequence route values are separable too. For each active stat $i$ and addition
$v\in[0,c_i]$, the optimizer evaluates the independent vector delta $\Delta_i(v)$
for every numeric component of $K$.

Let $D_i(p)$ be the preferred partial allocation using the first $i$ active stats and
spending exactly $p$ under $K$.

Initialization is

$$D_0(0)=m, \qquad D_0(p)=\bot\quad(p>0),$$

where $\bot$ is unreachable and is represented by `None`. For each legal addition,

$$
D_i(p)=\operatorname{best}_{0\le v\le\min(c_i,p)}
\left(D_{i-1}(p-v)\oplus\Delta_i(v)\right).
$$

The operator $\oplus$ adds the metric deltas and records the selected stat value. The
state retains the allocation itself; the stat vector is not treated as an additive
numeric score.

One state per $(i,p)$ is sufficient. If partial allocation $\alpha$ is preferred to
$\beta$ at the same state, every common completion adds the same metric vector. A
lexicographic order is translation-invariant, and fixed stat processing means a later
completion cannot reverse an earlier stat-vector difference. Discarding $\beta$
therefore cannot remove the optimum.

After the last active stat, every reachable $D_{|A|}(p)$ for
$p\in[p_{\min},p_{\max}]$ receives its canonical inactive completion $h(R-p)$.
`better_objective_allocation` then compares those completed states. Filling first matters
because the full stat vector is the final tie-break.

No recurrence or terminal-selection step assumes monotonicity, concavity, smoothness,
or universal soft caps.

### Cost

Let $T=p_{\max}$ and $c_{\max}=\max_i\min(c_i,T)$. Precomputation uses
$\sum_i(\min(c_i,T)+1)$ compact scalar evaluations per legal route. The recurrence costs

$$O(|A|Tc_{\max})$$

with $O(T)$ working states; the final spend scan is $O(T)$. Since the in-game stat cap
fixes $c_{\max}\le99$, the recurrence is effectively linear in $T$ within this domain.

### Floating-point boundary

The DP accumulates `f32` deltas, whose addition order is not bit-identical to direct
evaluation. These values select an allocation only. Every retained terminal stat
vector is re-evaluated canonically before terminal comparison, ranking, or display.
Accumulated DP totals never become reported metrics.

## 5. Compiled routes, oracle, and fallback

`prepare_scalar_aow_routes` compiles route membership, hit order, and buff activation
once per loadout. During DP, `evaluate_scalar_aow_route` calculates only first-hit and
full-sequence numbers. Display actions, hit objects, effects, warnings, strings, and
maps are materialized once for retained results.

Each legal route is optimized separately; route winners are compared under the same
key $K$. This is exact because route choice is a finite outer maximum over independent
DP problems. Equal route metrics use route priority and ID for stable display.

`search_work_unit_exhaustive` remains the differential-test oracle and the explicit
fallback if a future supported mechanic introduces a non-separable per-hit term. Its
logical candidate count is

$$
N=\sum_{p=p_{\min}}^{p_{\max}}
[z^p]\prod_{i\in A}(1+z+\cdots+z^{c_i}).
$$

Progress reports this logical domain as candidates covered, even though DP does not
visit each allocation individually.

## 6. Result ordering

Every legal weapon, affinity, Ash of War, upgrade, and retained stat allocation is
reduced under the objective-specific comparison. Final ordering also includes stable
loadout identifiers and the completed combat-stat vector, making serial and parallel
execution deterministic under the same model and floating-point boundary.

## 7. Scope of the claims

**Search exactness.** The generalized DP is exact over the declared integer domain
under separability and its stated `f32` selection boundary. The exhaustive oracle
visits the same domain and guards every objective family in differential tests.

**Model fidelity.** The evaluator is a fan reconstruction tied to selected profile,
dataset, model, source, and manifest hashes. It does not model enemy defense,
negation, resistance growth, proc explosion damage, poise, stance damage, or universal
temporary-buff stacking. Route stamina is reported but is not an objective.

**Data validity.** Runtime loading fails on missing curve entries, and offline
validation requires every used curve value through effective stat 148. Monotonicity is
also checked as a data-quality invariant, but search exactness does not depend on it.
