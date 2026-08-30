# Optimizer Mathematics

This document states the searched domain and an exact-arithmetic argument for the
Rust optimizer's recurrence. It is not an unconditional exactness proof for its
floating-point implementation. The companion
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
| $A$ | stats relevant to any numeric comparison component |

`compute_free_points` (`math.rs`) computes

$$P = B_c + (L-L_c) - \sum_{j=1}^{8} a_j.$$

The request is rejected if a current stat is below its class minimum or $P<0$.
`build_combat_constraints` (`optimizer.rs`) applies requested floors and locks:

$$m_i=\max(a_i,f_i), \qquad u_i=99,$$

where $f_i$ is the requested floor, with $m_i=u_i$ for a locked stat. Mandatory raises
consume budget, so initially

$$R=P-\sum_i(m_i-a_i).$$

`RelevantStatSearch::new` raises the minima again when needed to satisfy the selected
weapon's requirements and deducts those raises from $R$. The request or weapon is
rejected if a mandatory raise exceeds the budget or if $R>\sum_i c_i$. All later
capacities $c_i=u_i-m_i$ use the final bounds after floors, locks, and requirements.

Resolved handling determines whether the weapon receives the Strength bonus. When
it does, effective Strength is

$$e_{\mathrm{STR}}(s)=\left\lfloor\frac{3s}{2}\right\rfloor.$$

Forced bow-family handling and paired/no-bonus exceptions are resolved before
requirements and curve lookup; individual AoW rows can also disable the bonus.
These decisions are fixed for the work unit and remain functions of STR alone.
Generated and validated calc-correct curves cover every reachable effective value
through 148.

## 2. Feasible spend domain

For a fixed weapon, affinity, upgrade, Ash of War, route, and objective, let $M_o(x)$
be the numeric comparison vector in Section 4. The full key is
$K_o(x)=(M_o(x),-x)$, with ascending combat stats breaking numeric ties.

The active-set soundness obligation is: for every numeric component $r$, every
inactive stat $i$, and every legal value $s$,

$$i\notin A\ \Longrightarrow\ g_{r,i}(s)=g_{r,i}(m_i),\qquad m_i\le s\le u_i,$$

where $g_{r,i}$ is that stat's contribution. Over-including stats is safe;
omitting a changing contribution is not. `active_stats_for_choice` conservatively
combines weapon AR, bleed, and AoW-row dependencies. Its implementation must satisfy
this obligation independently of the DP. Let

$$C_A=\sum_{i\in A}c_i, \qquad
C_I=\sum_{i\notin A}c_i.$$

If $p$ points are spent on active stats, the remaining $R-p$ must fit in inactive
stats. Therefore the complete feasible active-spend interval is

$$p_{\min}=\max(0,R-C_I), \qquad
p_{\max}=\min(R,C_A).$$

The total-capacity check guarantees $p_{\min}\le p_{\max}$, so at least one feasible
final state exists. Termination follows separately from the finite stat and budget
loops.

For each integer $0\le q\le C_I$, $h(q)$ contains the lexicographically smallest
final inactive stat values with $m_i\le h_i(q)\le u_i$ and
$\sum_{i\notin A}(h_i(q)-m_i)=q$. These are final values, not increments.
`fill_inactive_stats` computes $h$. Under active-set soundness, inactive stats affect
only the stat-vector tie-break, so this completion loses no preferred result.

The searched region is the union

$$
\mathcal X_w=
\bigcup_{p=p_{\min}}^{p_{\max}}
\{
x\in\mathbb Z^5:
m_i\le x_i\le u_i,\
\sum_{i\in A}(x_i-m_i)=p,\
x_i=h_i(R-p)\text{ for }i\notin A
\}.
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
changes under two-handing: $x_{\mathrm{STR}}'=e_{\mathrm{STR}}(x_{\mathrm{STR}})$.

With $\beta_d=b_dr_d$,

$$
AR_d(x)=\beta_d+
\sum_i\beta_d I_{i,d}s_iq_i\gamma_d(x_i').
$$

Thus both physical AR and

$$AR_{\mathrm{total}}(x)=\sum_d AR_d(x)$$

are a constant plus a sum of single-stat terms. Two-handing does not alter this: its
floor operation is wholly inside the STR term. Equivalently,
$AR_d=\beta_d+\sum_i\kappa_{i,d}\gamma_d(x_i')$, where every coefficient is fixed
within the work unit. Base weapon AR uses Boolean routing flags; AoW overrides use
their own fixed correction coefficients.

The AR comparison fields include configured AoW attack-power buffs and the request's
world-damage multiplier: $\widetilde{AR}_d=a(AR_d+b_d^{\mathrm{buff}})$.
These constants preserve separability. This is the configured loadout's AR, not a
simulation of AR after executing the full route.

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
AoW_{\mathrm{first}}(x),Bleed(x)),
$$

ordered lexicographically. $S_o$ is buffed/scaled total AR, buffed/scaled physical AR,
bleed, first-hit damage, or full-sequence damage for the requested objective.
Repeated fields are harmless: deleting a later duplicate gives the equivalent
objective-specific key. Keeping this common representation matches the comparator.

### Bleed and route contributions

Modeled bleed has the form $B(x)=C_B+b_{\mathrm{ARC}}(x_{\mathrm{ARC}})$.
Profile-specific buildup, upgrade factors, and configured status buffs are constants
or ARC-only terms. Their local floors do not combine different decision variables.

For route $r$, hit $k$, the scalar damage formula is

$$H_{r,k}(x)=C_{r,k}+\sum_i h_{r,k,i}(x_i).$$

Each damage component multiplies a fixed weapon-motion/fixed-attack base by
$1+\sum_i\kappa_i\gamma_i(x_i')$. Motion values, reinforcement factors,
override coefficients, and curve identities are fixed. Route order fixes buff
activation; active flat weapon buffs add constants, then world scaling multiplies
the result by a fixed positive factor. No modeled proc threshold or stat-dependent
route transition is part of this scalar expression.

First-hit means the first **positive-damage** hit, not necessarily row 1. For the
shipped nonnegative bases, curves, coefficients, and buffs, each component is either
identically zero or strictly positive throughout the domain: its multiplier is at
least 1. Thus the first positive index $k_r^*$ is stat-independent, and

$$AoW_{\mathrm{first},r}=H_{r,k_r^*},\qquad
AoW_{\mathrm{full},r}=\sum_k H_{r,k}.$$

A route with no positive hit has both metrics zero. This first-hit identity must be
rechecked if data signs or formulas change; a sum of separable hits alone does not
prove it. Both metrics refer to the same route, optimized separately from other routes.

### Recurrence in exact arithmetic

For each active stat $i$ and addition $v\in[0,c_i]$, define
$\Delta_i(v)=M_o(m+v e_i)-M_o(m)$, where $e_i$ changes only stat $i$.
Separability makes these independent vector deltas.

Let $D_i(p)$ be the preferred partial allocation using the first $i$ active stats and
spending exactly $p$ under $K_o$; a state contains both stats and numeric metrics.

Initialization is

$$D_0(0)=(m,M_o(m)), \qquad D_0(p)=\bot\quad(p>0),$$

where $\bot$ is unreachable and is represented by `None`. For each legal addition,

$$
D_i(p)=\mathrm{best}_{0\le v\le\min(c_i,p)}
\left(D_{i-1}(p-v)\oplus\Delta_i(v)\right).
$$

The operator $\oplus$ adds the metric deltas and records the selected stat value. The
state retains the allocation itself; the stat vector is not treated as an additive
numeric score.

In exact arithmetic, one state per $(i,p)$ is sufficient. If partial allocation $\alpha$ is preferred to
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

If $A$ is empty, all numeric metrics are constant and the result is $m$ with
inactive values $h(R)$. Otherwise let $T=p_{\max}$ and
$c_{\max}=\max_{i\in A}\min(c_i,T)$. Precomputation uses
$1+\sum_{i\in A}(\min(c_i,T)+1)$ scalar evaluations per route. The recurrence costs

$$O(|A|(T+1)(c_{\max}+1))$$

with $O(T+1)$ working states and a final $O(T+1)$ spend scan, including zero-budget
cases. For positive budgets/capacities this is conventionally written
$O(|A|Tc_{\max})$, effectively linear in $T$ under the fixed in-game stat cap.
Optimizing $\rho$ legal routes multiplies the per-route work by $\rho$; the cost of
each scalar evaluation also depends on that route's hit count.

### Floating-point boundary

The implementation accumulates `f32` deltas. Rounded subtraction/addition is not
the exact arithmetic above: reassociation can change comparisons, and adding a
common completion can collapse a strict numeric difference into a tie with a
different preferred stat vector. A preferred allocation can be discarded permanently.

Every retained terminal is re-evaluated directly before terminal comparison.
This gives correct direct metrics and ordering **among retained terminals**, not a
guarantee that the canonical exhaustive winner survived. Accumulated DP totals are
not reported metrics. A shipped Convergence fixture demonstrates equal numeric
metrics but a different stat-vector winner; evidence and the numerical decision
are recorded in the overview. `f64` alone would reduce rounding without proving
the pruning step exact.

## 5. Compiled routes, oracle, and fallback

Route choice is a finite outer maximum over fixed-route problems, so it preserves
the exact-arithmetic argument. Implementation, progress accounting, and test details
belong in the overview.

The existing exhaustive path shares the active set and cannot independently detect
an omitted dependency. Separate small-budget tests enumerate all five bounded stats
without its mask, distribution counter, or inactive-fill helper.

Compilation currently declines supported `PerHitAttackPower` effects; it is not a
general nonseparability detector. A future cross-stat product, multi-stat clamp,
damage-dependent transition, or proc threshold requires a new proof or an explicitly
supported exhaustive path. Routing to exhaustive search does not itself implement
an unsupported evaluator mechanic.

## 6. Result ordering

Inside a fixed-route DP, only $M_o$ and the combat vector vary. Scored candidates use
numeric metrics, ascending weapon ID, descending upgrade, ascending skill ID, then
ascending combat stats and internal indices. Route metric ties use priority and ID
for stable display. Public result ordering omits the combat-stat tie-break. The
serial/parallel merge shares the scored-candidate comparator; determinism is distinct
from equivalence to a canonical exhaustive optimum.

## 7. Scope of the claims

**Combinatorial exactness.** Under exact arithmetic, separability, fixed first-hit
identity, and active-set soundness, the recurrence returns the preferred allocation
over the declared integer domain. The `f32` implementation has the pruning limitation
above. Differential regressions are evidence, not a proof covering all inputs.

**Model fidelity.** The evaluator is a fan reconstruction tied to selected profile,
dataset, model, source, and manifest hashes. It does not model enemy defense,
negation, resistance growth, proc explosion damage, or universal temporary-buff
stacking. Workbook-derived PvE stance/poise damage and route stamina are reported,
but are not objectives; enemy stagger thresholds, recovery, and timing are not simulated.

**Data validity.** Runtime loading fails on missing curve entries, and offline
validation requires every used curve value through effective stat 148. Monotonicity is
also checked as a data-quality invariant, but the recurrence does not assume it.
