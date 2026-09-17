use std::env;
use std::io::{self, Read};
use std::path::PathBuf;

use er_optimizer_core::math::calculate_status_buildup;
use er_optimizer_core::model::{GameData, Weapon};
use er_optimizer_core::{
    DamageBreakdown, FilterDimension, FilterMode, OptimizeObjective, OptimizeRequest,
    OptimizeResult, ResultGrouping, SomberFilter, StableFilter, Stats, calculate_ar, effective_str,
    load_game_data, meets_requirements, optimize, prepare_loadout_evaluator_with_cancel,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    weapon_name: String,
    affinity: String,
    aow_name: Option<String>,
    upgrade: u8,
    stats: CombatStats,
    #[serde(default)]
    two_handing: bool,
}

#[derive(Clone, Copy, Deserialize)]
struct CombatStats {
    str: u8,
    dex: u8,
    int: u8,
    fai: u8,
    arc: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectedSkill {
    id: Option<u16>,
    name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Detail {
    id: u32,
    weapon: String,
    affinity: String,
    upgrade: u8,
    stats: [u8; 5],
    ar_components: [f32; 5],
    total: f32,
    base_status: [f32; 7],
    selected_skill: SelectedSkill,
}

fn combat_stats(case: &Case) -> Stats {
    Stats {
        vig: 10,
        mnd: 10,
        end: 10,
        str: case.stats.str,
        dex: case.stats.dex,
        int: case.stats.int,
        fai: case.stats.fai,
        arc: case.stats.arc,
    }
}

fn ar_components(ar: DamageBreakdown) -> [f32; 5] {
    [ar.physical, ar.magic, ar.fire, ar.lightning, ar.holy]
}

fn base_status_array(status: er_optimizer_core::model::StatusBuildup) -> [f32; 7] {
    [
        status.poison,
        status.scarlet_rot,
        status.bleed,
        status.frost,
        status.sleep,
        status.madness,
        status.death,
    ]
}

fn detail(
    weapon: &er_optimizer_core::model::Weapon,
    upgrade: u8,
    stats: Stats,
    ar: DamageBreakdown,
    base_status: er_optimizer_core::model::StatusBuildup,
    selected_skill: SelectedSkill,
) -> Detail {
    let components = ar_components(ar);
    Detail {
        id: weapon.weapon_id,
        weapon: weapon.name.clone(),
        affinity: weapon.affinity.clone(),
        upgrade,
        stats: stats.combat_array(),
        ar_components: components,
        total: ar.total(),
        base_status: base_status_array(base_status),
        selected_skill,
    }
}

fn fixed_stats() -> Stats {
    Stats {
        vig: 10,
        mnd: 10,
        end: 10,
        str: 99,
        dex: 99,
        int: 99,
        fai: 99,
        arc: 99,
    }
}

fn finite_breakdown(ar: DamageBreakdown) -> bool {
    ar_components(ar).into_iter().all(f32::is_finite)
}

fn finite_status(status: er_optimizer_core::model::StatusBuildup) -> bool {
    base_status_array(status).into_iter().all(f32::is_finite)
}

#[derive(Default)]
struct KindCounts {
    evaluations: usize,
    mapped: usize,
    unmapped: usize,
    unsupported: usize,
}

#[derive(Default)]
struct MatrixCounts {
    transfer: KindCounts,
    native: KindCounts,
    ar_checks: usize,
    unsupported_ar: usize,
    unsupported_weapon: usize,
    status_checks: usize,
    materialized_routes: usize,
    materialized_hits: usize,
    unsupported_effect_evaluations: usize,
    route_error_evaluations: usize,
}

impl MatrixCounts {
    fn merge(&mut self, other: Self) {
        self.transfer.evaluations += other.transfer.evaluations;
        self.transfer.mapped += other.transfer.mapped;
        self.transfer.unmapped += other.transfer.unmapped;
        self.transfer.unsupported += other.transfer.unsupported;
        self.native.evaluations += other.native.evaluations;
        self.native.mapped += other.native.mapped;
        self.native.unmapped += other.native.unmapped;
        self.native.unsupported += other.native.unsupported;
        self.ar_checks += other.ar_checks;
        self.unsupported_ar += other.unsupported_ar;
        self.unsupported_weapon += other.unsupported_weapon;
        self.status_checks += other.status_checks;
        self.materialized_routes += other.materialized_routes;
        self.materialized_hits += other.materialized_hits;
        self.unsupported_effect_evaluations += other.unsupported_effect_evaluations;
        self.route_error_evaluations += other.route_error_evaluations;
    }
}

#[derive(Clone, Copy)]
struct FixedCase<'a> {
    weapon: &'a Weapon,
    skill_id: u16,
    skill_name: Option<&'a str>,
    upgrade: u8,
    two_handing: bool,
    transfer: bool,
    native: bool,
}

fn kind_counts_mut(counts: &mut MatrixCounts, native: bool) -> &mut KindCounts {
    if native {
        &mut counts.native
    } else {
        &mut counts.transfer
    }
}

fn upgrade_cap(data: &GameData, weapon: &Weapon) -> u8 {
    if weapon.is_somber {
        data.rules.somber_max_upgrade
    } else {
        data.rules.standard_max_upgrade
    }
}

fn max_available_upgrade(data: &GameData, weapon: &Weapon) -> Option<u8> {
    data.reinforce
        .get(usize::from(weapon.reinforce_type))
        .and_then(|levels| {
            levels
                .iter()
                .take(usize::from(upgrade_cap(data, weapon)) + 1)
                .rposition(Option::is_some)
        })
        .map(|level| level as u8)
}

fn fixed_request(
    data: &GameData,
    case: &FixedCase<'_>,
    stats: Stats,
    objective: OptimizeObjective,
) -> OptimizeRequest {
    let mut filters = vec![StableFilter {
        dimension: FilterDimension::Aow,
        id: format!("aow:{}", case.skill_id),
        mode: FilterMode::Include,
    }];
    if case.transfer && case.weapon.affinity.eq_ignore_ascii_case("Standard") {
        filters.push(StableFilter {
            dimension: FilterDimension::Aow,
            id: "aow:none".to_string(),
            mode: FilterMode::Exclude,
        });
    }
    OptimizeRequest {
        class_name: "Wretch".to_string(),
        character_level: stats.sum_all_8() - 79,
        current_stats: stats,
        min_combat_stats: [0; 5],
        locked_combat_stats: stats.combat_array().map(Some),
        standard_max_upgrade: if case.weapon.is_somber {
            data.rules.standard_max_upgrade
        } else {
            case.upgrade
        },
        somber_max_upgrade: if case.weapon.is_somber {
            case.upgrade
        } else {
            data.rules.somber_max_upgrade
        },
        exact_upgrade: true,
        two_handing: case.two_handing,
        dlc_scaling: false,
        scadutree_level: 0,
        weapon_name: Some(case.weapon.name.clone()),
        affinity: Some(case.weapon.affinity.clone()),
        aow_name: case.skill_name.map(str::to_string),
        weapon_type_key: None,
        somber_filter: SomberFilter::All,
        filters,
        result_grouping: ResultGrouping::Loadout,
        objective,
        top_k: 1,
    }
}

fn run_fixed_production(
    request: &OptimizeRequest,
    data: &GameData,
) -> Result<Option<OptimizeResult>, String> {
    let evaluator = prepare_loadout_evaluator_with_cancel(request, data, || true)?;
    evaluator
        .evaluate_with_cancel(request, || true)
        .map(|rows| rows.into_iter().next())
}

fn source_has_unsupported_effect(data: &GameData, case: &FixedCase<'_>) -> bool {
    let exact = case
        .native
        .then(|| data.native_skill_attack_rows(case.weapon.weapon_id));
    let rows = exact
        .filter(|rows| !rows.is_empty())
        .unwrap_or_else(|| data.aow_attack_rows(case.skill_id));
    rows.iter().filter(|row| !row.is_lacking_fp).any(|row| {
        data.aow_effects(row.aow_id, row.sheet_row)
            .iter()
            .chain(data.aow_effects(row.aow_id, 0))
            .any(|effect| !effect.is_supported)
    })
}

fn unsupported_route_error(error: &str) -> bool {
    error.contains("not implemented") || error.contains("not modeled")
}

fn route_has_unsupported_effect(route: &er_optimizer_core::model::AowRouteResult) -> bool {
    route.actions.iter().any(|action| {
        action.hits.iter().any(|hit| {
            !hit.warnings.is_empty() || hit.effects.iter().any(|effect| !effect.is_supported)
        })
    })
}

fn validate_production_result(
    result: &OptimizeResult,
    case: &FixedCase<'_>,
    stats: Stats,
    counts: &mut MatrixCounts,
) -> Result<(bool, bool), String> {
    if result.weapon_id != case.weapon.weapon_id
        || result.weapon_name != case.weapon.name
        || result.affinity != case.weapon.affinity
        || result.upgrade != case.upgrade
        || result.stats != stats
        || result.aow_id != Some(case.skill_id)
        || case.skill_name.is_some_and(|expected| {
            result
                .aow_name
                .as_deref()
                .is_none_or(|actual| !actual.eq_ignore_ascii_case(expected))
        })
    {
        return Err(format!(
            "production result identity mismatch for {} / {} / {}",
            case.weapon.name,
            case.weapon.affinity,
            case.skill_name.unwrap_or("<unnamed native skill>")
        ));
    }
    if !finite_breakdown(result.ar)
        || ![
            result.bleed_buildup,
            result.frost_buildup,
            result.poison_buildup,
            result.scarlet_rot_buildup,
            result.sleep_buildup,
            result.madness_buildup,
            result.death_buildup,
        ]
        .into_iter()
        .all(f32::is_finite)
    {
        return Err(format!(
            "production result has non-finite AR/status for {} / {}",
            case.weapon.name, case.weapon.affinity
        ));
    }
    counts.ar_checks += 1;
    counts.status_checks += 1;
    let Some(route) = result.aow_route.as_ref() else {
        return Ok((false, false));
    };
    if route.route_id.is_empty()
        || !route.first_hit_damage.is_finite()
        || !finite_breakdown(route.total_damage)
        || !route.total_poise_damage.is_finite()
        || !finite_status(route.total_status_buildup)
        || !route.total_stamina_cost.is_finite()
    {
        return Err(format!(
            "production route has non-finite values for {} / {}",
            case.weapon.name, case.weapon.affinity
        ));
    }
    counts.materialized_routes += 1;
    let mut unsupported_effect = route_has_unsupported_effect(route);
    for action in &route.actions {
        if action.action_id.is_empty() || !action.stamina_cost.is_finite() {
            return Err(format!(
                "production route has invalid action for {} / {}",
                case.weapon.name, case.weapon.affinity
            ));
        }
        for hit in &action.hits {
            if hit.raw_name.is_empty()
                || !finite_breakdown(hit.damage)
                || !hit.poise_damage.is_finite()
                || !finite_status(hit.status_buildup)
            {
                return Err(format!(
                    "production route has invalid hit for {} / {}",
                    case.weapon.name, case.weapon.affinity
                ));
            }
            counts.materialized_hits += 1;
            unsupported_effect |=
                !hit.warnings.is_empty() || hit.effects.iter().any(|effect| !effect.is_supported);
        }
    }
    Ok((true, unsupported_effect))
}

fn evaluate_fixed_pair(
    data: &GameData,
    case: &FixedCase<'_>,
    stats: Stats,
    counts: &mut MatrixCounts,
) -> Result<(), String> {
    kind_counts_mut(counts, case.native).evaluations += 1;
    if !data.weapon_ar_supported(case.weapon) {
        counts.unsupported_ar += 1;
        counts.unsupported_weapon += 1;
        kind_counts_mut(counts, case.native).unsupported += 1;
        return Ok(());
    }
    let use_routes = data.capabilities.aow_damage && data.capabilities.aow_routes;
    let unsupported_effect = use_routes && source_has_unsupported_effect(data, case);
    let request = fixed_request(
        data,
        case,
        stats,
        if use_routes {
            OptimizeObjective::AowFullSequence
        } else {
            OptimizeObjective::MaxAr
        },
    );
    let mut route_error = false;
    let mut result = match run_fixed_production(&request, data) {
        Ok(result) => result,
        Err(error) if use_routes && unsupported_route_error(&error) => {
            route_error = true;
            let fallback = fixed_request(data, case, stats, OptimizeObjective::MaxAr);
            run_fixed_production(&fallback, data)?
        }
        Err(error) => return Err(error),
    };
    if use_routes && result.is_none() {
        let fallback = fixed_request(data, case, stats, OptimizeObjective::MaxAr);
        result = run_fixed_production(&fallback, data)?;
    }
    let Some(result) = result else {
        if unsupported_effect || route_error {
            kind_counts_mut(counts, case.native).unsupported += 1;
            counts.unsupported_effect_evaluations += usize::from(unsupported_effect || route_error);
            counts.route_error_evaluations += usize::from(route_error);
            return Ok(());
        }
        return Err(format!(
            "production returned no result for weaponId={} weapon={} affinity={} skillId={} skillName={} upgrade={} hand={} kind={}",
            case.weapon.weapon_id,
            case.weapon.name,
            case.weapon.affinity,
            case.skill_id,
            case.skill_name.unwrap_or("<unnamed>"),
            case.upgrade,
            if case.two_handing { "2H" } else { "1H" },
            if case.native { "native" } else { "transfer" },
        ));
    };
    let (has_route, result_has_unsupported_effect) =
        validate_production_result(&result, case, stats, counts)?;
    if result_has_unsupported_effect || unsupported_effect {
        counts.unsupported_effect_evaluations += 1;
    }
    if !use_routes {
        kind_counts_mut(counts, case.native).unsupported += 1;
        return Ok(());
    }
    if has_route {
        kind_counts_mut(counts, case.native).mapped += 1;
        return Ok(());
    }
    if unsupported_effect || route_error {
        kind_counts_mut(counts, case.native).unsupported += 1;
        return Ok(());
    }
    kind_counts_mut(counts, case.native).unmapped += 1;
    Ok(())
}

fn nonunit_weapon_influence_count(data: &GameData) -> usize {
    data.weapons
        .iter()
        .filter(|weapon| {
            data.attack_element_ext(weapon.attack_element_correct_id)
                .is_some_and(|ext| {
                    (0..5).any(|stat| {
                        (0..5).any(|damage| {
                            ext.stat_scales(stat, damage)
                                && (!ext.influence_rate(stat, damage).is_finite()
                                    || ext.influence_rate(stat, damage) != 1.0)
                        })
                    })
                })
        })
        .count()
}

fn exhaustive(data: &GameData) -> Result<serde_json::Value, String> {
    let stats = fixed_stats();
    let (transfer_pairs, native_pairs, counts) = data
        .weapons
        .par_iter()
        .try_fold(
            || (0usize, 0usize, MatrixCounts::default()),
            |(mut transfer_pairs, mut native_pairs, mut counts),
             weapon|
             -> Result<(usize, usize, MatrixCounts), String> {
                let cap = upgrade_cap(data, weapon);
                let max = max_available_upgrade(data, weapon).ok_or_else(|| {
                    format!(
                        "no reinforcement level at or below cap {} for {} / {}",
                        cap, weapon.name, weapon.affinity
                    )
                })?;
                let upgrades = [0, max];
                for aow in data
                    .aows
                    .iter()
                    .filter(|aow| data.aow_compatible_with_weapon(aow, weapon))
                {
                    transfer_pairs += 1;
                    for upgrade in upgrades {
                        for two_handing in [false, true] {
                            let case = FixedCase {
                                weapon,
                                skill_id: aow.aow_id,
                                skill_name: Some(aow.name.as_str()),
                                upgrade,
                                two_handing,
                                transfer: true,
                                native: false,
                            };
                            evaluate_fixed_pair(data, &case, stats, &mut counts)?;
                        }
                    }
                }

                if let Some(native_skill_id) = weapon
                    .native_skill_id
                    .filter(|_| data.native_skill_compatible_with_weapon(weapon))
                {
                    native_pairs += 1;
                    let skill_name = weapon
                        .native_skill_name
                        .as_deref()
                        .or_else(|| {
                            data.native_skill_attack_rows(weapon.weapon_id)
                                .iter()
                                .find_map(|row| {
                                    (!row.aow_name.is_empty()).then_some(row.aow_name.as_str())
                                })
                        })
                        .or_else(|| {
                            data.aows
                                .iter()
                                .find(|aow| aow.aow_id == native_skill_id)
                                .map(|aow| aow.name.as_str())
                        });
                    for upgrade in upgrades {
                        for two_handing in [false, true] {
                            let case = FixedCase {
                                weapon,
                                skill_id: native_skill_id,
                                skill_name,
                                upgrade,
                                two_handing,
                                transfer: false,
                                native: true,
                            };
                            evaluate_fixed_pair(data, &case, stats, &mut counts)?;
                        }
                    }
                }
                Ok((transfer_pairs, native_pairs, counts))
            },
        )
        .try_reduce(
            || (0usize, 0usize, MatrixCounts::default()),
            |(mut transfer_pairs, mut native_pairs, mut counts),
             (other_transfer, other_native, other_counts)| {
                transfer_pairs += other_transfer;
                native_pairs += other_native;
                counts.merge(other_counts);
                Ok((transfer_pairs, native_pairs, counts))
            },
        )?;

    Ok(serde_json::json!({
        "profile": data.profile_id,
        "weaponConfigurations": data.weapons.len(),
        "transferablePairs": transfer_pairs,
        "nativePairs": native_pairs,
        "transferEvaluations": counts.transfer.evaluations,
        "nativeEvaluations": counts.native.evaluations,
        "arChecks": counts.ar_checks,
        "unsupportedArEvaluations": counts.unsupported_ar,
        "unsupportedWeaponEvaluations": counts.unsupported_weapon,
        "statusChecks": counts.status_checks,
        "nonUnitWeaponInfluenceWeapons": nonunit_weapon_influence_count(data),
        "routes": {
            "damageCapability": data.capabilities.aow_damage,
            "capability": data.capabilities.aow_routes,
            "mappedTransferEvaluations": counts.transfer.mapped,
            "unmappedTransferEvaluations": counts.transfer.unmapped,
            "unsupportedTransferEvaluations": counts.transfer.unsupported,
            "mappedNativeEvaluations": counts.native.mapped,
            "unmappedNativeEvaluations": counts.native.unmapped,
            "unsupportedNativeEvaluations": counts.native.unsupported,
            "materializedRoutes": counts.materialized_routes,
            "materializedHits": counts.materialized_hits,
            "unsupportedEffectEvaluations": counts.unsupported_effect_evaluations,
            "routeErrorEvaluations": counts.route_error_evaluations,
        },
        "fixedStats": stats.combat_array(),
        "upgradeCaps": {
            "standard": data.rules.standard_max_upgrade,
            "somber": data.rules.somber_max_upgrade,
            "separate": data.rules.separate_upgrade_caps,
        },
    }))
}

fn evaluate_base(data: &er_optimizer_core::model::GameData, case: &Case) -> Result<Detail, String> {
    let stats = combat_stats(case);
    let weapon = data
        .weapons
        .iter()
        .find(|weapon| weapon.name == case.weapon_name && weapon.affinity == case.affinity)
        .ok_or_else(|| {
            format!(
                "unknown weapon configuration: {} / {}",
                case.weapon_name, case.affinity
            )
        })?;
    if !data.weapon_ar_supported(weapon) {
        return Err(format!(
            "AR is unsupported for {} / {}",
            case.weapon_name, case.affinity
        ));
    }
    let handed = case.two_handing || weapon.forces_two_handing();
    let effective = effective_str(stats.str, handed, weapon.disable_two_hand_bonus);
    if !meets_requirements(weapon, effective, &stats) {
        return Err(format!(
            "stats do not meet requirements for {} / {}",
            case.weapon_name, case.affinity
        ));
    }
    let ar = calculate_ar(weapon, case.upgrade, &stats, effective, data)?;
    let status = calculate_status_buildup(weapon, case.upgrade, &stats, data)?;
    Ok(detail(
        weapon,
        case.upgrade,
        stats,
        ar,
        status,
        SelectedSkill {
            id: None,
            name: None,
        },
    ))
}

fn evaluate_with_skill(
    data: &er_optimizer_core::model::GameData,
    case: &Case,
    aow_name: &str,
) -> Result<Detail, String> {
    let stats = combat_stats(case);
    let request = OptimizeRequest {
        class_name: "Wretch".to_string(),
        character_level: stats.sum_all_8() - 79,
        current_stats: stats,
        min_combat_stats: [0; 5],
        locked_combat_stats: stats.combat_array().map(Some),
        standard_max_upgrade: case.upgrade,
        somber_max_upgrade: case.upgrade.min(10),
        exact_upgrade: true,
        two_handing: case.two_handing,
        dlc_scaling: false,
        scadutree_level: 0,
        weapon_name: Some(case.weapon_name.clone()),
        affinity: Some(case.affinity.clone()),
        aow_name: Some(aow_name.to_string()),
        weapon_type_key: None,
        somber_filter: SomberFilter::All,
        filters: Vec::new(),
        result_grouping: er_optimizer_core::ResultGrouping::Automatic,
        objective: OptimizeObjective::MaxAr,
        top_k: 1,
    };
    let row = optimize(&request, data)?
        .into_iter()
        .next()
        .ok_or_else(|| "local optimizer returned no rows".to_string())?;
    if row.weapon_name != case.weapon_name
        || row.affinity != case.affinity
        || row.upgrade != case.upgrade
        || row.stats.combat_array() != stats.combat_array()
        || row
            .aow_name
            .as_deref()
            .is_none_or(|name| !name.eq_ignore_ascii_case(aow_name))
    {
        return Err(format!(
            "local result identity mismatch for case {} / {}",
            case.weapon_name, case.affinity
        ));
    }
    let weapon = data
        .weapons
        .iter()
        .find(|weapon| weapon.weapon_id == row.weapon_id)
        .ok_or_else(|| format!("local result has unknown weapon id {}", row.weapon_id))?;
    let status = calculate_status_buildup(weapon, row.upgrade, &row.stats, data)?;
    Ok(detail(
        weapon,
        row.upgrade,
        row.stats,
        row.ar,
        status,
        SelectedSkill {
            id: row.aow_id,
            name: row.aow_name,
        },
    ))
}

fn catalog(data: &er_optimizer_core::model::GameData) -> serde_json::Value {
    let weapons = data
        .weapons
        .iter()
        .map(|weapon| {
            let max_upgrade = max_available_upgrade(data, weapon).unwrap_or(0);
            let ashes = data
                .aows
                .iter()
                .filter(|aow| data.aow_compatible_with_weapon(aow, weapon))
                .map(|aow| {
                    serde_json::json!({
                        "id": aow.aow_id,
                        "name": aow.name,
                        "unbuffed": aow.buff_attack_power.iter().all(|value| *value == 0.0)
                            && aow.poison_buildup_add == 0.0
                            && aow.scarlet_rot_buildup_add == 0.0
                            && aow.bleed_buildup_add == 0.0
                            && aow.frost_buildup_add == 0.0
                            && base_status_array(aow.scaling_status_add).iter().all(|value| *value == 0.0)
                            && base_status_array(aow.persistent_weapon_status_add).iter().all(|value| *value == 0.0)
                            && base_status_array(aow.persistent_on_hit_status_add).iter().all(|value| *value == 0.0),
                    })
                })
                .collect::<Vec<_>>();
            let native_skill = weapon.native_skill_id.map(|id| {
                serde_json::json!({
                    "id": id,
                    "name": weapon.native_skill_name.as_deref(),
                    "compatible": data.native_skill_compatible_with_weapon(weapon),
                })
            });
            serde_json::json!({
                "id": weapon.weapon_id,
                "name": weapon.name,
                "affinity": weapon.affinity,
                "maxUpgrade": max_upgrade,
                "requirements": weapon.requirements,
                "ashes": ashes,
                "canChangeAow": weapon.can_change_aow,
                "nativeSkill": native_skill,
                "supported": data.weapon_ar_supported(weapon),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!(weapons)
}

fn main() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let data_dir = args.next().map(PathBuf::from).ok_or_else(|| {
        "usage: evaluate_ar <data-dir> [--details|--catalog|--exhaustive]".to_string()
    })?;
    let flags = args.collect::<Vec<_>>();
    let details = flags.iter().any(|flag| flag == "--details");
    let catalog_mode = flags.iter().any(|flag| flag == "--catalog");
    let exhaustive_mode = flags.iter().any(|flag| flag == "--exhaustive");
    if flags
        .iter()
        .any(|flag| flag != "--details" && flag != "--catalog" && flag != "--exhaustive")
    {
        return Err("unknown evaluate_ar option".to_string());
    }
    if [details, catalog_mode, exhaustive_mode]
        .into_iter()
        .filter(|enabled| *enabled)
        .count()
        > 1
    {
        return Err("evaluate_ar modes are mutually exclusive".to_string());
    }

    let data = load_game_data(data_dir)?;
    if catalog_mode {
        println!("{}", serde_json::to_string(&catalog(&data)).unwrap());
        return Ok(());
    }
    if exhaustive_mode {
        println!("{}", serde_json::to_string(&exhaustive(&data)?).unwrap());
        return Ok(());
    }

    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let cases: Vec<Case> = serde_json::from_str(&input).map_err(|error| error.to_string())?;
    let details_out = cases
        .iter()
        .map(|case| match case.aow_name.as_deref() {
            Some(aow_name) => evaluate_with_skill(&data, case, aow_name),
            None => evaluate_base(&data, case),
        })
        .collect::<Result<Vec<_>, String>>()?;
    if details {
        println!("{}", serde_json::to_string(&details_out).unwrap());
    } else {
        let totals = details_out
            .iter()
            .map(|result| result.total)
            .collect::<Vec<_>>();
        println!("{}", serde_json::to_string(&totals).unwrap());
    }
    Ok(())
}
