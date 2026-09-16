use std::cmp::Ordering;
use std::ops::Add;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::{BigRational, Ratio};
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::math::exact_value::ExactRational;

const COMPONENT_COUNT: usize = 5;

type Combat = [u8; COMPONENT_COUNT];
type Key<I, const N: usize> = [I; N];
type Additions = [Vec<Vec<u8>>; COMPONENT_COUNT];
type BigKey<const N: usize> = Key<BigInt, N>;
type BigDeltas<const N: usize> = [Vec<BigKey<N>>; COMPONENT_COUNT];
type SmallKey<const N: usize> = Key<i128, N>;
type SmallDeltas<const N: usize> = [Vec<SmallKey<N>>; COMPONENT_COUNT];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DpResult {
    pub combat: Vec<Option<Combat>>,
    pub additions: Additions,
    pub ranks: Vec<Option<usize>>,
}

#[allow(clippy::too_many_arguments)]
pub fn solve<const N: usize>(
    base: &[BigRational; N],
    deltas: &[Vec<[BigRational; N]>; COMPONENT_COUNT],
    mins: Combat,
    active: [bool; COMPONENT_COUNT],
    budget: usize,
    primary_only: bool,
    allowed: Option<&Additions>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<DpResult, String> {
    let slots = budget
        .checked_add(1)
        .ok_or_else(|| "exact DP budget is too large".to_string())?;
    check_continue(should_continue)?;
    let normalized = normalize(base, deltas, active, slots, should_continue)?;
    if normalized.fits_i128 {
        let (base, deltas) = to_i128(&normalized)?;
        run_dp(
            base,
            deltas,
            mins,
            active,
            budget,
            primary_only,
            allowed,
            should_continue,
        )
    } else {
        run_dp(
            normalized.base,
            normalized.deltas,
            mins,
            active,
            budget,
            primary_only,
            allowed,
            should_continue,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn solve_exact<const N: usize>(
    base: &[ExactRational; N],
    deltas: &[Vec<[ExactRational; N]>; COMPONENT_COUNT],
    mins: Combat,
    active: [bool; COMPONENT_COUNT],
    budget: usize,
    primary_only: bool,
    allowed: Option<&Additions>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<DpResult, String> {
    let slots = budget
        .checked_add(1)
        .ok_or_else(|| "exact DP budget is too large".to_string())?;
    check_continue(should_continue)?;
    let Some((base, deltas)) = normalize_small(base, deltas, active, slots, should_continue)?
    else {
        let base = std::array::from_fn(|component| base[component].to_big());
        let mut big_deltas: [Vec<[BigRational; N]>; COMPONENT_COUNT] =
            std::array::from_fn(|_| Vec::new());
        for (stat, values) in deltas.iter().enumerate() {
            for delta in values {
                check_continue(should_continue)?;
                big_deltas[stat].push(std::array::from_fn(|component| delta[component].to_big()));
            }
        }
        return solve(
            &base,
            &big_deltas,
            mins,
            active,
            budget,
            primary_only,
            allowed,
            should_continue,
        );
    };
    run_dp(
        base,
        deltas,
        mins,
        active,
        budget,
        primary_only,
        allowed,
        should_continue,
    )
}

struct Normalized<const N: usize> {
    base: BigKey<N>,
    deltas: BigDeltas<N>,
    fits_i128: bool,
}

fn check_continue<F: FnMut() -> bool + ?Sized>(should_continue: &mut F) -> Result<(), String> {
    if should_continue() {
        Ok(())
    } else {
        Err("cancelled".to_string())
    }
}

fn normalize<const N: usize>(
    base: &[BigRational; N],
    deltas: &[Vec<[BigRational; N]>; COMPONENT_COUNT],
    active: [bool; COMPONENT_COUNT],
    slots: usize,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Normalized<N>, String> {
    let mut normalized_deltas: BigDeltas<N> = std::array::from_fn(|_| Vec::new());
    for stat in 0..COMPONENT_COUNT {
        if !active[stat] {
            continue;
        }
        check_continue(should_continue)?;
        if deltas[stat].is_empty() {
            return Err(format!("exact DP stat {stat} is missing its zero delta"));
        }
        let length = deltas[stat].len().min(slots);
        normalized_deltas[stat] = vec![std::array::from_fn(|_| BigInt::zero()); length];
    }

    let mut normalized_base: BigKey<N> = std::array::from_fn(|_| BigInt::zero());
    for component in 0..N {
        check_continue(should_continue)?;
        if base[component].is_zero()
            && deltas.iter().enumerate().all(|(stat, values)| {
                !active[stat]
                    || values
                        .iter()
                        .take(slots)
                        .all(|value| value[component].is_zero())
            })
        {
            continue;
        }
        if let Some(previous) = (0..component).find(|&previous| {
            base[component] == base[previous]
                && deltas.iter().enumerate().all(|(stat, values)| {
                    !active[stat]
                        || values
                            .iter()
                            .take(slots)
                            .all(|value| value[component] == value[previous])
                })
        }) {
            normalized_base[component] = normalized_base[previous].clone();
            for stat_values in &mut normalized_deltas {
                for value in stat_values {
                    value[component] = value[previous].clone();
                }
            }
            continue;
        }
        let mut denominator_exponent = Some(0usize);
        let mut denominator = None;
        collect_denominator(
            &mut denominator_exponent,
            &mut denominator,
            base[component].denom(),
        );
        for (stat, stat_deltas) in deltas.iter().enumerate() {
            if !active[stat] {
                continue;
            }
            for delta in stat_deltas.iter().take(slots) {
                check_continue(should_continue)?;
                collect_denominator(
                    &mut denominator_exponent,
                    &mut denominator,
                    delta[component].denom(),
                );
            }
        }

        let denominator = denominator.as_ref();

        let mut values = Vec::with_capacity(
            1 + (0..COMPONENT_COUNT)
                .filter(|&stat| active[stat])
                .map(|stat| normalized_deltas[stat].len())
                .sum::<usize>(),
        );
        values.push(match denominator_exponent {
            Some(exponent) => scale_rational_dyadic(&base[component], exponent),
            None => scale_rational(&base[component], denominator.expect("generic denominator")),
        });
        for (stat, stat_deltas) in deltas.iter().enumerate() {
            if !active[stat] {
                continue;
            }
            for delta in stat_deltas.iter().take(slots) {
                check_continue(should_continue)?;
                values.push(match denominator_exponent {
                    Some(exponent) => scale_rational_dyadic(&delta[component], exponent),
                    None => {
                        scale_rational(&delta[component], denominator.expect("generic denominator"))
                    }
                });
            }
        }

        let mut divisor = BigInt::zero();
        for value in &values {
            check_continue(should_continue)?;
            divisor = divisor.gcd(value);
            if divisor.is_one() {
                break;
            }
        }
        if !divisor.is_zero() && divisor != BigInt::from(1u8) {
            for value in &mut values {
                check_continue(should_continue)?;
                *value /= &divisor;
            }
        }

        let mut value_index = 0;
        normalized_base[component] = values[value_index].clone();
        value_index += 1;
        for stat in 0..COMPONENT_COUNT {
            if !active[stat] {
                continue;
            }
            for add in 0..normalized_deltas[stat].len() {
                check_continue(should_continue)?;
                normalized_deltas[stat][add][component] = values[value_index].clone();
                value_index += 1;
            }
        }
    }

    let max_i128 = BigInt::from(i128::MAX);
    let mut fits_i128 = true;
    for component in 0..N {
        check_continue(should_continue)?;
        let mut bound = normalized_base[component].abs();
        for (stat, stat_deltas) in normalized_deltas.iter().enumerate() {
            if !active[stat] {
                continue;
            }
            check_continue(should_continue)?;
            let mut max_delta = BigInt::zero();
            for delta in stat_deltas {
                check_continue(should_continue)?;
                let magnitude = delta[component].abs();
                if magnitude > max_delta {
                    max_delta = magnitude;
                }
            }
            bound += max_delta;
        }
        if bound > max_i128 {
            fits_i128 = false;
        }
    }

    Ok(Normalized {
        base: normalized_base,
        deltas: normalized_deltas,
        fits_i128,
    })
}

fn normalize_small<const N: usize>(
    base: &[ExactRational; N],
    deltas: &[Vec<[ExactRational; N]>; COMPONENT_COUNT],
    active: [bool; COMPONENT_COUNT],
    slots: usize,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<(SmallKey<N>, SmallDeltas<N>)>, String> {
    let mut normalized_deltas: SmallDeltas<N> = std::array::from_fn(|_| Vec::new());
    for stat in 0..COMPONENT_COUNT {
        if !active[stat] {
            continue;
        }
        check_continue(should_continue)?;
        if deltas[stat].is_empty() {
            return Err(format!("exact DP stat {stat} is missing its zero delta"));
        }
        let length = deltas[stat].len().min(slots);
        normalized_deltas[stat] = vec![[0i128; N]; length];
    }

    for component in 0..N {
        check_continue(should_continue)?;
        if base[component].small().is_none() {
            return Ok(None);
        }
        for (stat, stat_deltas) in deltas.iter().enumerate() {
            if !active[stat] {
                continue;
            }
            for delta in stat_deltas.iter().take(slots) {
                check_continue(should_continue)?;
                if delta[component].small().is_none() {
                    return Ok(None);
                }
            }
        }
    }

    let mut normalized_base = [0i128; N];
    for component in 0..N {
        check_continue(should_continue)?;
        let base_value = base[component]
            .small()
            .expect("small normalization checked base values");
        if base_value.is_zero()
            && deltas.iter().enumerate().all(|(stat, values)| {
                !active[stat]
                    || values.iter().take(slots).all(|value| {
                        value[component]
                            .small()
                            .is_some_and(|value| value.is_zero())
                    })
            })
        {
            continue;
        }
        if let Some(previous) = (0..component).find(|&previous| {
            base[component].small() == base[previous].small()
                && deltas.iter().enumerate().all(|(stat, values)| {
                    !active[stat]
                        || values
                            .iter()
                            .take(slots)
                            .all(|value| value[component].small() == value[previous].small())
                })
        }) {
            normalized_base[component] = normalized_base[previous];
            for stat_values in &mut normalized_deltas {
                for value in stat_values {
                    value[component] = value[previous];
                }
            }
            continue;
        }

        let mut denominator = 1i128;
        let mut add_denominator = |value: &Ratio<i128>| -> Option<()> {
            denominator = checked_lcm_i128(denominator, *value.denom())?;
            Some(())
        };
        if add_denominator(base_value).is_none() {
            return Ok(None);
        }
        for (stat, stat_deltas) in deltas.iter().enumerate() {
            if !active[stat] {
                continue;
            }
            for delta in stat_deltas.iter().take(slots) {
                check_continue(should_continue)?;
                let value = delta[component]
                    .small()
                    .expect("small normalization checked delta values");
                if add_denominator(value).is_none() {
                    return Ok(None);
                }
            }
        }

        let mut values = Vec::with_capacity(
            1 + (0..COMPONENT_COUNT)
                .filter(|&stat| active[stat])
                .map(|stat| normalized_deltas[stat].len())
                .sum::<usize>(),
        );
        let Some(value) = checked_scale_ratio(base_value, denominator) else {
            return Ok(None);
        };
        values.push(value);
        for (stat, stat_deltas) in deltas.iter().enumerate() {
            if !active[stat] {
                continue;
            }
            for delta in stat_deltas.iter().take(slots) {
                check_continue(should_continue)?;
                let value = delta[component]
                    .small()
                    .expect("small normalization checked delta values");
                let Some(value) = checked_scale_ratio(value, denominator) else {
                    return Ok(None);
                };
                values.push(value);
            }
        }

        let mut divisor = 0i128;
        for value in &values {
            check_continue(should_continue)?;
            let Some(magnitude) = value.checked_abs() else {
                return Ok(None);
            };
            divisor = divisor.gcd(&magnitude);
            if divisor == 1 {
                break;
            }
        }
        if divisor != 0 && divisor != 1 {
            for value in &mut values {
                check_continue(should_continue)?;
                *value /= divisor;
            }
        }

        let mut value_index = 0;
        normalized_base[component] = values[value_index];
        value_index += 1;
        for stat in 0..COMPONENT_COUNT {
            if !active[stat] {
                continue;
            }
            for add in 0..normalized_deltas[stat].len() {
                check_continue(should_continue)?;
                normalized_deltas[stat][add][component] = values[value_index];
                value_index += 1;
            }
        }
    }

    for component in 0..N {
        check_continue(should_continue)?;
        let Some(mut bound) = normalized_base[component].checked_abs() else {
            return Ok(None);
        };
        for (stat, stat_deltas) in normalized_deltas.iter().enumerate() {
            if !active[stat] {
                continue;
            }
            check_continue(should_continue)?;
            let mut max_delta = 0i128;
            for delta in stat_deltas {
                check_continue(should_continue)?;
                let Some(magnitude) = delta[component].checked_abs() else {
                    return Ok(None);
                };
                max_delta = max_delta.max(magnitude);
            }
            let Some(next_bound) = bound.checked_add(max_delta) else {
                return Ok(None);
            };
            bound = next_bound;
        }
    }

    Ok(Some((normalized_base, normalized_deltas)))
}

fn checked_lcm_i128(left: i128, right: i128) -> Option<i128> {
    if left <= 0 || right <= 0 {
        return None;
    }
    if left == right || left >> left.trailing_zeros() == right >> right.trailing_zeros() {
        return Some(left.max(right));
    }
    let divisor = left.gcd(&right);
    left.checked_div(divisor)?.checked_mul(right)
}

fn checked_scale_ratio(value: &Ratio<i128>, denominator: i128) -> Option<i128> {
    if denominator == *value.denom() {
        return Some(*value.numer());
    }
    let source_shift = value.denom().trailing_zeros();
    let target_shift = denominator.trailing_zeros();
    if target_shift >= source_shift && denominator >> target_shift == *value.denom() >> source_shift
    {
        return value
            .numer()
            .checked_mul(1i128 << (target_shift - source_shift));
    }
    let multiplier = denominator.checked_div(*value.denom())?;
    value.numer().checked_mul(multiplier)
}

fn to_i128<const N: usize>(
    normalized: &Normalized<N>,
) -> Result<(SmallKey<N>, SmallDeltas<N>), String> {
    let mut base = [0i128; N];
    for (index, value) in normalized.base.iter().enumerate() {
        base[index] = value
            .to_i128()
            .ok_or_else(|| "exact DP coefficient exceeded i128 after normalization".to_string())?;
    }

    let mut deltas: SmallDeltas<N> = std::array::from_fn(|_| Vec::new());
    for (stat, source) in normalized.deltas.iter().enumerate() {
        deltas[stat] = source
            .iter()
            .map(|delta| {
                let mut converted = [0i128; N];
                for (index, value) in delta.iter().enumerate() {
                    converted[index] = value.to_i128().ok_or_else(|| {
                        "exact DP coefficient exceeded i128 after normalization".to_string()
                    })?;
                }
                Ok(converted)
            })
            .collect::<Result<_, String>>()?;
    }
    Ok((base, deltas))
}

#[derive(Clone)]
struct State<I, const N: usize> {
    key: Key<I, N>,
    combat: Combat,
}

#[allow(clippy::too_many_arguments)]
fn run_dp<I, const N: usize>(
    base: Key<I, N>,
    deltas: [Vec<Key<I, N>>; COMPONENT_COUNT],
    mins: Combat,
    active: [bool; COMPONENT_COUNT],
    budget: usize,
    primary_only: bool,
    allowed: Option<&Additions>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<DpResult, String>
where
    I: Clone + Ord + Add<Output = I>,
{
    let slots = budget + 1;
    let mut states: Vec<Option<State<I, N>>> = vec![None; slots];
    states[0] = Some(State {
        key: base,
        combat: mins,
    });
    let mut additions: Additions = std::array::from_fn(|_| Vec::new());

    for stat in 0..COMPONENT_COUNT {
        if !active[stat] {
            continue;
        }
        check_continue(should_continue)?;
        additions[stat] = vec![Vec::new(); slots];
        let mut next: Vec<Option<State<I, N>>> = vec![None; slots];
        for (destination, destination_additions) in additions[stat].iter_mut().enumerate() {
            check_continue(should_continue)?;
            if let Some(allowed_stat) = allowed.map(|entries| &entries[stat]) {
                if let Some(adds) = allowed_stat.get(destination) {
                    for &add in adds {
                        consider(
                            &states,
                            &mut next,
                            &deltas[stat],
                            mins[stat],
                            stat,
                            destination,
                            usize::from(add),
                            primary_only,
                            destination_additions,
                        )?;
                    }
                }
            } else {
                let max_add = destination.min(deltas[stat].len() - 1);
                for add in 0..=max_add {
                    consider(
                        &states,
                        &mut next,
                        &deltas[stat],
                        mins[stat],
                        stat,
                        destination,
                        add,
                        primary_only,
                        destination_additions,
                    )?;
                }
            }
        }
        states = next;
    }

    let ranks = rank_states(&states, primary_only);
    Ok(DpResult {
        combat: states
            .into_iter()
            .map(|state| state.map(|state| state.combat))
            .collect(),
        additions,
        ranks,
    })
}

fn rank_states<I: Ord, const N: usize>(
    states: &[Option<State<I, N>>],
    primary_only: bool,
) -> Vec<Option<usize>> {
    let mut indices = states
        .iter()
        .enumerate()
        .filter_map(|(index, state)| state.as_ref().map(|_| index))
        .collect::<Vec<_>>();
    indices.sort_by(|&left, &right| {
        compare_keys(
            &states[left].as_ref().expect("ranked state exists").key,
            &states[right].as_ref().expect("ranked state exists").key,
            primary_only,
        )
    });

    let mut ranks = vec![None; states.len()];
    let mut rank = 0;
    for (position, &index) in indices.iter().enumerate() {
        if position > 0 {
            let previous = indices[position - 1];
            if compare_keys(
                &states[index].as_ref().expect("ranked state exists").key,
                &states[previous].as_ref().expect("ranked state exists").key,
                primary_only,
            ) != Ordering::Equal
            {
                rank += 1;
            }
        }
        ranks[index] = Some(rank);
    }
    ranks
}

#[allow(clippy::too_many_arguments)]
fn consider<I, const N: usize>(
    states: &[Option<State<I, N>>],
    next: &mut [Option<State<I, N>>],
    deltas: &[Key<I, N>],
    min: u8,
    stat: usize,
    destination: usize,
    add: usize,
    primary_only: bool,
    additions: &mut Vec<u8>,
) -> Result<(), String>
where
    I: Clone + Ord + Add<Output = I>,
{
    if add > destination || add >= deltas.len() {
        return Ok(());
    }
    let add = u8::try_from(add).map_err(|_| "exact DP addition exceeds u8".to_string())?;
    let value = min
        .checked_add(add)
        .ok_or_else(|| "exact DP combat stat exceeds u8".to_string())?;
    let Some(previous) = states[destination - usize::from(add)].as_ref() else {
        return Ok(());
    };
    let current = next[destination].as_ref();
    let mut ordering = if current.is_some() {
        Ordering::Equal
    } else {
        Ordering::Greater
    };
    let mut key = previous.key.clone();
    let components = if primary_only { N.min(2) } else { N };
    for component in 0..components {
        key[component] =
            previous.key[component].clone() + deltas[usize::from(add)][component].clone();
        if ordering == Ordering::Equal {
            ordering = key[component].cmp(&current.expect("compared state exists").key[component]);
            if ordering == Ordering::Less {
                return Ok(());
            }
        }
    }
    let mut combat = previous.combat;
    combat[stat] = value;
    let candidate = State { key, combat };

    let Some(current) = next[destination].as_ref() else {
        next[destination] = Some(candidate);
        additions.clear();
        additions.push(add);
        return Ok(());
    };
    let better = ordering == Ordering::Greater
        || (ordering == Ordering::Equal && !primary_only && candidate.combat < current.combat);
    if better {
        next[destination] = Some(candidate);
        additions.clear();
        additions.push(add);
    } else if primary_only && ordering == Ordering::Equal {
        additions.push(add);
    }
    Ok(())
}

fn compare_keys<I: Ord, const N: usize>(
    left: &Key<I, N>,
    right: &Key<I, N>,
    primary_only: bool,
) -> Ordering {
    let components = if primary_only { N.min(2) } else { N };
    for component in 0..components {
        let ordering = left[component].cmp(&right[component]);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn scale_rational(value: &BigRational, denominator: &BigInt) -> BigInt {
    if value.is_zero() {
        BigInt::zero()
    } else if denominator == value.denom() {
        value.numer().clone()
    } else {
        value.numer() * (denominator / value.denom())
    }
}

fn scale_rational_dyadic(value: &BigRational, denominator_exponent: usize) -> BigInt {
    if value.is_zero() {
        return BigInt::zero();
    }
    let value_exponent = dyadic_exponent(value.denom()).expect("dyadic denominator");
    value.numer() << (denominator_exponent - value_exponent)
}

fn dyadic_exponent(value: &BigInt) -> Option<usize> {
    if !value.is_positive() {
        return None;
    }
    let trailing_zeros = value.trailing_zeros()?;
    (value.bits() == trailing_zeros.saturating_add(1))
        .then(|| usize::try_from(trailing_zeros).ok())
        .flatten()
}

fn collect_denominator(
    denominator_exponent: &mut Option<usize>,
    denominator: &mut Option<BigInt>,
    value: &BigInt,
) {
    match (*denominator_exponent, dyadic_exponent(value)) {
        (Some(current), Some(exponent)) => {
            *denominator_exponent = Some(current.max(exponent));
        }
        (Some(current), None) => {
            *denominator_exponent = None;
            let dyadic_lcm = BigInt::one() << current;
            *denominator = Some(lcm(&dyadic_lcm, value));
        }
        (None, _) => {
            let current = denominator
                .as_ref()
                .expect("generic denominator initialized");
            *denominator = Some(lcm(current, value));
        }
    }
}

fn lcm(left: &BigInt, right: &BigInt) -> BigInt {
    if left == right || right.is_one() {
        return left.clone();
    }
    if left.is_one() {
        return right.clone();
    }
    if left.is_zero() || right.is_zero() {
        return BigInt::zero();
    }
    left.lcm(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn rational_text(numerator: &str, denominator: &str) -> BigRational {
        BigRational::new(
            BigInt::parse_bytes(numerator.as_bytes(), 10).expect("valid numerator"),
            BigInt::parse_bytes(denominator.as_bytes(), 10).expect("valid denominator"),
        )
    }

    fn row(score: BigRational, ar: BigRational, full: BigRational) -> [BigRational; 5] {
        [score, ar, full, rational(0), rational(0)]
    }

    fn zero_row() -> [BigRational; 5] {
        row(rational(0), rational(0), rational(0))
    }

    fn zero_deltas() -> [Vec<[BigRational; 5]>; 5] {
        std::array::from_fn(|_| vec![zero_row()])
    }

    fn exact_integer(value: i128) -> ExactRational {
        ExactRational::from_integer(BigInt::from(value))
    }

    fn exact_ratio(numerator: i128, denominator: i128) -> ExactRational {
        ExactRational::new(BigInt::from(numerator), BigInt::from(denominator))
    }

    fn exact_row(
        score: ExactRational,
        ar: ExactRational,
        full: ExactRational,
    ) -> [ExactRational; COMPONENT_COUNT] {
        [
            score,
            ar,
            full,
            ExactRational::zero(),
            ExactRational::zero(),
        ]
    }

    fn exact_zero_row() -> [ExactRational; COMPONENT_COUNT] {
        exact_row(exact_integer(0), exact_integer(0), exact_integer(0))
    }

    fn to_big_inputs(
        base: &[ExactRational; COMPONENT_COUNT],
        deltas: &[Vec<[ExactRational; COMPONENT_COUNT]>; COMPONENT_COUNT],
    ) -> (
        [BigRational; COMPONENT_COUNT],
        [Vec<[BigRational; COMPONENT_COUNT]>; COMPONENT_COUNT],
    ) {
        let base = std::array::from_fn(|component| base[component].to_big());
        let deltas = std::array::from_fn(|stat| {
            deltas[stat]
                .iter()
                .map(|delta| std::array::from_fn(|component| delta[component].to_big()))
                .collect()
        });
        (base, deltas)
    }

    #[test]
    fn small_normalization_matches_big_solve() {
        let base = [
            exact_ratio(1, 3),
            exact_integer(2),
            exact_integer(0),
            exact_integer(0),
            exact_integer(0),
        ];
        let mut deltas: [Vec<[ExactRational; COMPONENT_COUNT]>; COMPONENT_COUNT] =
            std::array::from_fn(|_| vec![exact_zero_row()]);
        deltas[0].push(exact_row(
            exact_ratio(1, 6),
            exact_ratio(1, 4),
            exact_integer(0),
        ));
        deltas[1].push(exact_row(
            exact_ratio(1, 4),
            exact_ratio(1, 3),
            exact_integer(1),
        ));

        let (big_base, big_deltas) = to_big_inputs(&base, &deltas);
        let mut small_continue = || true;
        let small = solve_exact(
            &base,
            &deltas,
            [0; COMPONENT_COUNT],
            [true, true, false, false, false],
            2,
            false,
            None,
            &mut small_continue,
        )
        .expect("small exact DP succeeds");
        let mut big_continue = || true;
        let big = solve(
            &big_base,
            &big_deltas,
            [0; COMPONENT_COUNT],
            [true, true, false, false, false],
            2,
            false,
            None,
            &mut big_continue,
        )
        .expect("big exact DP succeeds");
        assert_eq!(small, big);
    }

    #[test]
    fn small_normalization_promotes_on_checked_i128_overflow() {
        let maximum = exact_integer(i128::MAX);
        let base = [
            maximum.clone(),
            exact_integer(0),
            exact_integer(0),
            exact_integer(0),
            exact_integer(0),
        ];
        let mut deltas: [Vec<[ExactRational; COMPONENT_COUNT]>; COMPONENT_COUNT] =
            std::array::from_fn(|_| vec![exact_zero_row()]);
        deltas[0].push(exact_row(
            exact_ratio(i128::MAX, 2),
            exact_integer(0),
            exact_integer(0),
        ));
        let active = [true, false, false, false, false];
        let mut normalize_continue = || true;
        assert!(
            normalize_small(&base, &deltas, active, 2, &mut normalize_continue)
                .expect("small normalization succeeds")
                .is_none()
        );

        let (big_base, big_deltas) = to_big_inputs(&base, &deltas);
        let mut small_continue = || true;
        let promoted = solve_exact(
            &base,
            &deltas,
            [0; COMPONENT_COUNT],
            active,
            1,
            false,
            None,
            &mut small_continue,
        )
        .expect("promoted exact DP succeeds");
        let mut big_continue = || true;
        let expected = solve(
            &big_base,
            &big_deltas,
            [0; COMPONENT_COUNT],
            active,
            1,
            false,
            None,
            &mut big_continue,
        )
        .expect("big exact DP succeeds");
        assert_eq!(promoted, expected);
    }

    #[test]
    fn i128_and_bigint_choose_the_same_winner() {
        let base = [
            rational(0),
            rational(0),
            rational(0),
            rational(0),
            rational(0),
        ];
        let mut small_deltas = zero_deltas();
        small_deltas[0].push(row(rational(2), rational(0), rational(0)));
        small_deltas[1].push(row(rational(1), rational(0), rational(0)));
        let mut small_continue = || true;
        let small = solve(
            &base,
            &small_deltas,
            [0; 5],
            [true, true, false, false, false],
            1,
            false,
            None,
            &mut small_continue,
        )
        .expect("small DP succeeds");

        let denominator = "1000000000000000000000000000000";
        let numerator = "2000000000000000000000000000001";
        let mut big_deltas = zero_deltas();
        big_deltas[0].push(row(
            rational_text(numerator, denominator),
            rational(0),
            rational(0),
        ));
        big_deltas[1].push(row(rational(1), rational(0), rational(0)));
        let mut big_continue = || true;
        let big = solve(
            &base,
            &big_deltas,
            [0; 5],
            [true, true, false, false, false],
            1,
            false,
            None,
            &mut big_continue,
        )
        .expect("big DP succeeds");

        assert_eq!(small.combat[1], Some([1, 0, 0, 0, 0]));
        assert_eq!(big.combat[1], small.combat[1]);
        assert_eq!(big.ranks, small.ranks);
    }

    #[test]
    fn epsilon_completion_is_strict() {
        let base = [
            rational(0),
            rational(0),
            rational(0),
            rational(0),
            rational(0),
        ];
        let epsilon = rational_text(
            "1000000000000000000000000000001",
            "1000000000000000000000000000000",
        );
        let mut deltas = zero_deltas();
        deltas[0].push(row(rational(1), rational(0), rational(0)));
        deltas[1].push(row(epsilon, rational(0), rational(0)));
        let mut should_continue = || true;
        let result = solve(
            &base,
            &deltas,
            [0; 5],
            [true, true, false, false, false],
            1,
            false,
            None,
            &mut should_continue,
        )
        .expect("epsilon DP succeeds");

        assert_eq!(result.combat[1], Some([0, 1, 0, 0, 0]));
    }

    #[test]
    fn primary_ties_retain_every_addition() {
        let base = [
            rational(0),
            rational(0),
            rational(0),
            rational(0),
            rational(0),
        ];
        let mut deltas = zero_deltas();
        deltas[0].push(row(rational(1), rational(0), rational(0)));
        deltas[1].push(row(rational(1), rational(0), rational(5)));
        let mut should_continue = || true;
        let result = solve(
            &base,
            &deltas,
            [0; 5],
            [true, true, false, false, false],
            1,
            true,
            None,
            &mut should_continue,
        )
        .expect("primary DP succeeds");

        assert_eq!(result.additions[1][1], vec![0, 1]);
        assert_eq!(result.ranks, vec![Some(0), Some(1)]);
    }

    #[test]
    fn dyadic_normalization_matches_generic_lcm() {
        let dyadic_base = [
            rational_text("3", "8"),
            rational(0),
            rational(0),
            rational(0),
            rational(0),
        ];
        let mut dyadic_deltas = zero_deltas();
        dyadic_deltas[0].push(row(rational_text("1", "8"), rational(0), rational(0)));
        dyadic_deltas[1].push(row(rational_text("1", "4"), rational(0), rational(0)));

        let generic_base = [
            BigRational::new_raw(BigInt::from(9), BigInt::from(24)),
            rational(0),
            rational(0),
            rational(0),
            rational(0),
        ];
        let mut generic_deltas = zero_deltas();
        generic_deltas[0].push(row(
            BigRational::new_raw(BigInt::from(3), BigInt::from(24)),
            rational(0),
            rational(0),
        ));
        generic_deltas[1].push(row(
            BigRational::new_raw(BigInt::from(6), BigInt::from(24)),
            rational(0),
            rational(0),
        ));

        assert_eq!(dyadic_exponent(&BigInt::from(1)), Some(0));
        assert_eq!(dyadic_exponent(&BigInt::from(8)), Some(3));
        assert_eq!(dyadic_exponent(&BigInt::from(24)), None);

        let active = [true, true, false, false, false];
        let mut dyadic_continue = || true;
        let dyadic = normalize(
            &dyadic_base,
            &dyadic_deltas,
            active,
            2,
            &mut dyadic_continue,
        )
        .expect("dyadic normalization succeeds");
        let mut generic_continue = || true;
        let generic = normalize(
            &generic_base,
            &generic_deltas,
            active,
            2,
            &mut generic_continue,
        )
        .expect("generic normalization succeeds");

        assert_eq!(dyadic.base, generic.base);
        assert_eq!(dyadic.deltas, generic.deltas);
        assert_eq!(dyadic.fits_i128, generic.fits_i128);
    }

    #[test]
    fn cancellation_is_checked_before_and_during_normalization() {
        let base = [
            rational(0),
            rational(0),
            rational(0),
            rational(0),
            rational(0),
        ];
        let deltas = zero_deltas();

        let mut initial_calls = 0;
        let mut initially_cancelled = || {
            initial_calls += 1;
            false
        };
        let error = solve(
            &base,
            &deltas,
            [0; 5],
            [true, true, false, false, false],
            1,
            false,
            None,
            &mut initially_cancelled,
        )
        .expect_err("initial cancellation should stop normalization");
        assert_eq!(error, "cancelled");
        assert_eq!(initial_calls, 1);

        let mut periodic_calls = 0;
        let mut cancel_during_normalization = || {
            periodic_calls += 1;
            periodic_calls < 5
        };
        let error = solve(
            &base,
            &deltas,
            [0; 5],
            [true, true, false, false, false],
            1,
            false,
            None,
            &mut cancel_during_normalization,
        )
        .expect_err("normalization cancellation should stop the DP");
        assert_eq!(error, "cancelled");
        assert_eq!(periodic_calls, 5);
    }
}
