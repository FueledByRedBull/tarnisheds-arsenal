use std::env;
use std::io::{self, Read};
use std::path::PathBuf;

use er_optimizer_core::{
    OptimizeObjective, OptimizeRequest, SomberFilter, Stats, load_game_data, optimize,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    weapon_name: String,
    affinity: String,
    aow_name: String,
    upgrade: u8,
    stats: CombatStats,
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

fn main() -> Result<(), String> {
    let data_dir = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "usage: evaluate_ar <data-dir>".to_string())?;
    let data = load_game_data(data_dir)?;
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let cases: Vec<Case> = serde_json::from_str(&input).map_err(|error| error.to_string())?;
    let totals = cases
        .into_iter()
        .map(|case| {
            let stats = Stats {
                vig: 10,
                mnd: 10,
                end: 10,
                str: case.stats.str,
                dex: case.stats.dex,
                int: case.stats.int,
                fai: case.stats.fai,
                arc: case.stats.arc,
            };
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
                weapon_name: Some(case.weapon_name),
                affinity: Some(case.affinity),
                aow_name: Some(case.aow_name),
                weapon_type_key: None,
                somber_filter: SomberFilter::All,
                objective: OptimizeObjective::MaxAr,
                top_k: 1,
            };
            let rows = optimize(&request, &data)?;
            let row = rows
                .first()
                .ok_or_else(|| "local optimizer returned no rows".to_string())?;
            Ok(row.ar.total())
        })
        .collect::<Result<Vec<_>, String>>()?;
    println!("{}", serde_json::to_string(&totals).unwrap());
    Ok(())
}
