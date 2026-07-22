use std::env;
use std::path::PathBuf;
use std::time::Duration;

use er_optimizer_core::{
    GameData, OptimizeObjective, OptimizeRequest, ProfiledOptimizeResult, SomberFilter, Stats,
    load_game_data_with_manifest, optimize_profiled,
};
use serde_json::json;

const PREFIX: &str = "PHASE_BENCH ";

fn main() -> Result<(), String> {
    let (warmups, repeats) = parse_args()?;
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .join("phase1");
    let (data, manifest) = load_game_data_with_manifest(data_dir)?;
    println!(
        "{PREFIX}{}",
        json!({
            "kind": "metadata",
            "datasetId": manifest.id,
            "datasetVersion": manifest.dataset_version,
            "modelVersion": manifest.model_version,
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "rayonThreads": rayon::current_num_threads(),
            "warmups": warmups,
            "repeats": repeats,
        })
    );

    for (name, request) in cases() {
        for _ in 0..warmups {
            optimize_profiled(&request, &data)?;
        }
        let mut samples = Vec::with_capacity(repeats);
        for _ in 0..repeats {
            samples.push(optimize_profiled(&request, &data)?);
        }
        print_case(name, &request, &data, samples)?;
    }
    Ok(())
}

fn parse_args() -> Result<(usize, usize), String> {
    let mut warmups = 1_usize;
    let mut repeats = 5_usize;
    for argument in env::args().skip(1) {
        if let Some(value) = argument.strip_prefix("--warmups=") {
            warmups = value
                .parse()
                .map_err(|_| format!("invalid warmup count: {value}"))?;
        } else if let Some(value) = argument.strip_prefix("--repeats=") {
            repeats = value
                .parse()
                .map_err(|_| format!("invalid repeat count: {value}"))?;
        } else {
            return Err(format!("unknown argument: {argument}"));
        }
    }
    if repeats == 0 {
        return Err("repeats must be at least one".to_string());
    }
    Ok((warmups, repeats))
}

fn print_case(
    name: &str,
    request: &OptimizeRequest,
    _data: &GameData,
    samples: Vec<ProfiledOptimizeResult>,
) -> Result<(), String> {
    let first = samples.first().ok_or_else(|| "no samples".to_string())?;
    if samples.iter().any(|sample| {
        sample.rows.len() != first.rows.len()
            || sample.estimate.combinations != first.estimate.combinations
    }) {
        return Err(format!("{name} produced inconsistent samples"));
    }
    let preparation = samples
        .iter()
        .map(|sample| sample.timings.preparation)
        .collect::<Vec<_>>();
    let scoring = samples
        .iter()
        .map(|sample| sample.timings.scoring)
        .collect::<Vec<_>>();
    let materialization = samples
        .iter()
        .map(|sample| sample.timings.materialization)
        .collect::<Vec<_>>();
    let total = samples
        .iter()
        .map(|sample| {
            sample.timings.preparation + sample.timings.scoring + sample.timings.materialization
        })
        .collect::<Vec<_>>();
    println!(
        "{PREFIX}{}",
        json!({
            "kind": "case",
            "name": name,
            "objective": request.objective.as_str(),
            "weaponCandidates": first.estimate.weapon_candidates,
            "combinations": first.estimate.combinations,
            "rows": first.rows.len(),
            "preparationMedianMs": median_ms(&preparation),
            "scoringMedianMs": median_ms(&scoring),
            "materializationMedianMs": median_ms(&materialization),
            "totalMedianMs": median_ms(&total),
            "preparationSamplesMs": duration_samples_ms(&preparation),
            "scoringSamplesMs": duration_samples_ms(&scoring),
            "materializationSamplesMs": duration_samples_ms(&materialization),
            "totalSamplesMs": duration_samples_ms(&total),
        })
    );
    Ok(())
}

fn duration_samples_ms(samples: &[Duration]) -> Vec<f64> {
    samples
        .iter()
        .map(|sample| sample.as_secs_f64() * 1_000.0)
        .collect()
}

fn median_ms(samples: &[Duration]) -> f64 {
    let mut ordered = duration_samples_ms(samples);
    ordered.sort_by(f64::total_cmp);
    let middle = ordered.len() / 2;
    if ordered.len().is_multiple_of(2) {
        (ordered[middle - 1] + ordered[middle]) / 2.0
    } else {
        ordered[middle]
    }
}

fn cases() -> Vec<(&'static str, OptimizeRequest)> {
    let mut max_ar = low_level_request();
    let mut max_physical = max_ar.clone();
    max_physical.objective = OptimizeObjective::MaxPhysicalAr;
    let mut max_ar_export = max_ar.clone();
    max_ar_export.top_k = 500;
    let high_level_max_ar = high_level_request();

    let mut bleed = max_ar.clone();
    bleed.weapon_type_key = Some("Katana".to_string());
    bleed.objective = OptimizeObjective::MaxArPlusBleed;

    let mut first_hit = high_level_request();
    first_hit.weapon_name = Some("Sword Lance".to_string());
    first_hit.affinity = Some("Magic".to_string());
    first_hit.aow_name = Some("Glintstone Pebble".to_string());
    first_hit.objective = OptimizeObjective::AowFirstHit;
    let mut full_sequence = first_hit.clone();
    full_sequence.objective = OptimizeObjective::AowFullSequence;

    max_ar.objective = OptimizeObjective::MaxAr;
    vec![
        ("open-ranking-max-ar", max_ar),
        ("open-ranking-physical", max_physical),
        ("open-ranking-max-ar-export-500", max_ar_export),
        ("open-ranking-max-ar-high-level", high_level_max_ar),
        ("katana-bleed", bleed),
        ("fixed-aow-first-hit", first_hit),
        ("fixed-aow-full-sequence", full_sequence),
    ]
}

fn low_level_request() -> OptimizeRequest {
    OptimizeRequest {
        class_name: "Samurai".to_string(),
        character_level: 46,
        current_stats: Stats {
            vig: 12,
            mnd: 11,
            end: 13,
            str: 12,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 8,
        },
        min_combat_stats: [0; 5],
        locked_combat_stats: [None; 5],
        standard_max_upgrade: 25,
        somber_max_upgrade: 10,
        exact_upgrade: true,
        two_handing: false,
        dlc_scaling: false,
        scadutree_level: 0,
        weapon_name: None,
        affinity: None,
        aow_name: None,
        weapon_type_key: None,
        somber_filter: SomberFilter::All,
        objective: OptimizeObjective::MaxAr,
        top_k: 5,
    }
}

fn high_level_request() -> OptimizeRequest {
    OptimizeRequest {
        class_name: "Samurai".to_string(),
        character_level: 150,
        current_stats: Stats {
            vig: 40,
            mnd: 20,
            end: 20,
            str: 21,
            dex: 15,
            int: 20,
            fai: 20,
            arc: 8,
        },
        min_combat_stats: [0; 5],
        locked_combat_stats: [None; 5],
        standard_max_upgrade: 25,
        somber_max_upgrade: 10,
        exact_upgrade: true,
        two_handing: false,
        dlc_scaling: false,
        scadutree_level: 0,
        weapon_name: None,
        affinity: None,
        aow_name: None,
        weapon_type_key: None,
        somber_filter: SomberFilter::All,
        objective: OptimizeObjective::MaxAr,
        top_k: 5,
    }
}
