use std::collections::{BTreeSet, HashMap};

use er_optimizer_core::math::STARTING_CLASSES;
use er_optimizer_core::{GameData, OptimizeObjective, SomberFilter, Weapon};
use tauri::State;

use crate::AppState;
use crate::dto::{
    CatalogDto, ClassMetadataDto, CombatStateDto, CompatibleAowsForAffinityRequestDto,
    CompatibleAowsRequestDto, EightStatsDto, ScalingDto, WeaponNamesForTypeRequestDto,
    WeaponProfileDto, WeaponProfileRequestDto, WeaponScalingRequestDto, WeaponTypeOptionDto,
};
use crate::errors::AppError;

#[derive(Clone, Debug)]
pub struct CatalogIndex {
    weapon_names: Vec<String>,
    aow_names: Vec<String>,
    affinity_names: Vec<String>,
    weapon_type_keys: Vec<String>,
    weapon_type_options: Vec<WeaponTypeOptionDto>,
    weapon_names_by_type: HashMap<String, Vec<String>>,
    affinities_by_weapon: HashMap<String, Vec<String>>,
    compatible_aows_by_weapon: HashMap<String, Vec<String>>,
    compatible_aows_by_affinity: HashMap<String, Vec<String>>,
    compatible_aows_by_weapon_affinity: HashMap<(String, String), Vec<String>>,
    compatible_aows_all: Vec<String>,
    upgrade_cap_by_weapon: HashMap<String, u8>,
    upgrade_cap_by_weapon_affinity: HashMap<(String, String), u8>,
    requirements_by_weapon: HashMap<String, [u8; 5]>,
    requirements_by_weapon_affinity: HashMap<(String, String), [u8; 5]>,
    disables_two_hand_bonus_by_weapon: HashMap<String, bool>,
    disables_two_hand_bonus_by_weapon_affinity: HashMap<(String, String), bool>,
}

impl CatalogIndex {
    pub fn build(data: &GameData) -> Self {
        let mut weapon_names = BTreeSet::new();
        let mut aow_names = BTreeSet::new();
        let mut affinity_names = BTreeSet::new();
        let mut weapon_type_keys = BTreeSet::new();
        let mut weapon_type_options = BTreeSet::<(String, String)>::new();
        let mut weapon_names_by_type = HashMap::<String, BTreeSet<String>>::new();
        let mut affinities_by_weapon = HashMap::<String, BTreeSet<String>>::new();
        let mut compatible_aows_by_weapon = HashMap::<String, BTreeSet<String>>::new();
        let mut compatible_aows_by_affinity = HashMap::<String, BTreeSet<String>>::new();
        let mut compatible_aows_by_weapon_affinity =
            HashMap::<(String, String), BTreeSet<String>>::new();
        let mut compatible_aows_all = BTreeSet::new();
        let mut upgrade_cap_by_weapon = HashMap::<String, u8>::new();
        let mut upgrade_cap_by_weapon_affinity = HashMap::<(String, String), u8>::new();
        let mut requirements_by_weapon = HashMap::<String, [u8; 5]>::new();
        let mut requirements_by_weapon_affinity = HashMap::<(String, String), [u8; 5]>::new();
        let mut disables_two_hand_bonus_by_weapon = HashMap::<String, bool>::new();
        let mut disables_two_hand_bonus_by_weapon_affinity =
            HashMap::<(String, String), bool>::new();

        for aow in &data.aows {
            aow_names.insert(aow.name.clone());
        }

        for weapon in &data.weapons {
            let weapon_key = index_key(&weapon.name);
            let affinity_key = index_key(&weapon.affinity);
            let weapon_affinity_key = (weapon_key.clone(), affinity_key.clone());
            weapon_names.insert(weapon.name.clone());
            affinity_names.insert(weapon.affinity.clone());
            affinities_by_weapon
                .entry(weapon_key.clone())
                .or_default()
                .insert(weapon.affinity.clone());

            let label = normalize_weapon_type_display(&weapon.weapon_type_name)
                .trim()
                .to_string();
            if !label.is_empty() {
                weapon_type_keys.insert(label.clone());
                let key = weapon
                    .weapon_type_keys
                    .split('|')
                    .find(|raw| !raw.trim().is_empty() && raw.trim().eq_ignore_ascii_case(&label))
                    .or_else(|| {
                        weapon
                            .weapon_type_keys
                            .split('|')
                            .find(|raw| !raw.trim().is_empty())
                    })
                    .map(str::trim)
                    .unwrap_or(label.as_str())
                    .to_string();
                weapon_type_options.insert((label.clone(), key.clone()));
                weapon_names_by_type
                    .entry(index_key(&label))
                    .or_default()
                    .insert(weapon.name.clone());
                weapon_names_by_type
                    .entry(index_key(&key))
                    .or_default()
                    .insert(weapon.name.clone());
            }
            for key in weapon
                .weapon_type_keys
                .split('|')
                .map(str::trim)
                .filter(|key| !key.is_empty())
            {
                weapon_names_by_type
                    .entry(index_key(key))
                    .or_default()
                    .insert(weapon.name.clone());
            }

            let cap = if weapon.is_somber { 10 } else { 25 };
            upgrade_cap_by_weapon
                .entry(weapon_key.clone())
                .and_modify(|current| *current = (*current).max(cap))
                .or_insert(cap);
            upgrade_cap_by_weapon_affinity
                .entry(weapon_affinity_key.clone())
                .and_modify(|current| *current = (*current).max(cap))
                .or_insert(cap);

            requirements_by_weapon
                .entry(weapon_key.clone())
                .and_modify(|current| merge_requirements(current, weapon.requirements))
                .or_insert(weapon.requirements);
            requirements_by_weapon_affinity
                .entry(weapon_affinity_key.clone())
                .and_modify(|current| merge_requirements(current, weapon.requirements))
                .or_insert(weapon.requirements);

            disables_two_hand_bonus_by_weapon
                .entry(weapon_key.clone())
                .and_modify(|current| *current |= weapon.disable_two_hand_bonus)
                .or_insert(weapon.disable_two_hand_bonus);
            disables_two_hand_bonus_by_weapon_affinity
                .entry(weapon_affinity_key.clone())
                .and_modify(|current| *current |= weapon.disable_two_hand_bonus)
                .or_insert(weapon.disable_two_hand_bonus);

            if let Some(skill_name) = native_skill_name_for_weapon(data, weapon) {
                insert_compatible_name(
                    &mut compatible_aows_by_weapon,
                    &mut compatible_aows_by_affinity,
                    &mut compatible_aows_by_weapon_affinity,
                    &mut compatible_aows_all,
                    &weapon_key,
                    &affinity_key,
                    skill_name,
                );
            }
            for aow in &data.aows {
                if data.aow_compatible_with_weapon(aow, weapon) {
                    insert_compatible_name(
                        &mut compatible_aows_by_weapon,
                        &mut compatible_aows_by_affinity,
                        &mut compatible_aows_by_weapon_affinity,
                        &mut compatible_aows_all,
                        &weapon_key,
                        &affinity_key,
                        &aow.name,
                    );
                }
            }
        }

        Self {
            weapon_names: weapon_names.into_iter().collect(),
            aow_names: aow_names.into_iter().collect(),
            affinity_names: affinity_names.into_iter().collect(),
            weapon_type_keys: weapon_type_keys.into_iter().collect(),
            weapon_type_options: weapon_type_options
                .into_iter()
                .map(|(label, key)| WeaponTypeOptionDto { key, label })
                .collect(),
            weapon_names_by_type: finalize_set_map(weapon_names_by_type),
            affinities_by_weapon: finalize_set_map(affinities_by_weapon),
            compatible_aows_by_weapon: finalize_set_map(compatible_aows_by_weapon),
            compatible_aows_by_affinity: finalize_set_map(compatible_aows_by_affinity),
            compatible_aows_by_weapon_affinity: finalize_pair_set_map(
                compatible_aows_by_weapon_affinity,
            ),
            compatible_aows_all: compatible_aows_all.into_iter().collect(),
            upgrade_cap_by_weapon,
            upgrade_cap_by_weapon_affinity,
            requirements_by_weapon,
            requirements_by_weapon_affinity,
            disables_two_hand_bonus_by_weapon,
            disables_two_hand_bonus_by_weapon_affinity,
        }
    }
}

#[tauri::command]
pub fn get_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<crate::dto::DataManifestDto>, AppError> {
    let mut profiles = state
        .profiles
        .values()
        .map(|profile| profile.data_manifest.clone())
        .collect::<Vec<_>>();
    profiles.sort_by(|left, right| {
        (
            left.profile.id != er_optimizer_core::VANILLA_PROFILE_ID,
            &left.profile.id,
        )
            .cmp(&(
                right.profile.id != er_optimizer_core::VANILLA_PROFILE_ID,
                &right.profile.id,
            ))
    });
    Ok(profiles)
}

#[tauri::command]
pub fn get_catalog(profile_id: String, state: State<'_, AppState>) -> Result<CatalogDto, AppError> {
    let profile = state.profile(&profile_id)?;
    let data = &profile.data;
    let index = &profile.catalog_index;
    Ok(CatalogDto {
        weapon_count: data.weapons.len(),
        aow_count: data.aows.len(),
        weapon_names: index.weapon_names.clone(),
        weapon_type_keys: index.weapon_type_keys.clone(),
        classes: class_metadata(),
        weapon_type_options: index.weapon_type_options.clone(),
        aow_names: index.aow_names.clone(),
        affinity_names: index.affinity_names.clone(),
        objective_ids: OptimizeObjective::ALL
            .into_iter()
            .filter(|objective| match objective {
                OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr => {
                    data.capabilities.weapon_ar
                }
                OptimizeObjective::MaxArPlusBleed => data.capabilities.status_buildup,
                OptimizeObjective::AowFirstHit => data.capabilities.aow_damage,
                OptimizeObjective::AowFullSequence => {
                    data.capabilities.aow_damage && data.capabilities.aow_routes
                }
            })
            .map(|objective| objective.as_str().to_string())
            .collect(),
        somber_filters: SomberFilter::ALL
            .into_iter()
            .map(|filter| filter.as_str().to_string())
            .collect(),
        data_manifest: profile.data_manifest.clone(),
    })
}

#[tauri::command]
pub fn get_data_manifest(
    profile_id: String,
    state: State<'_, AppState>,
) -> Result<crate::dto::DataManifestDto, AppError> {
    Ok(state.profile(&profile_id)?.data_manifest.clone())
}

#[tauri::command]
pub fn weapon_names_for_type(
    request: WeaponNamesForTypeRequestDto,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    let profile = state.profile(&request.profile_id)?;
    Ok(weapon_names_for_type_inner(
        &profile.catalog_index,
        request.weapon_type_key.as_deref(),
    ))
}

#[tauri::command]
pub fn compatible_aow_names_for_affinity(
    request: CompatibleAowsForAffinityRequestDto,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    let profile = state.profile(&request.profile_id)?;
    Ok(compatible_aow_names_inner(
        &profile.catalog_index,
        None,
        request.affinity.as_deref(),
    ))
}

#[tauri::command]
pub fn weapon_scaling_for_upgrade(
    request: WeaponScalingRequestDto,
    state: State<'_, AppState>,
) -> Result<ScalingDto, AppError> {
    let profile = state.profile(&request.profile_id)?;
    weapon_scaling_for_upgrade_inner(
        &profile.data,
        &request.weapon_name,
        &request.affinity,
        request.upgrade,
    )
}

#[tauri::command]
pub fn affinities_for_weapon(
    profile_id: String,
    weapon_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    let profile = state.profile(&profile_id)?;
    Ok(affinities_for_weapon_inner(
        &profile.catalog_index,
        &weapon_name,
    ))
}

#[tauri::command]
pub fn compatible_aow_names(
    request: CompatibleAowsRequestDto,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    let profile = state.profile(&request.profile_id)?;
    Ok(compatible_aow_names_inner(
        &profile.catalog_index,
        request.weapon_name.as_deref(),
        request.affinity.as_deref(),
    ))
}

#[tauri::command]
pub fn get_weapon_profile(
    request: WeaponProfileRequestDto,
    state: State<'_, AppState>,
) -> Result<WeaponProfileDto, AppError> {
    let profile = state.profile(&request.profile_id)?;
    let index = &profile.catalog_index;
    let requirements =
        weapon_requirements(index, &request.weapon_name, request.affinity.as_deref())?;
    let max_upgrade = weapon_upgrade_cap(index, &request.weapon_name, request.affinity.as_deref())?;
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
            index,
            &request.weapon_name,
            request.affinity.as_deref(),
        ),
        affinities: affinities_for_weapon_inner(index, &request.weapon_name),
        compatible_aows: compatible_aow_names_inner(
            index,
            Some(&request.weapon_name),
            request.affinity.as_deref(),
        ),
    })
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

pub fn weapon_names_for_type_inner(
    index: &CatalogIndex,
    weapon_type_key: Option<&str>,
) -> Vec<String> {
    let Some(type_key) = weapon_type_key else {
        return index.weapon_names.clone();
    };
    index
        .weapon_names_by_type
        .get(&index_key(type_key))
        .cloned()
        .unwrap_or_default()
}

pub fn affinities_for_weapon_inner(index: &CatalogIndex, weapon_name: &str) -> Vec<String> {
    index
        .affinities_by_weapon
        .get(&index_key(weapon_name))
        .cloned()
        .unwrap_or_default()
}

pub fn compatible_aow_names_inner(
    index: &CatalogIndex,
    weapon_name: Option<&str>,
    affinity: Option<&str>,
) -> Vec<String> {
    match (weapon_name, affinity) {
        (Some(weapon_name), Some(affinity)) => index
            .compatible_aows_by_weapon_affinity
            .get(&(index_key(weapon_name), index_key(affinity)))
            .cloned()
            .unwrap_or_default(),
        (Some(weapon_name), None) => index
            .compatible_aows_by_weapon
            .get(&index_key(weapon_name))
            .cloned()
            .unwrap_or_default(),
        (None, Some(affinity)) => index
            .compatible_aows_by_affinity
            .get(&index_key(affinity))
            .cloned()
            .unwrap_or_default(),
        (None, None) => index.compatible_aows_all.clone(),
    }
}

pub fn weapon_upgrade_cap(
    index: &CatalogIndex,
    weapon_name: &str,
    affinity: Option<&str>,
) -> Result<u8, AppError> {
    let cap = match affinity {
        Some(affinity) => index
            .upgrade_cap_by_weapon_affinity
            .get(&(index_key(weapon_name), index_key(affinity)))
            .copied(),
        None => index
            .upgrade_cap_by_weapon
            .get(&index_key(weapon_name))
            .copied(),
    };
    cap.ok_or_else(|| AppError::new(format!("weapon not found for upgrade cap: {weapon_name}")))
}

pub fn weapon_requirements(
    index: &CatalogIndex,
    weapon_name: &str,
    affinity: Option<&str>,
) -> Result<[u8; 5], AppError> {
    let requirements = match affinity {
        Some(affinity) => index
            .requirements_by_weapon_affinity
            .get(&(index_key(weapon_name), index_key(affinity)))
            .copied(),
        None => index
            .requirements_by_weapon
            .get(&index_key(weapon_name))
            .copied(),
    };
    requirements
        .ok_or_else(|| AppError::new(format!("weapon not found for requirements: {weapon_name}")))
}

pub fn weapon_disables_two_hand_bonus(
    index: &CatalogIndex,
    weapon_name: &str,
    affinity: Option<&str>,
) -> bool {
    match affinity {
        Some(affinity) => index
            .disables_two_hand_bonus_by_weapon_affinity
            .get(&(index_key(weapon_name), index_key(affinity)))
            .copied()
            .unwrap_or(false),
        None => index
            .disables_two_hand_bonus_by_weapon
            .get(&index_key(weapon_name))
            .copied()
            .unwrap_or(false),
    }
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

fn index_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn merge_requirements(current: &mut [u8; 5], next: [u8; 5]) {
    for idx in 0..current.len() {
        current[idx] = current[idx].max(next[idx]);
    }
}

fn native_skill_name_for_weapon<'a>(data: &'a GameData, weapon: &'a Weapon) -> Option<&'a str> {
    let native_rows = data.native_skill_attack_rows(weapon.weapon_id);
    if native_rows.is_empty() {
        return None;
    }
    weapon
        .native_skill_name
        .as_deref()
        .or_else(|| native_rows.first().map(|row| row.aow_name.as_str()))
}

fn insert_compatible_name(
    by_weapon: &mut HashMap<String, BTreeSet<String>>,
    by_affinity: &mut HashMap<String, BTreeSet<String>>,
    by_weapon_affinity: &mut HashMap<(String, String), BTreeSet<String>>,
    all: &mut BTreeSet<String>,
    weapon_key: &str,
    affinity_key: &str,
    name: &str,
) {
    by_weapon
        .entry(weapon_key.to_string())
        .or_default()
        .insert(name.to_string());
    by_affinity
        .entry(affinity_key.to_string())
        .or_default()
        .insert(name.to_string());
    by_weapon_affinity
        .entry((weapon_key.to_string(), affinity_key.to_string()))
        .or_default()
        .insert(name.to_string());
    all.insert(name.to_string());
}

fn finalize_set_map(map: HashMap<String, BTreeSet<String>>) -> HashMap<String, Vec<String>> {
    map.into_iter()
        .map(|(key, values)| (key, values.into_iter().collect()))
        .collect()
}

fn finalize_pair_set_map(
    map: HashMap<(String, String), BTreeSet<String>>,
) -> HashMap<(String, String), Vec<String>> {
    map.into_iter()
        .map(|(key, values)| (key, values.into_iter().collect()))
        .collect()
}
