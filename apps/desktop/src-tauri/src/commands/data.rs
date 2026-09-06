use std::collections::{BTreeMap, BTreeSet, HashMap};

use er_optimizer_core::math::STARTING_CLASSES;
use er_optimizer_core::{
    GameData, OptimizeObjective, SomberFilter, Weapon, normalize_weapon_type_display,
};
use tauri::State;

use crate::AppState;
use crate::dto::{
    CatalogDto, ClassMetadataDto, CombatStateDto, CompatibleAowsForAffinityRequestDto,
    CompatibleAowsRequestDto, DisplayPoiseDamageDto, EightStatsDto, FilterDimensionDto,
    FilterOptionDto, WeaponNamesForTypeRequestDto, WeaponProfileDto, WeaponProfileRequestDto,
    WeaponTypeOptionDto,
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
    forces_two_handing_by_weapon: HashMap<String, bool>,
    filter_dimensions: Vec<FilterDimensionDto>,
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
        let mut forces_two_handing_by_weapon = HashMap::<String, bool>::new();
        let mut weapon_family_facets = BTreeMap::<String, (String, usize)>::new();
        let mut weapon_type_facets = BTreeMap::<String, (String, usize)>::new();
        let mut affinity_facets = BTreeMap::<String, (String, usize)>::new();
        let mut reinforcement_facets = BTreeMap::<String, (String, usize)>::new();

        for aow in &data.aows {
            aow_names.insert(aow.name.clone());
        }

        for weapon in &data.weapons {
            if !data.weapon_ar_supported(weapon) {
                continue;
            }
            let weapon_key = index_key(&weapon.name);
            let affinity_key = index_key(&weapon.affinity);
            let weapon_affinity_key = (weapon_key.clone(), affinity_key.clone());
            weapon_names.insert(weapon.name.clone());
            affinity_names.insert(weapon.affinity.clone());
            increment_facet(
                &mut weapon_family_facets,
                weapon.family_filter_id(),
                weapon.name.clone(),
            );
            increment_facet(
                &mut weapon_type_facets,
                weapon.type_filter_id(),
                normalize_weapon_type_display(&weapon.weapon_type_name).to_string(),
            );
            increment_facet(
                &mut affinity_facets,
                weapon.affinity_filter_id(),
                weapon.affinity.clone(),
            );
            increment_facet(
                &mut reinforcement_facets,
                if weapon.is_somber {
                    "reinforcement:somber".to_string()
                } else {
                    "reinforcement:standard".to_string()
                },
                if weapon.is_somber {
                    "Somber".to_string()
                } else {
                    "Standard".to_string()
                },
            );
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

            let cap = if weapon.is_somber {
                data.rules.somber_max_upgrade
            } else {
                data.rules.standard_max_upgrade
            };
            if let Some(cap) = (0..=cap)
                .rev()
                .find(|&level| data.reinforce_level(weapon.reinforce_type, level).is_some())
            {
                upgrade_cap_by_weapon
                    .entry(weapon_key.clone())
                    .and_modify(|current| *current = (*current).max(cap))
                    .or_insert(cap);
                upgrade_cap_by_weapon_affinity
                    .entry(weapon_affinity_key.clone())
                    .and_modify(|current| *current = (*current).max(cap))
                    .or_insert(cap);
            }

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
            forces_two_handing_by_weapon
                .entry(weapon_key.clone())
                .and_modify(|current| *current |= weapon.forces_two_handing())
                .or_insert_with(|| weapon.forces_two_handing());

            if let Some(skill_name) = native_skill_name_for_weapon(data, weapon) {
                aow_names.insert(skill_name.to_string());
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

        let weapon_count = weapon_names.len();
        let mut aow_facets = BTreeMap::<String, (String, usize)>::new();
        aow_facets.insert(
            "aow:none".to_string(),
            ("No applied Ash".to_string(), weapon_count),
        );
        for aow in &data.aows {
            let count = data
                .weapons
                .iter()
                .filter(|weapon| data.weapon_ar_supported(weapon))
                .filter(|weapon| {
                    data.aow_compatible_with_weapon(aow, weapon)
                        || (weapon.native_skill_id == Some(aow.aow_id)
                            && data.native_skill_compatible_with_weapon(weapon))
                })
                .count();
            if count > 0 {
                aow_facets.insert(format!("aow:{}", aow.aow_id), (aow.name.clone(), count));
            }
        }
        for weapon in &data.weapons {
            if !data.weapon_ar_supported(weapon) {
                continue;
            }
            if let Some(skill_name) = native_skill_name_for_weapon(data, weapon)
                && let Some(skill_id) = weapon.native_skill_id
                && !data.aows.iter().any(|aow| aow.aow_id == skill_id)
            {
                aow_facets
                    .entry(format!("aow:{skill_id}"))
                    .or_insert_with(|| (skill_name.to_string(), 0))
                    .1 += 1;
            }
        }
        let coverage = [
            (
                "coverage:weapon-ar",
                "Weapon AR",
                data.capabilities.weapon_ar,
            ),
            (
                "coverage:status",
                "Status buildup",
                data.capabilities.status_buildup,
            ),
            (
                "coverage:aow-compatibility",
                "Ash compatibility",
                data.capabilities.aow_compatibility,
            ),
            (
                "coverage:aow-damage",
                "Ash damage",
                data.capabilities.aow_damage,
            ),
            (
                "coverage:aow-routes",
                "Ash routes",
                data.capabilities.aow_routes,
            ),
        ]
        .into_iter()
        .map(|(id, label, supported)| FilterOptionDto {
            id: id.to_string(),
            label: label.to_string(),
            count: usize::from(supported) * weapon_count,
        })
        .collect();
        let filter_dimensions = vec![
            facet_dimension("weapon_family", "Weapon family", weapon_family_facets),
            facet_dimension("weapon_type", "Weapon type", weapon_type_facets),
            facet_dimension("affinity", "Affinity", affinity_facets),
            facet_dimension("aow", "Ash of War", aow_facets),
            facet_dimension("reinforcement", "Reinforcement", reinforcement_facets),
            FilterDimensionDto {
                id: "coverage".to_string(),
                label: "Model coverage".to_string(),
                options: coverage,
            },
        ];

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
            forces_two_handing_by_weapon,
            filter_dimensions,
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
        weapon_count: index.weapon_names.len(),
        aow_count: data.aows.len(),
        weapon_names: index.weapon_names.clone(),
        weapon_type_keys: index.weapon_type_keys.clone(),
        classes: class_metadata(
            profile.data_manifest.profile.game_version == "1.17",
            profile.data.capabilities.class_budget,
        ),
        weapon_type_options: index.weapon_type_options.clone(),
        aow_names: index.aow_names.clone(),
        affinity_names: index.affinity_names.clone(),
        objective_ids: OptimizeObjective::ALL
            .into_iter()
            .filter(|objective| match objective {
                OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr => {
                    data.capabilities.weapon_ar
                }
                OptimizeObjective::BleedThenAr => data.capabilities.status_buildup,
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
        filter_dimensions: index.filter_dimensions.clone(),
        data_manifest: profile.data_manifest.clone(),
    })
}

fn increment_facet(facets: &mut BTreeMap<String, (String, usize)>, id: String, label: String) {
    facets
        .entry(id)
        .and_modify(|entry| entry.1 += 1)
        .or_insert((label, 1));
}

fn facet_dimension(
    id: &str,
    label: &str,
    facets: BTreeMap<String, (String, usize)>,
) -> FilterDimensionDto {
    let mut options = facets
        .into_iter()
        .map(|(id, (label, count))| FilterOptionDto { id, label, count })
        .collect::<Vec<_>>();
    options.sort_by(|left, right| left.label.cmp(&right.label).then(left.id.cmp(&right.id)));
    FilterDimensionDto {
        id: id.to_string(),
        label: label.to_string(),
        options,
    }
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
    weapon_profile_inner(&profile.data, &profile.catalog_index, &request)
}

fn weapon_profile_inner(
    data: &GameData,
    index: &CatalogIndex,
    request: &WeaponProfileRequestDto,
) -> Result<WeaponProfileDto, AppError> {
    let requirements =
        weapon_requirements(index, &request.weapon_name, request.affinity.as_deref())?;
    let max_upgrade = weapon_upgrade_cap(index, &request.weapon_name, request.affinity.as_deref())?;
    let weapon = data
        .weapons
        .iter()
        .find(|weapon| {
            weapon.name.eq_ignore_ascii_case(&request.weapon_name)
                && request
                    .affinity
                    .as_deref()
                    .is_none_or(|affinity| weapon.affinity.eq_ignore_ascii_case(affinity))
        })
        .ok_or_else(|| AppError::new(format!("weapon not found: {}", request.weapon_name)))?;
    let poise = |values: &er_optimizer_core::DisplayPoiseDamage| DisplayPoiseDamageDto {
        light: values.light.clone(),
        heavy: values.heavy.clone(),
        charged_heavy: values.charged_heavy.clone(),
        jumping_light: values.jumping_light.clone(),
        jumping_heavy: values.jumping_heavy.clone(),
    };
    Ok(WeaponProfileDto {
        can_change_aow: weapon.can_change_aow,
        native_skill_name: native_skill_name_for_weapon(data, weapon).map(str::to_owned),
        requirements: CombatStateDto {
            str_stat: requirements[0],
            dex: requirements[1],
            int_stat: requirements[2],
            fai: requirements[3],
            arc: requirements[4],
        },
        max_upgrade,
        is_somber: weapon.is_somber,
        disables_two_hand_bonus: weapon_disables_two_hand_bonus(
            index,
            &request.weapon_name,
            request.affinity.as_deref(),
        ),
        forces_two_handing: weapon_forces_two_handing(index, &request.weapon_name),
        weight: weapon.weight,
        move_count: weapon.move_count,
        one_handed_poise: poise(&weapon.one_handed_poise),
        two_handed_poise: poise(&weapon.two_handed_poise),
        affinities: affinities_for_weapon_inner(index, &request.weapon_name),
        compatible_aows: compatible_aow_names_inner(
            index,
            Some(&request.weapon_name),
            request.affinity.as_deref(),
        ),
    })
}

pub fn class_metadata(include_tarnished_pack: bool, class_budget: bool) -> Vec<ClassMetadataDto> {
    if !class_budget {
        return vec![ClassMetadataDto {
            name: "Custom stats".to_string(),
            base_level: 0,
            base_total: 0,
            base_stats: EightStatsDto {
                vig: 0,
                mnd: 0,
                end: 0,
                str_stat: 0,
                dex: 0,
                int_stat: 0,
                fai: 0,
                arc: 0,
            },
        }];
    }
    STARTING_CLASSES
        .iter()
        .filter(|class_info| {
            include_tarnished_pack || !matches!(class_info.name, "Idus Knight" | "Heavy Knight")
        })
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

pub fn weapon_forces_two_handing(index: &CatalogIndex, weapon_name: &str) -> bool {
    index
        .forces_two_handing_by_weapon
        .get(&index_key(weapon_name))
        .copied()
        .unwrap_or(false)
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
    if !data.native_skill_compatible_with_weapon(weapon) {
        return None;
    }
    let native_rows = data.native_skill_attack_rows(weapon.weapon_id);
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

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn weapon_profiles_expose_native_skills_and_mount_permissions() {
        let data = er_optimizer_core::load_embedded_game_profile("vanilla").unwrap();
        let index = CatalogIndex::build(&data);
        for (weapon_name, affinity, native_skill, can_change) in [
            ("Buckler", None, Some("Buckler Parry"), true),
            ("Buckler", Some("Blood"), None, true),
            ("Rivers of Blood", None, Some("Corpse Piler"), false),
            ("Icerind Hatchet", None, Some("Hoarfrost Stomp"), false),
        ] {
            let profile = weapon_profile_inner(
                &data,
                &index,
                &WeaponProfileRequestDto {
                    profile_id: "vanilla".to_owned(),
                    weapon_name: weapon_name.to_owned(),
                    affinity: affinity.map(str::to_owned),
                },
            )
            .unwrap();
            assert_eq!(
                profile.native_skill_name.as_deref(),
                native_skill,
                "{weapon_name}"
            );
            assert_eq!(profile.can_change_aow, can_change, "{weapon_name}");
        }
    }

    #[test]
    fn catalog_exposes_every_affinity_once() {
        for (profile, expected_count) in [("vanilla", 13), ("convergence", 22)] {
            let data = er_optimizer_core::load_embedded_game_profile(profile).unwrap();
            let index = CatalogIndex::build(&data);
            let options = &index
                .filter_dimensions
                .iter()
                .find(|dimension| dimension.id == "affinity")
                .unwrap()
                .options;
            assert_eq!(options.len(), expected_count, "{profile}");
            let expected = data
                .weapons
                .iter()
                .filter(|weapon| data.weapon_ar_supported(weapon))
                .map(|weapon| weapon.affinity.as_str())
                .collect::<BTreeSet<_>>();
            assert_eq!(
                options
                    .iter()
                    .map(|option| option.label.as_str())
                    .collect::<BTreeSet<_>>(),
                expected
            );
        }
    }

    #[test]
    fn catalog_preserves_native_skills_without_bypassing_compatibility() {
        for (profile, cases) in [
            (
                "vanilla",
                vec![
                    ("Godskin Peeler", "Cold", "Black Flame Tornado", false),
                    ("Icerind Hatchet", "Standard", "Flaming Strike", false),
                    ("Uchigatana", "Keen", "Firebreather", false),
                    ("Shortbow", "Standard", "Rain of Arrows", true),
                    (
                        "Firespark Perfume Bottle",
                        "Standard",
                        "Rolling Sparks",
                        true,
                    ),
                    ("Steel-Wire Torch", "Standard", "Firebreather", true),
                    ("Buckler", "Standard", "Buckler Parry", true),
                    ("Buckler", "Blood", "Buckler Parry", false),
                    ("Dueling Shield", "Flame Art", "Flaming Strike", true),
                ],
            ),
            (
                "convergence",
                vec![("Dueling Shield", "Night", "Flaming Strike", true)],
            ),
        ] {
            let data = er_optimizer_core::load_embedded_game_profile(profile).unwrap();
            let index = CatalogIndex::build(&data);
            for (weapon, affinity, skill, expected) in cases {
                let names = compatible_aow_names_inner(&index, Some(weapon), Some(affinity));
                assert_eq!(
                    names.iter().any(|name| name == skill),
                    expected,
                    "{profile}: {weapon} / {affinity} / {skill}"
                );
                if expected {
                    assert!(index.aow_names.iter().any(|name| name == skill));
                }
            }
        }
    }

    #[test]
    fn weapon_profiles_use_profile_caps_and_reinforcement_identity() {
        for (profile_id, weapon_name, expected_cap, expected_somber) in [
            (
                "convergence",
                "Galvanic Culling Blade [Twinblade]",
                15,
                true,
            ),
            ("vanilla", "Black Knife", 10, true),
            ("vanilla", "Dagger", 25, false),
            ("vanilla", "Meteorite Staff", 0, true),
        ] {
            let data = er_optimizer_core::load_embedded_game_profile(profile_id).unwrap();
            let index = CatalogIndex::build(&data);
            let profile = weapon_profile_inner(
                &data,
                &index,
                &WeaponProfileRequestDto {
                    profile_id: profile_id.to_owned(),
                    weapon_name: weapon_name.to_owned(),
                    affinity: Some("Standard".to_owned()),
                },
            )
            .unwrap();

            assert_eq!(
                profile.max_upgrade, expected_cap,
                "{profile_id}: {weapon_name}"
            );
            assert_eq!(
                profile.is_somber, expected_somber,
                "{profile_id}: {weapon_name}"
            );
        }
    }

    #[test]
    fn upgrade_metadata_uses_available_levels_within_the_profile_ceiling() {
        let mut data = er_optimizer_core::load_embedded_game_profile("vanilla").unwrap();
        let weapon = data
            .weapons
            .iter()
            .find(|w| w.name == "Dagger" && w.affinity == "Standard")
            .unwrap();
        let reinforce_type = usize::from(weapon.reinforce_type);
        data.reinforce[reinforce_type][25] = None;
        let index = CatalogIndex::build(&data);
        assert_eq!(
            weapon_upgrade_cap(&index, "Dagger", Some("Standard")).unwrap(),
            24
        );
        data.rules.standard_max_upgrade = 3;
        let index = CatalogIndex::build(&data);
        assert_eq!(
            weapon_upgrade_cap(&index, "Dagger", Some("Standard")).unwrap(),
            3
        );
        data.reinforce[reinforce_type].fill(None);
        let index = CatalogIndex::build(&data);
        assert!(weapon_upgrade_cap(&index, "Dagger", Some("Standard")).is_err());
    }
}
