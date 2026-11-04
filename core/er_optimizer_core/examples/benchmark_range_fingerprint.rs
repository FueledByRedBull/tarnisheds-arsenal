use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Instant;

use er_optimizer_core::{
    CUSTOM_STATS_CLASS_NAME, OptimizeObjective, OptimizeRequest, ResultGrouping, SomberFilter,
    Stats, load_game_data_with_manifest, optimize_level_ranges_with_progress,
};

const USAGE: &str = "usage: benchmark_range_fingerprint <vanilla|convergence> <maxar|bleed|first|sequence> <weapon> <aow|-> <horizon> <repeats>\n\
                     Underscores in <weapon> and <aow> stand for spaces.";

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [profile, objective, weapon, aow, horizon, repeats] = args.as_slice() else {
        return Err(USAGE.to_string());
    };
    let weapon = weapon.replace('_', " ");
    let aow = aow.replace('_', " ");
    let horizon: u16 = horizon.parse().map_err(|_| USAGE.to_string())?;
    let repeats: usize = repeats.parse().map_err(|_| USAGE.to_string())?;
    let (directory, custom) = match profile.as_str() {
        "vanilla" => ("phase1", false),
        "convergence" => ("profiles/convergence", true),
        _ => return Err(USAGE.to_string()),
    };
    let objective = match objective.as_str() {
        "maxar" => OptimizeObjective::MaxAr,
        "bleed" => OptimizeObjective::BleedThenAr,
        "first" => OptimizeObjective::AowFirstHit,
        "sequence" => OptimizeObjective::AowFullSequence,
        _ => return Err(USAGE.to_string()),
    };
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(directory);
    let (data, _) = load_game_data_with_manifest(data_dir)?;

    let mut affinities: Vec<String> = data
        .weapons
        .iter()
        .filter(|candidate| candidate.name == weapon)
        .map(|candidate| candidate.affinity.clone())
        .collect();
    affinities.sort();
    affinities.dedup();
    if affinities.is_empty() {
        return Err(format!("unknown weapon {weapon}"));
    }

    let request = |affinity: &str| OptimizeRequest {
        class_name: if custom {
            CUSTOM_STATS_CLASS_NAME.to_string()
        } else {
            "Samurai".to_string()
        },
        character_level: if custom { 159 } else { 80 },
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
        standard_max_upgrade: data.rules.standard_max_upgrade,
        somber_max_upgrade: data.rules.somber_max_upgrade,
        exact_upgrade: true,
        two_handing: false,
        dlc_scaling: false,
        scadutree_level: 0,
        weapon_name: Some(weapon.clone()),
        affinity: Some(affinity.to_string()),
        aow_name: (aow != "-").then(|| aow.clone()),
        weapon_type_key: None,
        somber_filter: SomberFilter::All,
        filters: Vec::new(),
        result_grouping: ResultGrouping::Automatic,
        objective,
        top_k: 1,
    };
    let requests: Vec<_> = affinities
        .iter()
        .map(|affinity| request(affinity))
        .collect();
    let first_level = requests[0].character_level;
    let levels: Vec<u16> = (0..=horizon).map(|offset| first_level + offset).collect();

    let mut samples = Vec::with_capacity(repeats);
    let mut fingerprint = String::new();
    for repeat in 0..=repeats {
        let started = Instant::now();
        let results =
            optimize_level_ranges_with_progress(&requests, &levels, &data, |_, _| true, || true);
        let elapsed = started.elapsed().as_secs_f64() * 1e3;
        let rendered = format!("{results:?}");
        if repeat == 0 {
            fingerprint = rendered;
        } else {
            samples.push(elapsed);
            assert_eq!(fingerprint, rendered, "result changed between repeats");
        }
    }
    samples.sort_by(f64::total_cmp);
    let solved = fingerprint.matches("Ok(").count();
    if solved == 0 {
        return Err(format!(
            "no solved range: {}",
            &fingerprint[..fingerprint.len().min(300)]
        ));
    }
    println!(
        "{profile} {objective:?} {weapon} {aow} h={horizon} affinities={} solved={solved} median={:.1}ms min={:.1} max={:.1} fp_len={} fp_sha256={:x}",
        affinities.len(),
        samples[samples.len() / 2],
        samples[0],
        samples[samples.len() - 1],
        fingerprint.len(),
        Sha256::digest(fingerprint.as_bytes()),
    );
    Ok(())
}
