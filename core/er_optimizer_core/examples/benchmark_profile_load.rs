use std::time::Instant;

use er_optimizer_core::{
    CONVERGENCE_PROFILE_ID, VANILLA_PROFILE_ID, load_embedded_game_profile_with_manifest,
};

const USAGE: &str = "usage: benchmark_profile_load <repeats>\n\
                     Times loading both embedded profiles one after the other and side by side.";

fn load(profile_id: &str) -> Result<usize, String> {
    let (data, _) = load_embedded_game_profile_with_manifest(profile_id)?;
    Ok(data.weapons.len() + data.aows.len())
}

fn main() -> Result<(), String> {
    let repeats: usize = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .filter(|&repeats| repeats > 0)
        .ok_or_else(|| USAGE.to_string())?;
    let profiles = [VANILLA_PROFILE_ID, CONVERGENCE_PROFILE_ID];
    let mut sequential = Vec::with_capacity(repeats);
    let mut parallel = Vec::with_capacity(repeats);
    for repeat in 0..=repeats {
        let started = Instant::now();
        let mut rows = 0;
        for profile in profiles {
            rows += load(profile)?;
        }
        let sequential_ms = started.elapsed().as_secs_f64() * 1e3;

        let started = Instant::now();
        let parallel_rows: usize = std::thread::scope(|scope| {
            let workers = profiles.map(|profile| scope.spawn(move || load(profile)));
            workers
                .into_iter()
                .map(|worker| worker.join().expect("profile load thread"))
                .sum::<Result<usize, String>>()
        })?;
        let parallel_ms = started.elapsed().as_secs_f64() * 1e3;
        assert_eq!(rows, parallel_rows);
        if repeat > 0 {
            sequential.push(sequential_ms);
            parallel.push(parallel_ms);
        }
    }
    sequential.sort_by(f64::total_cmp);
    parallel.sort_by(f64::total_cmp);
    println!(
        "sequential min={:.1}ms median={:.1}ms | side by side min={:.1}ms median={:.1}ms",
        sequential[0],
        sequential[repeats / 2],
        parallel[0],
        parallel[repeats / 2],
    );
    Ok(())
}
