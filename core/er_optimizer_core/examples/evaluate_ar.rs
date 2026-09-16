use std::env;
use std::io::{self, Read};
use std::path::PathBuf;

use er_optimizer_core::math::calculate_status_buildup;
use er_optimizer_core::{
    DamageBreakdown, OptimizeObjective, OptimizeRequest, SomberFilter, Stats, calculate_ar,
    effective_str, load_game_data, meets_requirements, optimize,
};
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
            let max_upgrade = data
                .reinforce
                .get(usize::from(weapon.reinforce_type))
                .and_then(|levels| levels.iter().rposition(Option::is_some))
                .unwrap_or(0) as u8;
            let ashes = data
                .aows
                .iter()
                .filter(|aow| data.aow_compatible_with_weapon(aow, weapon))
                .map(|aow| {
                    serde_json::json!({
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
            serde_json::json!({
                "name": weapon.name,
                "affinity": weapon.affinity,
                "maxUpgrade": max_upgrade,
                "requirements": weapon.requirements,
                "ashes": ashes,
                "supported": data.weapon_ar_supported(weapon),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!(weapons)
}

fn main() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let data_dir = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: evaluate_ar <data-dir> [--details|--catalog]".to_string())?;
    let flags = args.collect::<Vec<_>>();
    let details = flags.iter().any(|flag| flag == "--details");
    let catalog_mode = flags.iter().any(|flag| flag == "--catalog");
    if flags
        .iter()
        .any(|flag| flag != "--details" && flag != "--catalog")
    {
        return Err("unknown evaluate_ar option".to_string());
    }

    let data = load_game_data(data_dir)?;
    if catalog_mode {
        println!("{}", serde_json::to_string(&catalog(&data)).unwrap());
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
