//! Integer keys for the coupled-stat enumeration (optimizer-math Section 5).
//!
//! Every coupled candidate's key is a sum of precomputed exact values: route parts read per
//! stat value, the separable AR and bleed deltas of the coupled stats, and the best completion
//! of its spend. Scaling one key component by a positive common denominator preserves its
//! order, so comparing the scaled integers selects the same allocation as exact rationals,
//! without a gcd per addition.

use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::Zero;

use super::{RelevantStatSearch, SearchProgress};
use crate::math::exact::{ExactRouteParts, ExactStatTable};
use crate::math::exact_value::ExactRational;
use crate::model::COMBAT_STAT_COUNT;

pub(super) type Combat = [u8; COMBAT_STAT_COUNT];
const KEY: usize = 5;

/// Where a key component's head comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Head {
    /// Coupled route damage: the first damaging hit or the full sequence.
    Route { first_hit: bool },
    /// Separable in the coupled stats: a base plus one delta per coupled stat.
    Separable,
}

/// One key component as exact or integer values; a candidate sums its constant, one entry of
/// each table, each corrected group's selection and the tail of its spend.
#[derive(Clone)]
pub(super) struct Component<T> {
    constant: T,
    tables: Vec<Table<T>>,
    corrected: Vec<Vec<Table<T>>>,
    tails: Vec<Option<T>>,
}

#[derive(Clone)]
struct Table<T> {
    stat: usize,
    raw_str: bool,
    offset: u16,
    values: Vec<T>,
}

/// Builds the five exact components. `deltas[stat][add]` is the separable key change of
/// raising one coupled stat by `add`; `tails[spent]` is the best completion of that spend.
/// Route tables keep only the values `reachable(stat, raw_str)` allows, and a table with one
/// reachable value outside a corrected group joins the constant.
pub(super) fn exact_components(
    heads: [Head; KEY],
    route: (&ExactRouteParts, &ExactRouteParts),
    separable_base: &[ExactRational; KEY],
    deltas: &[(usize, Vec<[ExactRational; KEY]>)],
    mins: Combat,
    tails: &[Option<[ExactRational; KEY]>],
    reachable: impl Fn(usize, bool) -> (u16, u16),
) -> Result<[Component<ExactRational>; KEY], String> {
    let restrict = |table: &ExactStatTable| {
        let (low, high) = reachable(table.stat, table.raw_str);
        let values = table
            .values
            .get(usize::from(low)..=usize::from(high))
            .ok_or_else(|| format!("route table for stat {} has no value {high}", table.stat))?;
        Ok::<_, String>(Table {
            stat: table.stat,
            raw_str: table.raw_str,
            offset: low,
            values: values.to_vec(),
        })
    };
    let mut components = Vec::with_capacity(KEY);
    for (component, head) in heads.into_iter().enumerate() {
        let tails = tails
            .iter()
            .map(|tail| tail.as_ref().map(|tail| tail[component].clone()))
            .collect();
        components.push(match head {
            Head::Route { first_hit } => {
                let parts = if first_hit { route.0 } else { route.1 };
                let mut constant = parts.constant.clone();
                let mut tables = Vec::new();
                for table in &parts.tables {
                    let table = restrict(table)?;
                    match table.values.as_slice() {
                        [value] => constant += value,
                        _ => tables.push(table),
                    }
                }
                Component {
                    constant,
                    tables,
                    corrected: parts
                        .corrected
                        .iter()
                        .map(|group| group.iter().map(restrict).collect())
                        .collect::<Result<_, _>>()?,
                    tails,
                }
            }
            Head::Separable => Component {
                constant: separable_base[component].clone(),
                tables: deltas
                    .iter()
                    .map(|(stat, values)| Table {
                        stat: *stat,
                        raw_str: true,
                        offset: u16::from(mins[*stat]),
                        values: values
                            .iter()
                            .map(|delta| delta[component].clone())
                            .collect(),
                    })
                    .collect(),
                corrected: Vec::new(),
                tails,
            },
        });
    }
    Ok(components
        .try_into()
        .ok()
        .expect("one component per key entry"))
}

/// Scales every value of each component by that component's least common denominator.
pub(super) fn normalize(components: &[Component<ExactRational>; KEY]) -> [Component<BigInt>; KEY] {
    std::array::from_fn(|index| {
        let component = components[index].map(&ExactRational::to_big);
        // Few distinct denominators recur across thousands of values.
        let mut denominators: Vec<&BigInt> =
            component.values().map(|value| value.denom()).collect();
        denominators.sort_unstable();
        denominators.dedup();
        let denominator = denominators
            .iter()
            .fold(BigInt::from(1), |lcm, denominator| lcm.lcm(denominator));
        let factors: Vec<(&BigInt, BigInt)> = denominators
            .into_iter()
            .map(|value| (value, &denominator / value))
            .collect();
        component.map(&|value: &BigRational| {
            let (_, factor) = factors
                .iter()
                .find(|(denominator, _)| *denominator == value.denom())
                .expect("every denominator was collected");
            value.numer() * factor
        })
    })
}

impl<T> Component<T> {
    fn values(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.constant)
            .chain(self.tables.iter().flat_map(|table| &table.values))
            .chain(
                self.corrected
                    .iter()
                    .flatten()
                    .flat_map(|table| &table.values),
            )
            .chain(self.tails.iter().flatten())
    }

    fn map<U>(&self, convert: &impl Fn(&T) -> U) -> Component<U> {
        let table = |table: &Table<T>| Table {
            stat: table.stat,
            raw_str: table.raw_str,
            offset: table.offset,
            values: table.values.iter().map(convert).collect(),
        };
        Component {
            constant: convert(&self.constant),
            tables: self.tables.iter().map(table).collect(),
            corrected: self
                .corrected
                .iter()
                .map(|group| group.iter().map(table).collect())
                .collect(),
            tails: self
                .tails
                .iter()
                .map(|tail| tail.as_ref().map(convert))
                .collect(),
        }
    }
}

/// The same components as 256-bit integers, when no candidate's sum can overflow: every value
/// stays below 2^250 and a key component adds at most 32 of them, so sums stay below 2^255.
pub(super) fn narrow(components: &[Component<BigInt>; KEY]) -> Option<[Component<I256>; KEY]> {
    let mut narrowed = Vec::with_capacity(KEY);
    for component in components {
        let terms =
            2 + component.tables.len() + component.corrected.iter().map(Vec::len).sum::<usize>();
        if terms > 32 {
            return None;
        }
        let table = |table: &Table<BigInt>| {
            Some(Table {
                stat: table.stat,
                raw_str: table.raw_str,
                offset: table.offset,
                values: table
                    .values
                    .iter()
                    .map(I256::from_big)
                    .collect::<Option<_>>()?,
            })
        };
        narrowed.push(Component {
            constant: I256::from_big(&component.constant)?,
            tables: component.tables.iter().map(table).collect::<Option<_>>()?,
            corrected: component
                .corrected
                .iter()
                .map(|group| group.iter().map(table).collect::<Option<_>>())
                .collect::<Option<_>>()?,
            tails: component
                .tails
                .iter()
                .map(|tail| match tail {
                    Some(tail) => I256::from_big(tail).map(Some),
                    None => Some(None),
                })
                .collect::<Option<_>>()?,
        });
    }
    narrowed.try_into().ok()
}

/// Integer operations the candidate keys need.
pub(super) trait KeyInt: Clone + Ord {
    fn zero() -> Self;
    fn add(&mut self, other: &Self);
}

impl KeyInt for BigInt {
    fn zero() -> Self {
        Zero::zero()
    }

    fn add(&mut self, other: &Self) {
        *self += other;
    }
}

/// A two's-complement 256-bit integer with exact addition and order; callers keep sums in
/// range (see `narrow`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct I256 {
    // Field order makes the derived order signed on `hi`, then unsigned on `lo`.
    hi: i128,
    lo: u128,
}

impl I256 {
    fn from_big(value: &BigInt) -> Option<Self> {
        if value.bits() > 250 {
            return None;
        }
        let digits = value.magnitude().to_u64_digits();
        let digit = |index: usize| u128::from(digits.get(index).copied().unwrap_or(0));
        let magnitude = Self {
            hi: (digit(2) | digit(3) << 64) as i128,
            lo: digit(0) | digit(1) << 64,
        };
        Some(if value.sign() == Sign::Minus {
            let lo = (!magnitude.lo).wrapping_add(1);
            Self {
                hi: (!magnitude.hi).wrapping_add(i128::from(lo == 0)),
                lo,
            }
        } else {
            magnitude
        })
    }
}

impl KeyInt for I256 {
    fn zero() -> Self {
        Self { hi: 0, lo: 0 }
    }

    fn add(&mut self, other: &Self) {
        let (lo, carry) = self.lo.overflowing_add(other.lo);
        self.hi = self
            .hi
            .wrapping_add(other.hi)
            .wrapping_add(i128::from(carry));
        self.lo = lo;
    }
}

/// The best coupled allocation under these keys: the largest key, then the smaller combat.
/// `completions[spent]` holds the combat that completes each coupled spend.
pub(super) fn best_allocation<T: KeyInt, P: SearchProgress>(
    components: &[Component<T>; KEY],
    enumerated: &RelevantStatSearch,
    coupled: &[usize],
    completions: &[Option<Combat>],
    effective_str: impl Fn(u8) -> u16,
    progress: &mut P,
) -> Result<Option<Combat>, String> {
    let mut best: Option<([T; KEY], Combat)> = None;
    let mut error = None;
    let mut current = enumerated.mins;
    // Uncoupled stats are filled like inactive ones here; only the coupled values are used.
    enumerated.visit(&mut current, |filled| {
        let candidate =
            progress.poll().and_then(|()| {
                let mut allocation = enumerated.mins;
                for &stat in coupled {
                    allocation[stat] = filled[stat];
                }
                let spent = coupled
                    .iter()
                    .map(|&stat| usize::from(allocation[stat] - enumerated.mins[stat]))
                    .sum::<usize>();
                let completion =
                    completions.get(spent).copied().flatten().ok_or_else(|| {
                        "feasible coupled allocation has no completion".to_string()
                    })?;
                let effective = effective_str(allocation[0]);
                let value_of = |table| table_value(table, &allocation, effective);
                let mut key: [T; KEY] =
                    std::array::from_fn(|index| components[index].constant.clone());
                for (sum, component) in key.iter_mut().zip(components) {
                    for table in &component.tables {
                        sum.add(value_of(table)?);
                    }
                    for group in &component.corrected {
                        let mut minimum: Option<&T> = None;
                        for table in group {
                            let value = value_of(table)?;
                            if minimum.is_none_or(|minimum| value < minimum) {
                                minimum = Some(value);
                            }
                        }
                        match minimum {
                            Some(minimum) if *minimum < T::zero() => sum.add(minimum),
                            _ => {
                                for table in group {
                                    sum.add(value_of(table)?);
                                }
                            }
                        }
                    }
                    sum.add(component.tails[spent].as_ref().ok_or_else(|| {
                        "feasible coupled allocation has no completion".to_string()
                    })?);
                }
                let mut combat = completion;
                for &stat in coupled {
                    combat[stat] = allocation[stat];
                }
                Ok((key, combat))
            });
        match candidate {
            Ok((key, combat)) => {
                if best.as_ref().is_none_or(|(current, current_combat)| {
                    key > *current || key == *current && combat < *current_combat
                }) {
                    best = Some((key, combat));
                }
                true
            }
            Err(failure) => {
                error = Some(failure);
                false
            }
        }
    });
    if let Some(error) = error {
        return Err(error);
    }
    Ok(best.map(|(_, combat)| combat))
}

fn table_value<'a, T>(
    table: &'a Table<T>,
    allocation: &Combat,
    effective_str: u16,
) -> Result<&'a T, String> {
    let value = if table.stat == 0 && !table.raw_str {
        effective_str
    } else {
        u16::from(allocation[table.stat])
    };
    value
        .checked_sub(table.offset)
        .and_then(|index| table.values.get(usize::from(index)))
        .ok_or_else(|| format!("coupled key table has no value for stat {}", table.stat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_integers_add_and_order_like_big_integers() {
        let samples: Vec<BigInt> = [
            "0",
            "1",
            "-1",
            "340282366920938463463374607431768211455",
            "-340282366920938463463374607431768211456",
            "123456789012345678901234567890123456789012345678901234567890",
            "-987654321098765432109876543210987654321098765432109876543210",
            "1809251394333065553493296640760748560207343510400633813116524750123642650623",
        ]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
        for left in &samples {
            for right in &samples {
                let (Some(mut wide), Some(other)) = (I256::from_big(left), I256::from_big(right))
                else {
                    assert!(left.bits() > 250 || right.bits() > 250);
                    continue;
                };
                assert_eq!(wide.cmp(&other), left.cmp(right), "{left} vs {right}");
                let sum = left + right;
                if sum.bits() <= 250 {
                    wide.add(&other);
                    assert_eq!(Some(wide), I256::from_big(&sum), "{left} + {right}");
                }
            }
        }
        assert!(I256::from_big(&(BigInt::from(1) << 251)).is_none());
    }
}
