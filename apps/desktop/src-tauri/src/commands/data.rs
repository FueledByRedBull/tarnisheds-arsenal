use std::collections::BTreeSet;

use er_optimizer_core::{Aow, GameData, Weapon};
use er_optimizer_core::math::STARTING_CLASSES;
use tauri::State;

use crate::AppState;
use crate::dto::{
    CatalogDto, ClassMetadataDto, CombatStateDto, CompatibleAowsForAffinityRequestDto,
    CompatibleAowsRequestDto, EightStatsDto, ScalingDto, WeaponNamesForTypeRequestDto,
    WeaponProfileDto, WeaponProfileRequestDto, WeaponScalingRequestDto, WeaponTypeOptionDto,
};
use crate::errors::AppError;

#[tauri::command]
pub fn get_catalog(state: State<'_, AppState>) -> Result<CatalogDto, AppError> {
    let data = &state.data;
    Ok(CatalogDto {
        weapon_count: data.weapons.len(),
        aow_count: data.aows.len(),
        weapon_names: weapon_names(data),
        weapon_type_keys: weapon_type_keys(data),
        classes: class_metadata(),
        weapon_type_options: weapon_type_options(data),
        aow_names: aow_names(data),
        objective_ids: vec![
            "max_ar".to_string(),
            "max_ar_plus_bleed".to_string(),
            "aow_first_hit".to_string(),
            "aow_full_sequence".to_string(),
        ],
        somber_filters: vec![
            "all".to_string(),
            "standard_only".to_string(),
            "somber_only".to_string(),
        ],
    })
}

#[tauri::command]
pub fn weapon_names_for_type(
    request: WeaponNamesForTypeRequestDto,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    Ok(weapon_names_for_type_inner(
        &state.data,
        request.weapon_type_key.as_deref(),
    ))
}

#[tauri::command]
pub fn compatible_aow_names_for_affinity(
    request: CompatibleAowsForAffinityRequestDto,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    Ok(compatible_aow_names_inner(
        &state.data,
        None,
        request.affinity.as_deref(),
    ))
}

#[tauri::command]
pub fn weapon_scaling_for_upgrade(
    request: WeaponScalingRequestDto,
    state: State<'_, AppState>,
) -> Result<ScalingDto, AppError> {
    weapon_scaling_for_upgrade_inner(
        &state.data,
        &request.weapon_name,
        &request.affinity,
        request.upgrade,
    )
}

#[tauri::command]
pub fn affinities_for_weapon(
    weapon_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    Ok(affinities_for_weapon_inner(&state.data, &weapon_name))
}

#[tauri::command]
pub fn compatible_aow_names(
    request: CompatibleAowsRequestDto,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    Ok(compatible_aow_names_inner(
        &state.data,
        request.weapon_name.as_deref(),
        request.affinity.as_deref(),
    ))
}

#[tauri::command]
pub fn get_weapon_profile(
    request: WeaponProfileRequestDto,
    state: State<'_, AppState>,
) -> Result<WeaponProfileDto, AppError> {
    let requirements = weapon_requirements(
        &state.data,
        &request.weapon_name,
        request.affinity.as_deref(),
    )?;
    let max_upgrade = weapon_upgrade_cap(
        &state.data,
        &request.weapon_name,
        request.affinity.as_deref(),
    )?;
    Ok(WeaponProfileDto {
        requirements: CombatStateDto {
            str_stat: requirements[0],
            dex: requirements[1],
            int_stat: requirements[2],
            fai: requirements[3],
            arc: requirements[4],
        },
        max_upgrade,
        is_somber: max_upgrade <= 10,
        disables_two_hand_bonus: weapon_disables_two_hand_bonus(
            &state.data,
            &request.weapon_name,
            request.affinity.as_deref(),
        ),
        affinities: affinities_for_weapon_inner(&state.data, &request.weapon_name),
        compatible_aows: compatible_aow_names_inner(
            &state.data,
            Some(&request.weapon_name),
            request.affinity.as_deref(),
        ),
    })
}

pub fn weapon_names(data: &GameData) -> Vec<String> {
    let mut set = BTreeSet::new();
    for weapon in &data.weapons {
        set.insert(weapon.name.clone());
    }
    set.into_iter().collect()
}

pub fn aow_names(data: &GameData) -> Vec<String> {
    let mut set = BTreeSet::new();
    for aow in &data.aows {
        set.insert(aow.name.clone());
    }
    set.into_iter().collect()
}

pub fn weapon_type_keys(data: &GameData) -> Vec<String> {
    let mut set = BTreeSet::new();
    for weapon in &data.weapons {
        let name = normalize_weapon_type_display(&weapon.weapon_type_name).trim();
        if !name.is_empty() {
            set.insert(name.to_string());
        }
    }
    set.into_iter().collect()
}

pub fn weapon_type_options(data: &GameData) -> Vec<WeaponTypeOptionDto> {
    let mut set = BTreeSet::<(String, String)>::new();
    for weapon in &data.weapons {
        let label = normalize_weapon_type_display(&weapon.weapon_type_name)
            .trim()
            .to_string();
        if label.is_empty() {
            continue;
        }
        let key = weapon
            .weapon_type_keys
            .split('|')
            .find(|raw| !raw.trim().is_empty() && raw.trim().eq_ignore_ascii_case(&label))
            .or_else(|| weapon.weapon_type_keys.split('|').find(|raw| !raw.trim().is_empty()))
            .map(str::trim)
            .unwrap_or(label.as_str())
            .to_string();
        set.insert((label, key));
    }
    set.into_iter()
        .map(|(label, key)| WeaponTypeOptionDto { key, label })
        .collect()
}

pub fn class_metadata() -> Vec<ClassMetadataDto> {
    STARTING_CLASSES
        .iter()
        .map(|class_info| ClassMetadataDto {
            name: class_info.name.to_string(),
            base_level: class_info.base_level,
            base_total: class_info.base_total,
            base_stats: EightStatsDto {
                vig: class_info.base_stats.vig,
                mnd: class_info.base_stats.mnd,
                end: class_info.base_stats.end,
                str_stat: class_info.base_stats.str,
                dex: class_info.base_stats.dex,
                int_stat: class_info.base_stats.int,
                fai: class_info.base_stats.fai,
                arc: class_info.base_stats.arc,
            },
        })
        .collect()
}

pub fn weapon_names_for_type_inner(data: &GameData, weapon_type_key: Option<&str>) -> Vec<String> {
    let mut set = BTreeSet::new();
    for weapon in &data.weapons {
        let matches_type = weapon_type_key
            .map(|key| {
                weapon
                    .weapon_type_keys
                    .split('|')
                    .any(|candidate| candidate.eq_ignore_ascii_case(key))
                    || normalize_weapon_type_display(&weapon.weapon_type_name).eq_ignore_ascii_case(key)
            })
            .unwrap_or(true);
        if matches_type {
            set.insert(weapon.name.clone());
        }
    }
    set.into_iter().collect()
}

pub fn affinities_for_weapon_inner(data: &GameData, weapon_name: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    for weapon in &data.weapons {
        if weapon.name.eq_ignore_ascii_case(weapon_name) {
            set.insert(weapon.affinity.clone());
        }
    }
    set.into_iter().collect()
}

pub fn compatible_aow_names_inner(
    data: &GameData,
    weapon_name: Option<&str>,
    affinity: Option<&str>,
) -> Vec<String> {
    let mut set = BTreeSet::new();
    for weapon in &data.weapons {
        if let Some(name) = weapon_name {
            if !weapon.name.eq_ignore_ascii_case(name) {
                continue;
            }
        }
        if let Some(affinity) = affinity {
            if !weapon.affinity.eq_ignore_ascii_case(affinity) {
                continue;
            }
        }
        let native_rows = data.native_skill_attack_rows(weapon.weapon_id);
        if !native_rows.is_empty() {
            if let Some(skill_name) = weapon
                .native_skill_name
                .as_deref()
                .or_else(|| native_rows.first().map(|row| row.aow_name.as_str()))
            {
                set.insert(skill_name.to_string());
            }
        }
        for aow in &data.aows {
            if aow_compatible_with_weapon(aow, weapon, data) {
                set.insert(aow.name.clone());
            }
        }
    }
    set.into_iter().collect()
}

pub fn weapon_upgrade_cap(
    data: &GameData,
    weapon_name: &str,
    affinity: Option<&str>,
) -> Result<u8, AppError> {
    let mut best = None;
    for weapon in &data.weapons {
        if !weapon.name.eq_ignore_ascii_case(weapon_name) {
            continue;
        }
        if let Some(affinity) = affinity {
            if !weapon.affinity.eq_ignore_ascii_case(affinity) {
                continue;
            }
        }
        let cap = if weapon.is_somber { 10 } else { 25 };
        best = Some(best.map_or(cap, |current: u8| current.max(cap)));
    }
    best.ok_or_else(|| AppError::new(format!("weapon not found for upgrade cap: {weapon_name}")))
}

pub fn weapon_requirements(
    data: &GameData,
    weapon_name: &str,
    affinity: Option<&str>,
) -> Result<[u8; 5], AppError> {
    let mut best: Option<[u8; 5]> = None;
    for weapon in &data.weapons {
        if !weapon.name.eq_ignore_ascii_case(weapon_name) {
            continue;
        }
        if let Some(affinity) = affinity {
            if !weapon.affinity.eq_ignore_ascii_case(affinity) {
                continue;
            }
        }
        best = Some(match best {
            Some(current) => [
                current[0].max(weapon.requirements[0]),
                current[1].max(weapon.requirements[1]),
                current[2].max(weapon.requirements[2]),
                current[3].max(weapon.requirements[3]),
                current[4].max(weapon.requirements[4]),
            ],
            None => weapon.requirements,
        });
    }
    best.ok_or_else(|| AppError::new(format!("weapon not found for requirements: {weapon_name}")))
}

pub fn weapon_disables_two_hand_bonus(
    data: &GameData,
    weapon_name: &str,
    affinity: Option<&str>,
) -> bool {
    data.weapons.iter().any(|weapon| {
        weapon.name.eq_ignore_ascii_case(weapon_name)
            && affinity
                .map(|affinity| weapon.affinity.eq_ignore_ascii_case(affinity))
                .unwrap_or(true)
            && weapon.disable_two_hand_bonus
    })
}

pub fn weapon_scaling_for_upgrade_inner(
    data: &GameData,
    weapon_name: &str,
    affinity: &str,
    upgrade: u8,
) -> Result<ScalingDto, AppError> {
    let weapon = data
        .weapons
        .iter()
        .find(|weapon| {
            weapon.name.eq_ignore_ascii_case(weapon_name)
                && weapon.affinity.eq_ignore_ascii_case(affinity)
        })
        .ok_or_else(|| {
            AppError::new(format!(
                "weapon not found for scaling lookup: {weapon_name} | {affinity}"
            ))
        })?;
    let reinforce = data
        .reinforce_level(weapon.reinforce_type, upgrade)
        .ok_or_else(|| {
            AppError::new(format!(
                "missing reinforce level for scaling lookup: type={} level={upgrade}",
                weapon.reinforce_type
            ))
        })?;
    Ok(ScalingDto {
        str: weapon.scaling[0] * reinforce.scaling_mult[0],
        dex: weapon.scaling[1] * reinforce.scaling_mult[1],
        int: weapon.scaling[2] * reinforce.scaling_mult[2],
        fai: weapon.scaling[3] * reinforce.scaling_mult[3],
        arc: weapon.scaling[4] * reinforce.scaling_mult[4],
    })
}

fn aow_compatible_with_weapon(aow: &Aow, weapon: &Weapon, data: &GameData) -> bool {
    if let Some(exact_match) = data.exact_aow_compatibility(aow.aow_id, weapon.weapon_id) {
        return exact_match;
    }
    if aow.valid_weapon_types.is_empty() || weapon.weapon_type_keys.is_empty() {
        return false;
    }
    weapon
        .weapon_type_keys
        .split('|')
        .filter(|value| !value.is_empty())
        .any(|weapon_key| {
            aow.valid_weapon_types
                .split('|')
                .filter(|value| !value.is_empty())
                .any(|valid_key| weapon_key == valid_key)
        })
}

pub fn normalize_weapon_type_display(raw: &str) -> &str {
    match raw.trim() {
        "Hand-to-Hand" => "Hand-to-Hand Arts",
        "Heavy Spear" => "Great Spear",
        "Reverse-hand Blade" => "Backhand Blade",
        "Scythe" => "Reaper",
        "Seal" => "Sacred Seal",
        "Staff" => "Glintstone Staff",
        other => other,
    }
}
