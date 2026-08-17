use std::collections::BTreeSet;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use er_optimizer_core::{
    GameData, LevelOptimizeResult, OptimizeObjective, OptimizeRequest, SomberFilter, Stats,
    load_game_data_with_manifest, optimize, optimize_level_range_with_progress,
};
use serde_json::json;

struct Config {
    repeats: usize,
    horizons: Vec<u16>,
    all_affinities: bool,
}

fn main() -> Result<(), String> {
    let config = parse_args()?;
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .join("phase1");
    let (data, manifest) = load_game_data_with_manifest(data_dir)?;
    let affinities = benchmark_affinities(&data, config.all_affinities);
    println!(
        "{}",
        json!({
            "kind": "metadata",
            "datasetId": manifest.id,
            "modelVersion": manifest.model_version,
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "repeats": config.repeats,
            "warmups": 1,
            "weapon": "Uchigatana",
            "affinities": affinities,
        })
    );

    for horizon in config.horizons {
        let levels: Vec<u16> = (0..=horizon).map(|offset| 80 + offset).collect();
        let mut independent_samples = Vec::with_capacity(config.repeats);
        let mut ranged_samples = Vec::with_capacity(config.repeats);

        run_independent(&data, &affinities, &levels)?;
        run_ranged(&data, &affinities, &levels)?;
        for _ in 0..config.repeats {
            let independent_started = Instant::now();
            let independent = run_independent(&data, &affinities, &levels)?;
            independent_samples.push(independent_started.elapsed());

            let ranged_started = Instant::now();
            let ranged = run_ranged(&data, &affinities, &levels)?;
            ranged_samples.push(ranged_started.elapsed());
            assert_equivalent(&independent, &ranged)?;
        }

        let independent = median(&mut independent_samples);
        let ranged = median(&mut ranged_samples);
        println!(
            "{}",
            json!({
                "kind": "case",
                "workflow": "affinity-watch-level-range",
                "horizon": horizon,
                "levels": levels.len(),
                "affinityCount": affinities.len(),
                "independentMedianMs": millis(independent),
                "sharedRangeMedianMs": millis(ranged),
                "speedup": independent.as_secs_f64() / ranged.as_secs_f64(),
            })
        );
    }
    Ok(())
}

fn parse_args() -> Result<Config, String> {
    let mut repeats = 3_usize;
    let mut horizons = vec![10_u16, 50, 200];
    let mut all_affinities = false;
    for argument in env::args().skip(1) {
        if let Some(value) = argument.strip_prefix("--repeats=") {
            repeats = value
                .parse()
                .map_err(|_| format!("invalid repeat count: {value}"))?;
        } else if let Some(value) = argument.strip_prefix("--horizons=") {
            horizons = value
                .split(',')
                .map(|part| {
                    part.parse::<u16>()
                        .map_err(|_| format!("invalid horizon: {part}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
        } else if argument == "--all-affinities" {
            all_affinities = true;
        } else {
            return Err(format!("unknown argument: {argument}"));
        }
    }
    if repeats == 0 {
        return Err("repeats must be at least one".to_string());
    }
    if horizons.is_empty() || horizons.iter().any(|horizon| *horizon > 200) {
        return Err("horizons must contain values from 0 through 200".to_string());
    }
    horizons.sort_unstable();
    horizons.dedup();
    Ok(Config {
        repeats,
        horizons,
        all_affinities,
    })
}

fn benchmark_affinities(data: &GameData, all: bool) -> Vec<String> {
    let available: BTreeSet<String> = data
        .weapons
        .iter()
        .filter(|weapon| weapon.name == "Uchigatana")
        .map(|weapon| weapon.affinity.clone())
        .collect();
    if all {
        available.into_iter().collect()
    } else {
        ["Keen", "Blood", "Occult"]
            .into_iter()
            .filter(|affinity| available.contains(*affinity))
            .map(str::to_string)
            .collect()
    }
}

fn request(affinity: &str) -> OptimizeRequest {
    OptimizeRequest {
        class_name: "Samurai".to_string(),
        character_level: 80,
        current_stats: Stats {
            vig: 40,
            mnd: 11,
            end: 20,
            str: 12,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 20,
        },
        min_combat_stats: [0; 5],
        locked_combat_stats: [None; 5],
        standard_max_upgrade: 25,
        somber_max_upgrade: 10,
        exact_upgrade: true,
        two_handing: false,
        dlc_scaling: false,
        scadutree_level: 0,
        weapon_name: Some("Uchigatana".to_string()),
        affinity: Some(affinity.to_string()),
        aow_name: Some("Seppuku".to_string()),
        weapon_type_key: None,
        somber_filter: SomberFilter::All,
        objective: OptimizeObjective::BleedThenAr,
        top_k: 1,
    }
}

fn run_independent(
    data: &GameData,
    affinities: &[String],
    levels: &[u16],
) -> Result<Vec<(String, Vec<LevelOptimizeResult>)>, String> {
    affinities
        .iter()
        .map(|affinity| {
            let base = request(affinity);
            let rows = levels
                .iter()
                .map(|level| {
                    let mut level_request = base.clone();
                    level_request.character_level = *level;
                    optimize(&level_request, data).map(|rows| LevelOptimizeResult {
                        level: *level,
                        rows,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((affinity.clone(), rows))
        })
        .collect()
}

fn run_ranged(
    data: &GameData,
    affinities: &[String],
    levels: &[u16],
) -> Result<Vec<(String, Vec<LevelOptimizeResult>)>, String> {
    affinities
        .iter()
        .map(|affinity| {
            optimize_level_range_with_progress(&request(affinity), levels, data, |_| true, || true)
                .map(|rows| (affinity.clone(), rows))
        })
        .collect()
}

fn assert_equivalent(
    independent: &[(String, Vec<LevelOptimizeResult>)],
    ranged: &[(String, Vec<LevelOptimizeResult>)],
) -> Result<(), String> {
    if independent.len() != ranged.len() {
        return Err("affinity result count changed".to_string());
    }
    for ((independent_affinity, independent_levels), (ranged_affinity, ranged_levels)) in
        independent.iter().zip(ranged)
    {
        if independent_affinity != ranged_affinity
            || independent_levels.len() != ranged_levels.len()
        {
            return Err(format!("level series changed for {independent_affinity}"));
        }
        for (independent_level, ranged_level) in independent_levels.iter().zip(ranged_levels) {
            let independent_row = independent_level.rows.first();
            let ranged_row = ranged_level.rows.first();
            let same = independent_level.level == ranged_level.level
                && independent_row.map(|row| row.weapon_id) == ranged_row.map(|row| row.weapon_id)
                && independent_row.map(|row| row.stats.combat_array())
                    == ranged_row.map(|row| row.stats.combat_array())
                && independent_row.map(|row| row.score.to_bits())
                    == ranged_row.map(|row| row.score.to_bits());
            if !same {
                return Err(format!(
                    "shared range diverged for {independent_affinity} at level {}",
                    independent_level.level
                ));
            }
        }
    }
    Ok(())
}

fn median(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
