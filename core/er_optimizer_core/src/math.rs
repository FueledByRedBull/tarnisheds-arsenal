use crate::model::{
    DamageBreakdown, DamageType, GameData, Stats, Weapon, COMBAT_STAT_COUNT, STAT_ARC, STAT_DEX, STAT_FAI,
    STAT_INT, STAT_STR,
};

#[derive(Clone, Copy, Debug)]
pub struct ScalingContribution {
    pub scaling: f32,
    pub scaling_mult: f32,
    pub curve_mult: f32,
    pub contributes: bool,
}

pub fn effective_str(str_stat: u8, two_handing: bool) -> u8 {
    if two_handing {
        ((f32::from(str_stat) * 1.5) as u8).min(99)
    } else {
        str_stat
    }
}

pub fn meets_requirements(weapon: &Weapon, effective_str: u8, stats: &Stats) -> bool {
    effective_str >= weapon.requirements[STAT_STR]
        && stats.dex >= weapon.requirements[STAT_DEX]
        && stats.int >= weapon.requirements[STAT_INT]
        && stats.fai >= weapon.requirements[STAT_FAI]
        && stats.arc >= weapon.requirements[STAT_ARC]
}

pub fn build_contributions(
    weapon: &Weapon,
    reinforce: &crate::model::ReinforceLevel,
    aec: &crate::model::AttackElementCorrect,
    curve_mults: &[f32; COMBAT_STAT_COUNT],
    damage_type: DamageType,
) -> [ScalingContribution; COMBAT_STAT_COUNT] {
    [
        ScalingContribution {
            scaling: weapon.scaling[STAT_STR],
            scaling_mult: reinforce.scaling_mult[STAT_STR],
            curve_mult: curve_mults[STAT_STR],
            contributes: aec.stat_scales(STAT_STR, damage_type),
        },
        ScalingContribution {
            scaling: weapon.scaling[STAT_DEX],
            scaling_mult: reinforce.scaling_mult[STAT_DEX],
            curve_mult: curve_mults[STAT_DEX],
            contributes: aec.stat_scales(STAT_DEX, damage_type),
        },
        ScalingContribution {
            scaling: weapon.scaling[STAT_INT],
            scaling_mult: reinforce.scaling_mult[STAT_INT],
            curve_mult: curve_mults[STAT_INT],
            contributes: aec.stat_scales(STAT_INT, damage_type),
        },
        ScalingContribution {
            scaling: weapon.scaling[STAT_FAI],
            scaling_mult: reinforce.scaling_mult[STAT_FAI],
            curve_mult: curve_mults[STAT_FAI],
            contributes: aec.stat_scales(STAT_FAI, damage_type),
        },
        ScalingContribution {
            scaling: weapon.scaling[STAT_ARC],
            scaling_mult: reinforce.scaling_mult[STAT_ARC],
            curve_mult: curve_mults[STAT_ARC],
            contributes: aec.stat_scales(STAT_ARC, damage_type),
        },
    ]
}

pub fn calculate_ar_for_type(actual_base: f32, contributions: &[ScalingContribution; COMBAT_STAT_COUNT]) -> f32 {
    let bonus: f32 = contributions
        .iter()
        .filter(|contribution| contribution.contributes)
        .map(|contribution| {
            contribution.scaling * contribution.scaling_mult * contribution.curve_mult
        })
        .sum();
    actual_base * (1.0 + bonus)
}

pub fn calculate_ar(
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u8,
    data: &GameData,
) -> Result<DamageBreakdown, String> {
    let reinforce = data
        .reinforce_level(weapon.reinforce_type, upgrade)
        .ok_or_else(|| {
            format!(
                "missing reinforce level: type={} level={upgrade}",
                weapon.reinforce_type
            )
        })?;
    let aec = data
        .attack_element(weapon.attack_element_correct_id)
        .ok_or_else(|| format!("missing attack_element_correct_id={}", weapon.attack_element_correct_id))?;

    let curve_mults = [
        data.calc_curve_value(weapon.curve_ids[STAT_STR], effective_str_value)
            .ok_or_else(|| format!("missing str curve_id={}", weapon.curve_ids[STAT_STR]))?,
        data.calc_curve_value(weapon.curve_ids[STAT_DEX], stats.dex)
            .ok_or_else(|| format!("missing dex curve_id={}", weapon.curve_ids[STAT_DEX]))?,
        data.calc_curve_value(weapon.curve_ids[STAT_INT], stats.int)
            .ok_or_else(|| format!("missing int curve_id={}", weapon.curve_ids[STAT_INT]))?,
        data.calc_curve_value(weapon.curve_ids[STAT_FAI], stats.fai)
            .ok_or_else(|| format!("missing fai curve_id={}", weapon.curve_ids[STAT_FAI]))?,
        data.calc_curve_value(weapon.curve_ids[STAT_ARC], stats.arc)
            .ok_or_else(|| format!("missing arc curve_id={}", weapon.curve_ids[STAT_ARC]))?,
    ];

    let mut breakdown = DamageBreakdown::default();
    for damage_type in DamageType::ALL {
        let actual_base =
            weapon.base[damage_type.as_index()] * reinforce.damage_mult[damage_type.as_index()];
        if actual_base <= 0.0 {
            continue;
        }
        let contributions = build_contributions(weapon, reinforce, aec, &curve_mults, damage_type);
        let value = calculate_ar_for_type(actual_base, &contributions);
        match damage_type {
            DamageType::Physical => breakdown.physical = value,
            DamageType::Magic => breakdown.magic = value,
            DamageType::Fire => breakdown.fire = value,
            DamageType::Lightning => breakdown.lightning = value,
            DamageType::Holy => breakdown.holy = value,
        }
    }
    Ok(breakdown)
}

#[derive(Clone, Copy, Debug)]
pub struct StartingClass {
    pub name: &'static str,
    pub base_level: u16,
    pub base_total: u16,
    pub base_stats: Stats,
}

pub const STARTING_CLASSES: [StartingClass; 10] = [
    StartingClass {
        name: "Vagabond",
        base_level: 9,
        base_total: 88,
        base_stats: Stats {
            vig: 15,
            mnd: 10,
            end: 11,
            str: 14,
            dex: 13,
            int: 9,
            fai: 9,
            arc: 7,
        },
    },
    StartingClass {
        name: "Warrior",
        base_level: 8,
        base_total: 87,
        base_stats: Stats {
            vig: 11,
            mnd: 12,
            end: 11,
            str: 10,
            dex: 16,
            int: 10,
            fai: 8,
            arc: 9,
        },
    },
    StartingClass {
        name: "Hero",
        base_level: 7,
        base_total: 86,
        base_stats: Stats {
            vig: 14,
            mnd: 9,
            end: 12,
            str: 16,
            dex: 9,
            int: 7,
            fai: 8,
            arc: 11,
        },
    },
    StartingClass {
        name: "Bandit",
        base_level: 5,
        base_total: 84,
        base_stats: Stats {
            vig: 10,
            mnd: 11,
            end: 10,
            str: 9,
            dex: 13,
            int: 9,
            fai: 8,
            arc: 14,
        },
    },
    StartingClass {
        name: "Astrologer",
        base_level: 6,
        base_total: 85,
        base_stats: Stats {
            vig: 9,
            mnd: 15,
            end: 9,
            str: 8,
            dex: 12,
            int: 16,
            fai: 7,
            arc: 9,
        },
    },
    StartingClass {
        name: "Prophet",
        base_level: 7,
        base_total: 86,
        base_stats: Stats {
            vig: 10,
            mnd: 14,
            end: 8,
            str: 11,
            dex: 10,
            int: 7,
            fai: 16,
            arc: 10,
        },
    },
    StartingClass {
        name: "Samurai",
        base_level: 9,
        base_total: 88,
        base_stats: Stats {
            vig: 12,
            mnd: 11,
            end: 13,
            str: 12,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 8,
        },
    },
    StartingClass {
        name: "Prisoner",
        base_level: 9,
        base_total: 88,
        base_stats: Stats {
            vig: 11,
            mnd: 12,
            end: 11,
            str: 11,
            dex: 14,
            int: 14,
            fai: 6,
            arc: 9,
        },
    },
    StartingClass {
        name: "Confessor",
        base_level: 10,
        base_total: 89,
        base_stats: Stats {
            vig: 10,
            mnd: 13,
            end: 10,
            str: 12,
            dex: 12,
            int: 9,
            fai: 14,
            arc: 9,
        },
    },
    StartingClass {
        name: "Wretch",
        base_level: 1,
        base_total: 80,
        base_stats: Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 10,
            dex: 10,
            int: 10,
            fai: 10,
            arc: 10,
        },
    },
];

pub fn class_by_name(name: &str) -> Option<StartingClass> {
    STARTING_CLASSES
        .iter()
        .copied()
        .find(|class_info| class_info.name.eq_ignore_ascii_case(name))
}

pub fn compute_free_points(
    class_info: StartingClass,
    character_level: u16,
    current_stats: &Stats,
) -> Result<u16, String> {
    let base = class_info.base_stats;
    let floor_ok = current_stats.vig >= base.vig
        && current_stats.mnd >= base.mnd
        && current_stats.end >= base.end
        && current_stats.str >= base.str
        && current_stats.dex >= base.dex
        && current_stats.int >= base.int
        && current_stats.fai >= base.fai
        && current_stats.arc >= base.arc;
    if !floor_ok {
        return Err("current stats are below class minimums".to_string());
    }

    let total_stat_points =
        i32::from(class_info.base_total) + (i32::from(character_level) - i32::from(class_info.base_level));
    let current_sum = i32::from(current_stats.sum_all_8());
    let free = total_stat_points - current_sum;

    if free < 0 {
        return Err("current stats exceed level budget".to_string());
    }
    Ok(free as u16)
}

#[derive(Clone, Debug)]
pub struct StatIter {
    mins: [u8; COMBAT_STAT_COUNT],
    free: u16,
    current: [u8; COMBAT_STAT_COUNT],
    done: bool,
}

impl StatIter {
    pub fn new(mins: [u8; COMBAT_STAT_COUNT], free: u16) -> Result<Self, String> {
        let capacity: u16 = mins.iter().map(|value| 99_u16 - u16::from(*value)).sum();
        if free > capacity {
            return Err(format!(
                "free points {free} exceed combat stat capacity {capacity} for mins={mins:?}"
            ));
        }

        let mut current = mins;
        let mut remaining = free;
        for (idx, min_value) in mins.iter().enumerate() {
            let can_add = (99_u16 - u16::from(*min_value)).min(remaining) as u8;
            current[idx] = *min_value + can_add;
            remaining -= u16::from(can_add);
            if remaining == 0 {
                break;
            }
        }

        Ok(Self {
            mins,
            free,
            current,
            done: false,
        })
    }

    pub fn free(&self) -> u16 {
        self.free
    }
}

impl Iterator for StatIter {
    type Item = [u8; COMBAT_STAT_COUNT];

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let result = self.current;
        let mut pivot: Option<usize> = None;
        for idx in (0..=3).rev() {
            if self.current[idx] > self.mins[idx] {
                pivot = Some(idx);
                break;
            }
        }

        let Some(pivot_idx) = pivot else {
            self.done = true;
            return Some(result);
        };

        self.current[pivot_idx] -= 1;
        let mut freed: u16 = 1;
        for idx in (pivot_idx + 1)..COMBAT_STAT_COUNT {
            freed += u16::from(self.current[idx] - self.mins[idx]);
            self.current[idx] = self.mins[idx];
        }
        for idx in (pivot_idx + 1)..COMBAT_STAT_COUNT {
            if freed == 0 {
                break;
            }
            let room = u16::from(99_u8 - self.current[idx]);
            let add = room.min(freed) as u8;
            self.current[idx] += add;
            freed -= u16::from(add);
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{calculate_ar, data::load_game_data};

    use super::*;

    fn find_weapon<'a>(data: &'a GameData, name: &str, affinity: &str) -> &'a Weapon {
        data.weapons
            .iter()
            .find(|weapon| weapon.name == name && weapon.affinity == affinity)
            .unwrap_or_else(|| panic!("weapon not found: {name} | {affinity}"))
    }

    #[test]
    fn stat_iter_preserves_sum() {
        let mins = [10, 10, 10, 10, 10];
        let free = 7;
        let iter = StatIter::new(mins, free).unwrap();
        let values: Vec<[u8; COMBAT_STAT_COUNT]> = iter.collect();
        assert!(!values.is_empty());

        let target_sum = mins.iter().map(|value| u16::from(*value)).sum::<u16>() + free;
        for value in values {
            let sum = value.iter().map(|point| u16::from(*point)).sum::<u16>();
            assert_eq!(sum, target_sum);
            assert!(value.iter().all(|point| (1..=99).contains(point)));
        }
    }

    #[test]
    fn data_matches_known_ar_cases() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let game_data = load_game_data(data_path).unwrap();

        let uchi = find_weapon(&game_data, "Uchigatana", "Keen");
        let uchi_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 11,
            dex: 40,
            int: 9,
            fai: 8,
            arc: 8,
        };
        let uchi_breakdown =
            calculate_ar(uchi, 25, &uchi_stats, effective_str(uchi_stats.str, false), &game_data).unwrap();
        assert!((uchi_breakdown.total() - 475.983).abs() < 0.01);

        let lordsworn = find_weapon(&game_data, "Lordsworn's Quality Greatsword", "Quality");
        let lordsworn_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 40,
            dex: 40,
            int: 9,
            fai: 9,
            arc: 7,
        };
        let lordsworn_breakdown = calculate_ar(
            lordsworn,
            25,
            &lordsworn_stats,
            effective_str(lordsworn_stats.str, false),
            &game_data,
        )
        .unwrap();
        assert!((lordsworn_breakdown.total() - 582.1702).abs() < 0.01);

        let reduvia = find_weapon(&game_data, "Reduvia", "Standard");
        let reduvia_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 5,
            dex: 13,
            int: 9,
            fai: 8,
            arc: 45,
        };
        let reduvia_breakdown = calculate_ar(
            reduvia,
            10,
            &reduvia_stats,
            effective_str(reduvia_stats.str, false),
            &game_data,
        )
        .unwrap();
        assert!((reduvia_breakdown.total() - 343.6182).abs() < 0.01);
    }
}
