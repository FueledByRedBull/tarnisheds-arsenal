use std::path::Path;

use crate::data::load_game_data;

use super::*;

fn load_data() -> GameData {
    let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .join("phase1");
    load_game_data(data_path).expect("failed to load phase1 data")
}

fn load_convergence_data() -> GameData {
    let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .join("profiles")
        .join("convergence");
    load_game_data(data_path).expect("failed to load Convergence data")
}

#[test]
fn objective_aliases_parse_to_canonical_variants() {
    assert_eq!(
        OptimizeObjective::parse("max_phys_ar").unwrap(),
        OptimizeObjective::MaxPhysicalAr
    );
    assert_eq!(
        OptimizeObjective::parse("max_ar+bleed").unwrap(),
        OptimizeObjective::MaxArPlusBleed
    );
    assert_eq!(
        OptimizeObjective::parse("aow_full").unwrap().as_str(),
        "aow_full_sequence"
    );
    assert!(OptimizeObjective::parse("not_real").is_err());
}

#[test]
fn profile_capabilities_reject_unverified_aow_objectives() {
    let data = load_convergence_data();
    let mut request = base_request();
    request.standard_max_upgrade = 15;
    request.somber_max_upgrade = 15;
    request.objective = OptimizeObjective::AowFirstHit;
    let error = prepare_search(&request, &data).expect_err("AoW damage must be disabled");
    assert!(error.contains("The Convergence"));
    assert!(error.contains("verified Ash of War damage"));

    request.objective = OptimizeObjective::AowFullSequence;
    let error = prepare_search(&request, &data).expect_err("AoW routes must be disabled");
    assert!(error.contains("verified Ash of War route"));
}

#[test]
fn convergence_profile_rules_reject_vanilla_upgrade_and_scadutree_inputs() {
    let data = load_convergence_data();
    let mut request = base_request();
    let error = prepare_search(&request, &data).expect_err("+25 must be rejected");
    assert!(error.contains("only through +15"));

    request.standard_max_upgrade = 15;
    request.somber_max_upgrade = 15;
    request.dlc_scaling = true;
    request.scadutree_level = 20;
    let error = prepare_search(&request, &data).expect_err("Scadutree must be rejected");
    assert!(error.contains("does not use Scadutree"));
}

#[test]
fn convergence_ammunition_weapons_are_not_ranked_without_an_ammo_model() {
    let data = load_convergence_data();
    let ranged = data
        .weapons
        .iter()
        .find(|weapon| weapon.weapon_type_name == "Greatbow")
        .expect("Convergence greatbow");
    let melee = data
        .weapons
        .iter()
        .find(|weapon| weapon.weapon_type_name == "Twinblade")
        .expect("Convergence twinblade");
    assert!(!data.weapon_ar_supported(ranged));
    assert!(data.weapon_ar_supported(melee));

    let mut request = base_request();
    request.weapon_name = Some(ranged.name.clone());
    request.affinity = Some(ranged.affinity.clone());
    request.standard_max_upgrade = 15;
    request.somber_max_upgrade = 15;
    request.exact_upgrade = true;
    let plan = prepare_search(&request, &data).expect("supported Convergence request shape");
    assert_eq!(plan.estimate().weapon_candidates, 0);
}

#[test]
fn convergence_galvanic_optimizer_uses_str_dex_int_without_arcane_fill() {
    let data = load_convergence_data();
    let mut request = base_request();
    request.character_level = 107;
    request.weapon_name = Some("Galvanic Culling Blade [Twinblade]".to_string());
    request.affinity = Some("Standard".to_string());
    request.standard_max_upgrade = 13;
    request.somber_max_upgrade = 13;
    request.exact_upgrade = true;
    request.top_k = 3;

    let rows = optimize(&request, &data).expect("Convergence Galvanic search");
    let best = rows.first().expect("Galvanic result");
    assert_eq!(best.stats.arc, 8, "Arcane does not scale this weapon");
    assert!(best.stats.str > 12 || best.stats.dex > 15 || best.stats.int > 35);
    assert!(
        best.ar.lightning > 280.0,
        "attribute scaling must add lightning AR"
    );
    assert_eq!(best.weapon_type_name, "Twinblade");
    assert!((best.effective_scaling[STAT_INT] - 2.277).abs() < 0.001);
    assert_eq!(best.requirements, [15, 12, 35, 0, 0]);
}

#[test]
fn sleep_and_madness_survive_result_materialization() {
    let data = load_data();
    let cases = [("Sword of St. Trina", true), ("Vyke's War Spear", false)];

    for (weapon_name, expects_sleep) in cases {
        let mut request = base_request();
        request.character_level = 150;
        request.weapon_name = Some(weapon_name.to_string());
        request.affinity = Some("Standard".to_string());
        request.exact_upgrade = true;
        request.top_k = 1;

        let rows = optimize(&request, &data).expect("status weapon search");
        let result = rows.first().expect("status weapon result");
        if expects_sleep {
            assert!(
                result.sleep_buildup > 0.0,
                "{weapon_name} sleep was discarded"
            );
            assert_eq!(result.madness_buildup, 0.0);
        } else {
            assert!(
                result.madness_buildup > 0.0,
                "{weapon_name} madness was discarded"
            );
            assert_eq!(result.sleep_buildup, 0.0);
        }
    }
}

#[test]
fn fixed_native_skill_falls_back_to_generic_rows_by_skill_id() {
    let data = load_data();
    let weapon = data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Carian Knight's Sword" && weapon.affinity == "Standard")
        .expect("Carian Knight's Sword");
    assert!(data.native_skill_attack_rows(weapon.weapon_id).is_empty());

    let choice = native_skill_choice_for_weapon(weapon, &data, OptimizeObjective::AowFirstHit)
        .expect("native skill choice");
    assert_eq!(choice.skill_name, Some("Carian Grandeur"));
    assert!(!choice.attack_rows.is_empty());
    assert!(
        choice
            .attack_rows
            .iter()
            .all(|row| Some(row.aow_id) == weapon.native_skill_id)
    );
}

fn test_result(
    weapon_id: u32,
    upgrade: u8,
    physical_ar: f32,
    bleed_buildup: f32,
) -> OptimizeResult {
    OptimizeResult {
        weapon_id,
        weapon_name: format!("Test Weapon {weapon_id}"),
        weapon_type_name: "Test Weapon Type".to_string(),
        affinity: "Standard".to_string(),
        is_somber: false,
        upgrade,
        stats: Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 10,
            dex: 10,
            int: 10,
            fai: 10,
            arc: 10,
        },
        requirements: [0; COMBAT_STAT_COUNT],
        effective_scaling: [0.0; COMBAT_STAT_COUNT],
        ar: DamageBreakdown {
            physical: physical_ar,
            magic: 0.0,
            fire: 0.0,
            lightning: 0.0,
            holy: 0.0,
        },
        aow_id: None,
        aow_name: None,
        bleed_buildup,
        bleed_buildup_add: 0.0,
        frost_buildup: 0.0,
        poison_buildup: 0.0,
        scarlet_rot_buildup: 0.0,
        sleep_buildup: 0.0,
        madness_buildup: 0.0,
        death_buildup: 0.0,
        aow_first_hit_damage: 0.0,
        aow_full_sequence_damage: 0.0,
        aow_route: None,
        score: bleed_buildup,
    }
}

fn scored_candidate(
    prepared_idx: usize,
    aow_idx: usize,
    upgrade: u8,
    stats: Stats,
    score: f32,
) -> ScoredCandidate {
    ScoredCandidate {
        prepared_idx,
        aow_idx,
        upgrade,
        stats,
        metric: CandidateMetric {
            score,
            ar: None,
            status_buildup: None,
            bleed_buildup: None,
            aow_first_hit_damage: None,
            aow_full_sequence_damage: None,
        },
    }
}

fn base_request() -> OptimizeRequest {
    OptimizeRequest {
        class_name: "Samurai".to_string(),
        character_level: 9,
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
        min_combat_stats: [0, 0, 0, 0, 0],
        locked_combat_stats: [None, None, None, None, None],
        standard_max_upgrade: 25,
        somber_max_upgrade: 10,
        exact_upgrade: false,
        two_handing: false,
        dlc_scaling: false,
        scadutree_level: 0,
        weapon_name: Some("Uchigatana".to_string()),
        affinity: Some("Keen".to_string()),
        aow_name: None,
        weapon_type_key: None,
        somber_filter: SomberFilter::All,
        objective: OptimizeObjective::MaxAr,
        top_k: 3,
    }
}

fn broad_request() -> OptimizeRequest {
    OptimizeRequest {
        class_name: "Samurai".to_string(),
        character_level: 150,
        current_stats: Stats {
            vig: 40,
            mnd: 20,
            end: 20,
            str: 20,
            dex: 20,
            int: 20,
            fai: 20,
            arc: 20,
        },
        min_combat_stats: [0, 0, 0, 0, 0],
        locked_combat_stats: [None, None, None, None, None],
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
        top_k: 3,
    }
}

fn active_mask_for(
    game_data: &GameData,
    weapon_name: &str,
    affinity: &str,
    objective: OptimizeObjective,
    aow_name: Option<&str>,
) -> [bool; COMBAT_STAT_COUNT] {
    let mut request = broad_request();
    request.weapon_name = Some(weapon_name.to_string());
    request.affinity = Some(affinity.to_string());
    request.aow_name = aow_name.map(str::to_string);
    request.objective = objective;
    let constraints = build_combat_constraints(&request).expect("constraints failed");
    let prepared_weapons =
        prepare_weapons(&request, game_data, constraints).expect("prepare failed");
    let prepared = prepared_weapons.first().expect("expected prepared weapon");
    active_stats_for_choice(
        &request,
        prepared,
        prepared.aow_choices.first().expect("expected AoW choice"),
        game_data,
    )
}

#[test]
fn optimize_returns_sorted_top_results_for_locked_weapon() {
    let game_data = load_data();
    let request = base_request();
    let results = optimize(&request, &game_data).expect("optimizer failed");

    assert!(!results.is_empty());
    assert!(
        results
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score)
    );
    assert!(
        results
            .iter()
            .all(|result| result.weapon_name == "Uchigatana")
    );
    assert!(results.iter().all(|result| result.affinity == "Keen"));
    assert!(results.iter().all(|result| result.upgrade <= 25));
}

#[test]
fn dynamic_ar_search_matches_exhaustive_search() {
    let game_data = load_data();
    for objective in [OptimizeObjective::MaxAr, OptimizeObjective::MaxPhysicalAr] {
        let mut request = base_request();
        request.character_level = 60;
        request.weapon_name = Some("Uchigatana".to_string());
        request.affinity = Some("Blood".to_string());
        request.aow_name = Some("Seppuku".to_string());
        request.standard_max_upgrade = 18;
        request.exact_upgrade = true;
        request.objective = objective;
        request.top_k = 5;
        let plan = prepare_search(&request, &game_data).expect("prepare AR comparison");
        let unit = *plan.serial_work_units.first().expect("AR work unit");
        let mut dynamic_progress =
            SerialSearchProgress::new(unit.candidate_count, 0, |_snapshot| true);
        let dynamic = search_ar_work_unit(
            &plan,
            unit,
            result_group_mode(&request),
            &mut dynamic_progress,
        )
        .expect("dynamic AR search");
        let mut exhaustive_progress =
            SerialSearchProgress::new(unit.candidate_count, 0, |_snapshot| true);
        let exhaustive = search_work_unit_exhaustive(
            &plan,
            unit,
            result_group_mode(&request),
            &mut exhaustive_progress,
        )
        .expect("exhaustive AR search");

        assert_eq!(dynamic.len(), exhaustive.len());
        for (fast, reference) in dynamic.iter().zip(exhaustive.iter()) {
            assert_eq!(fast.upgrade, reference.upgrade);
            assert_eq!(fast.stats, reference.stats);
            assert_eq!(fast.metric.score, reference.metric.score);
            let fast_ar = fast.metric.ar.expect("dynamic AR metric");
            let reference_ar = reference.metric.ar.expect("exhaustive AR metric");
            assert_eq!(fast_ar.physical, reference_ar.physical);
            assert_eq!(fast_ar.magic, reference_ar.magic);
            assert_eq!(fast_ar.fire, reference_ar.fire);
            assert_eq!(fast_ar.lightning, reference_ar.lightning);
            assert_eq!(fast_ar.holy, reference_ar.holy);
        }
    }
}

#[test]
fn vanilla_export_sized_search_handles_unsupported_utility_effect_rows() {
    let game_data = load_data();
    let mut request = broad_request();
    request.character_level = 180;
    request.two_handing = true;
    request.top_k = 500;

    let rows = optimize(&request, &game_data).expect("top-500 Vanilla export search");
    assert!(!rows.is_empty());
    assert!(rows.len() <= 500);
}

#[test]
fn profiled_optimizer_preserves_results_and_reports_all_phases() {
    let game_data = load_data();
    let request = base_request();
    let expected = optimize(&request, &game_data).expect("ordinary optimizer succeeds");
    let profiled = optimize_profiled(&request, &game_data).expect("profiled optimizer succeeds");

    assert_eq!(profiled.rows.len(), expected.len());
    assert_eq!(profiled.estimate.weapon_candidates, 1);
    for (actual, expected) in profiled.rows.iter().zip(expected.iter()) {
        assert_eq!(actual.weapon_id, expected.weapon_id);
        assert_eq!(actual.aow_id, expected.aow_id);
        assert_eq!(actual.upgrade, expected.upgrade);
        assert_eq!(actual.stats, expected.stats);
        assert!((actual.score - expected.score).abs() < 0.001);
    }
    assert!(profiled.timings.preparation > Duration::ZERO);
    assert!(profiled.timings.scoring > Duration::ZERO);
    assert!(profiled.timings.materialization > Duration::ZERO);
}

#[test]
fn level_range_matches_independent_exact_searches() {
    let game_data = load_data();
    let mut request = base_request();
    request.top_k = 1;
    request.exact_upgrade = true;
    let levels = [12, 9, 10, 12];
    let mut completed = Vec::new();
    let ranged = optimize_level_range_with_progress(
        &request,
        &levels,
        &game_data,
        |level| {
            completed.push(level);
            true
        },
        || true,
    )
    .expect("level range succeeds");

    assert_eq!(completed, vec![9, 10, 12]);
    assert_eq!(
        ranged.iter().map(|entry| entry.level).collect::<Vec<_>>(),
        vec![9, 10, 12]
    );
    for entry in ranged {
        let mut independent_request = request.clone();
        independent_request.character_level = entry.level;
        let expected =
            optimize(&independent_request, &game_data).expect("independent level succeeds");
        assert_eq!(entry.rows.len(), expected.len());
        for (actual, expected) in entry.rows.iter().zip(expected.iter()) {
            assert_eq!(actual.weapon_id, expected.weapon_id);
            assert_eq!(actual.affinity, expected.affinity);
            assert_eq!(actual.upgrade, expected.upgrade);
            assert_eq!(actual.stats.combat_array(), expected.stats.combat_array());
            assert!((actual.score - expected.score).abs() < 0.001);
        }
    }
}

#[test]
fn level_range_honors_cancellation_before_preparation() {
    let game_data = load_data();
    let error = optimize_level_range_with_progress(
        &base_request(),
        &[9, 10],
        &game_data,
        |_| true,
        || false,
    )
    .expect_err("cancelled range must fail closed");
    assert_eq!(error, "cancelled");
}

#[test]
fn reusable_loadout_evaluator_matches_independent_exact_search() {
    let game_data = load_data();
    let mut template = base_request();
    template.character_level = 12;
    template.exact_upgrade = true;
    template.top_k = 1;
    let evaluator = prepare_loadout_evaluator_with_cancel(&template, &game_data, || true)
        .expect("loadout preparation succeeds");

    let mut exact = template.clone();
    exact.locked_combat_stats = [Some(12), Some(18), Some(9), Some(8), Some(8)];
    let reused = evaluator
        .evaluate_with_cancel(&exact, || true)
        .expect("reused evaluation succeeds");
    let independent = optimize(&exact, &game_data).expect("independent evaluation succeeds");

    assert_eq!(reused.len(), independent.len());
    assert_eq!(reused[0].weapon_id, independent[0].weapon_id);
    assert_eq!(reused[0].upgrade, independent[0].upgrade);
    assert_eq!(
        reused[0].stats.combat_array(),
        independent[0].stats.combat_array()
    );
    assert!((reused[0].score - independent[0].score).abs() < 0.001);
}

#[test]
fn reusable_loadout_evaluator_rejects_loadout_drift() {
    let game_data = load_data();
    let mut template = base_request();
    template.exact_upgrade = true;
    let evaluator = prepare_loadout_evaluator_with_cancel(&template, &game_data, || true)
        .expect("loadout preparation succeeds");
    let mut changed = template;
    changed.affinity = Some("Heavy".to_string());

    assert_eq!(
        evaluator
            .evaluate_with_cancel(&changed, || true)
            .expect_err("loadout drift must fail"),
        "request does not match the prepared loadout evaluator"
    );
}

fn lock_request_to_combat_stats(request: &mut OptimizeRequest, stats: Stats) {
    request.min_combat_stats = [0; COMBAT_STAT_COUNT];
    request.locked_combat_stats = stats.combat_array().map(Some);
}

fn assert_upgrade_series_matches_independent_searches(
    request: &OptimizeRequest,
    game_data: &GameData,
    expected_upgrades: &[u8],
) {
    let evaluator = prepare_upgrade_series_evaluator_with_cancel(request, game_data, || true)
        .expect("upgrade-series preparation succeeds");
    let rows = evaluator
        .evaluate_with_cancel(request, 25, || true)
        .expect("upgrade-series evaluation succeeds");
    assert_eq!(
        rows.iter().map(|row| row.upgrade).collect::<Vec<_>>(),
        expected_upgrades
    );
    for row in rows {
        let mut independent_request = request.clone();
        independent_request.exact_upgrade = true;
        if row.is_somber {
            independent_request.somber_max_upgrade = row.upgrade;
        } else {
            independent_request.standard_max_upgrade = row.upgrade;
        }
        let expected = optimize(&independent_request, game_data)
            .expect("independent upgrade succeeds")
            .pop()
            .expect("independent upgrade returns a row");
        assert_eq!(row.weapon_id, expected.weapon_id);
        assert_eq!(row.aow_id, expected.aow_id);
        assert_eq!(row.upgrade, expected.upgrade);
        assert_eq!(row.stats, expected.stats);
        assert!((row.score - expected.score).abs() < 0.001);
        assert!((row.ar.total() - expected.ar.total()).abs() < 0.001);
    }
}

#[test]
fn direct_standard_upgrade_series_matches_independent_searches() {
    let game_data = load_data();
    let mut request = base_request();
    request.exact_upgrade = false;
    request.top_k = 1;
    let stats = request.current_stats;
    lock_request_to_combat_stats(&mut request, stats);
    assert_upgrade_series_matches_independent_searches(
        &request,
        &game_data,
        &(0_u8..=25).collect::<Vec<_>>(),
    );
}

#[test]
fn direct_upgrade_series_preserves_sparse_reinforcement_levels() {
    let mut game_data = load_data();
    let mut request = base_request();
    request.exact_upgrade = false;
    request.top_k = 1;
    let stats = request.current_stats;
    lock_request_to_combat_stats(&mut request, stats);
    let reinforce_type = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
        .expect("test weapon exists")
        .reinforce_type;
    game_data.reinforce[usize::from(reinforce_type)][7] = None;
    let expected = (0_u8..=25)
        .filter(|upgrade| *upgrade != 7)
        .collect::<Vec<_>>();
    assert_upgrade_series_matches_independent_searches(&request, &game_data, &expected);
}

#[test]
fn direct_somber_native_skill_series_matches_independent_searches() {
    let game_data = load_data();
    let mut request = broad_request();
    request.weapon_name = Some("Bloodhound's Fang".to_string());
    request.affinity = Some("Standard".to_string());
    request.aow_name = Some("Bloodhound's Finesse".to_string());
    request.objective = OptimizeObjective::AowFullSequence;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.top_k = 1;
    let solved = optimize(&request, &game_data)
        .expect("somber seed search succeeds")
        .pop()
        .expect("somber seed row exists");
    request.exact_upgrade = false;
    lock_request_to_combat_stats(&mut request, solved.stats);
    assert_upgrade_series_matches_independent_searches(
        &request,
        &game_data,
        &(0_u8..=10).collect::<Vec<_>>(),
    );
}

#[test]
fn direct_upgrade_series_honors_cancellation_and_rejects_identity_drift() {
    let game_data = load_data();
    let mut request = base_request();
    request.exact_upgrade = false;
    request.top_k = 1;
    let stats = request.current_stats;
    lock_request_to_combat_stats(&mut request, stats);
    let evaluator = prepare_upgrade_series_evaluator_with_cancel(&request, &game_data, || true)
        .expect("upgrade-series preparation succeeds");
    assert_eq!(
        evaluator
            .evaluate_with_cancel(&request, 25, || false)
            .expect_err("cancelled series must fail closed"),
        "cancelled"
    );

    let mut changed = request;
    changed.objective = OptimizeObjective::MaxPhysicalAr;
    assert_eq!(
        evaluator
            .evaluate_with_cancel(&changed, 25, || true)
            .expect_err("identity drift must fail closed"),
        "request does not match the prepared loadout evaluator"
    );
}

#[test]
fn distribution_counter_matches_brute_force_property_corpus() {
    fn brute(caps: &[u8], index: usize, remaining: u16) -> u64 {
        if index == caps.len() {
            return u64::from(remaining == 0);
        }
        (0..=u16::from(caps[index]).min(remaining))
            .map(|value| brute(caps, index + 1, remaining - value))
            .sum()
    }

    for seed in 0_u16..192 {
        let caps = [
            (seed % 7) as u8,
            (seed.wrapping_mul(3) % 8) as u8,
            (seed.wrapping_mul(5) % 6) as u8,
            (seed.wrapping_mul(7) % 5) as u8,
            (seed.wrapping_mul(11) % 4) as u8,
        ];
        let capacity = caps.iter().map(|value| u16::from(*value)).sum::<u16>();
        let remaining = if capacity == 0 {
            0
        } else {
            seed % (capacity + 1)
        };
        let actual = count_distributions(&caps, 0, remaining, &mut HashMap::new());
        assert_eq!(actual, brute(&caps, 0, remaining), "seed {seed}");
    }
}

#[test]
fn grouped_top_k_remains_bounded_unique_and_sorted() {
    let mut rows = Vec::new();
    for index in 0_u32..500 {
        let weapon_id = index % 17;
        let score = ((index.wrapping_mul(37)) % 101) as f32;
        push_top_k(
            &mut rows,
            test_result(weapon_id, (index % 26) as u8, score, score % 12.0),
            9,
            ResultGroupMode::WeaponOnly,
        );
        assert!(rows.len() <= 9);
    }
    assert!(
        rows.windows(2)
            .all(|pair| better_result(&pair[0], &pair[1]))
    );
    let unique = rows.iter().map(|row| row.weapon_id).collect::<HashSet<_>>();
    assert_eq!(unique.len(), rows.len());
}

#[test]
fn max_physical_ar_scores_the_physical_ar_component() {
    let game_data = load_data();
    let mut request = base_request();
    request.objective = OptimizeObjective::MaxPhysicalAr;
    request.affinity = Some("Heavy".to_string());
    request.aow_name = Some("Seppuku".to_string());
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert!((results[0].score - results[0].ar.physical).abs() < 0.001);
}

#[test]
fn scadutree_scaling_multiplies_outgoing_damage_only() {
    let game_data = load_data();
    let mut base = base_request();
    base.affinity = Some("Blood".to_string());
    base.aow_name = Some("Seppuku".to_string());
    base.standard_max_upgrade = 25;
    base.somber_max_upgrade = 10;
    base.exact_upgrade = true;
    base.top_k = 1;

    let mut scaled = base.clone();
    scaled.dlc_scaling = true;
    scaled.scadutree_level = 20;

    let base_result = optimize(&base, &game_data)
        .expect("base optimizer failed")
        .pop()
        .expect("expected base result");
    let scaled_result = optimize(&scaled, &game_data)
        .expect("scaled optimizer failed")
        .pop()
        .expect("expected scaled result");

    assert!((scaled_result.ar.total() - base_result.ar.total() * 2.05).abs() < 0.1);
    assert!(
        (scaled_result.aow_first_hit_damage - base_result.aow_first_hit_damage * 2.05).abs() < 0.1
    );
    assert!((scaled_result.bleed_buildup - base_result.bleed_buildup).abs() < 0.001);
}

#[test]
fn scadutree_curve_uses_patch_1122_values() {
    assert!((crate::math::scadutree_attack_multiplier(true, 1) - 1.10).abs() < f32::EPSILON);
    assert!((crate::math::scadutree_attack_multiplier(true, 12) - 1.85).abs() < f32::EPSILON);
    assert!((crate::math::scadutree_attack_multiplier(true, 20) - 2.05).abs() < f32::EPSILON);
    assert!((crate::math::scadutree_attack_multiplier(false, 20) - 1.0).abs() < f32::EPSILON);
    assert!((crate::math::scadutree_damage_negation(true, 20) - (1.0 - 1.0 / 2.05)).abs() < 0.0001);
}

#[test]
fn optimize_errors_when_stats_exceed_level_budget() {
    let game_data = load_data();
    let mut request = base_request();
    request.current_stats.str = 40;
    request.current_stats.dex = 40;

    let err = optimize(&request, &game_data).expect_err("expected budget error");
    assert!(err.contains("level budget"));
}

#[test]
fn optimize_respects_weapon_type_filter() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = None;
    request.affinity = None;
    request.weapon_type_key = Some("Katana".to_string());
    request.top_k = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    for result in &results {
        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| {
                weapon.weapon_id == result.weapon_id && weapon.affinity == result.affinity
            })
            .expect("missing weapon");
        assert!(weapon.weapon_type_name.eq_ignore_ascii_case("Katana"));
    }
}

#[test]
fn open_weapon_search_returns_one_result_per_weapon() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = None;
    request.affinity = None;
    request.aow_name = None;
    request.weapon_type_key = Some("Katana".to_string());
    request.top_k = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");

    assert!(!results.is_empty());
    let unique_weapon_names = results
        .iter()
        .map(|result| result.weapon_name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    assert_eq!(unique_weapon_names.len(), results.len());
    assert_eq!(
        results
            .iter()
            .filter(|result| result.weapon_name == "Uchigatana")
            .count(),
        1
    );
}

#[test]
fn open_ar_search_excludes_compatible_aows_that_cannot_change_ar() {
    let game_data = load_data();
    let request = base_request();
    let weapon = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Blood")
        .expect("missing blood Uchigatana");

    let choices = resolve_aow_choices(weapon, &request, &game_data)
        .expect("AoW resolution failed")
        .expect("expected open AoW choices");
    let names = choices
        .iter()
        .filter_map(|choice| choice.skill_name)
        .collect::<HashSet<_>>();

    assert!(names.contains("Seppuku"));
    assert!(!names.contains("Double Slash"));
    assert!(choices.iter().all(|choice| {
        choice
            .aow
            .is_none_or(|aow| aow_affects_objective(aow, OptimizeObjective::MaxAr))
    }));
}

#[test]
fn locked_weapon_search_keeps_multiple_loadouts_for_that_weapon() {
    let game_data = load_data();
    let mut request = base_request();
    request.affinity = None;
    request.aow_name = None;
    request.top_k = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");

    assert!(results.len() > 1);
    assert!(
        results
            .iter()
            .all(|result| result.weapon_name == "Uchigatana")
    );
    let unique_loadouts = results
        .iter()
        .map(|result| (result.upgrade, result.aow_id))
        .collect::<HashSet<_>>();
    assert!(unique_loadouts.len() > 1);
}

#[test]
fn optimize_accepts_normalized_weapon_type_filter_names() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = None;
    request.affinity = None;
    request.weapon_type_key = Some("Hand-to-Hand Arts".to_string());
    request.top_k = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    for result in &results {
        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| {
                weapon.weapon_id == result.weapon_id && weapon.affinity == result.affinity
            })
            .expect("missing weapon");
        assert!(weapon.weapon_type_name.eq_ignore_ascii_case("Hand-to-Hand"));
    }
}

#[test]
fn parallel_search_matches_serial_results() {
    let game_data = load_data();
    let mut request = base_request();
    request.character_level = 46;
    request.current_stats = Stats {
        vig: 12,
        mnd: 11,
        end: 13,
        str: 12,
        dex: 15,
        int: 9,
        fai: 8,
        arc: 45,
    };
    request.weapon_name = None;
    request.affinity = None;
    request.weapon_type_key = Some("Katana".to_string());
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.top_k = 5;

    let plan = prepare_search(&request, &game_data).expect("prepare failed");
    let work_units = build_search_work_units(&plan, true);
    let total = work_units
        .iter()
        .map(|unit| unit.candidate_count)
        .sum::<u64>();

    let mut serial_progress = SerialSearchProgress::new(total, 0, |_snapshot| true);
    let serial = optimize_serial(
        &plan,
        &work_units,
        result_group_mode(&request),
        &mut serial_progress,
    )
    .expect("serial search failed");
    let parallel = optimize_parallel(
        &plan,
        &work_units,
        result_group_mode(&request),
        total,
        0,
        |_snapshot| true,
    )
    .expect("parallel search failed");

    assert_eq!(serial.len(), parallel.len());
    for (left, right) in serial.iter().zip(parallel.iter()) {
        assert_eq!(left.weapon_id, right.weapon_id);
        assert_eq!(left.aow_id, right.aow_id);
        assert_eq!(left.upgrade, right.upgrade);
        assert_eq!(left.stats, right.stats);
        assert!((left.score - right.score).abs() < 0.001);
    }
}

#[test]
fn parallel_threshold_accepts_medium_searches_when_threads_are_available() {
    let thread_count = rayon::current_num_threads();
    assert!(!should_use_parallel_search(
        PARALLEL_SEARCH_MIN_COMBINATIONS - 1,
        usize::MAX
    ));
    assert!(!should_use_parallel_search(
        PARALLEL_SEARCH_MIN_COMBINATIONS,
        1
    ));
    if thread_count > 1 {
        assert!(should_use_parallel_search(
            PARALLEL_SEARCH_MIN_COMBINATIONS,
            thread_count.min(2)
        ));
    }
}

#[test]
fn parallel_work_units_split_to_single_aow_choices() {
    let game_data = load_data();
    let mut request = base_request();
    request.aow_name = None;
    request.top_k = 5;

    let plan = prepare_search(&request, &game_data).expect("prepare failed");
    let fine = build_search_work_units(&plan, true);
    let grouped = build_search_work_units(&plan, false);

    assert!(!fine.is_empty());
    assert!(fine.iter().all(|unit| unit.aow_end - unit.aow_start == 1));
    assert_eq!(
        fine.iter().map(|unit| unit.candidate_count).sum::<u64>(),
        grouped.iter().map(|unit| unit.candidate_count).sum::<u64>()
    );
}

#[test]
fn prepared_plan_groups_aows_that_share_one_stat_search() {
    let game_data = load_data();
    let mut request = base_request();
    request.character_level = 46;
    request.current_stats.arc = 8;
    request.weapon_name = None;
    request.affinity = None;
    request.weapon_type_key = Some("Katana".to_string());
    request.exact_upgrade = true;
    request.objective = OptimizeObjective::MaxArPlusBleed;
    request.top_k = 5;

    let plan = prepare_search(&request, &game_data).expect("prepare failed");
    assert!(
        plan.groups.iter().any(|group| group.aow_indices.len() > 1),
        "expected compatible AoWs with the same relevant-stat search to share a group"
    );
    let grouped = build_search_work_units(&plan, false);
    assert!(grouped.iter().any(|unit| unit.aow_end - unit.aow_start > 1));
    assert_eq!(
        grouped.iter().map(|unit| unit.candidate_count).sum::<u64>(),
        plan.estimate().combinations
    );
    let rows = optimize_prepared_with_progress(&plan, 0, |_| true).expect("prepared search failed");
    assert!(!rows.is_empty());
}

#[test]
fn scored_top_k_hard_bounds_cutoff_score_ties_deterministically() {
    let game_data = load_data();
    let request = base_request();
    let constraints = build_combat_constraints(&request).expect("constraints failed");
    let weapons = prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
    let stats = request.current_stats;
    let mut candidates = Vec::new();

    for index in 0..10_000 {
        let upgrade = (index % 26) as u8;
        let mut stats = stats;
        stats.str = 12 + ((index / 26) % 8) as u8;
        push_scored_top_k(
            &mut candidates,
            scored_candidate(0, 0, upgrade, stats, 13.0),
            &request,
            &game_data,
            &weapons,
            ResultGroupMode::Loadout,
            1,
        )
        .expect("candidate ranking succeeds");
        assert!(candidates.len() <= SCORED_TOP_K_LOADOUT_OVERSAMPLE);
    }

    assert_eq!(candidates.len(), 8);
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.upgrade)
            .collect::<Vec<_>>(),
        vec![25, 24, 23, 22, 21, 20, 19, 18]
    );

    let mut reversed = Vec::new();
    for index in (0..10_000).rev() {
        let upgrade = (index % 26) as u8;
        let mut stats = stats;
        stats.str = 12 + ((index / 26) % 8) as u8;
        push_scored_top_k(
            &mut reversed,
            scored_candidate(0, 0, upgrade, stats, 13.0),
            &request,
            &game_data,
            &weapons,
            ResultGroupMode::Loadout,
            1,
        )
        .expect("candidate ranking succeeds");
    }
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| (candidate.upgrade, candidate.stats.combat_array()))
            .collect::<Vec<_>>(),
        reversed
            .iter()
            .map(|candidate| (candidate.upgrade, candidate.stats.combat_array()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn scored_top_k_replaces_lower_score_for_same_loadout() {
    let game_data = load_data();
    let request = base_request();
    let constraints = build_combat_constraints(&request).expect("constraints failed");
    let weapons = prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
    let stats = request.current_stats;
    let mut candidates = Vec::new();

    push_scored_top_k(
        &mut candidates,
        scored_candidate(0, 0, 25, stats, 10.0),
        &request,
        &game_data,
        &weapons,
        ResultGroupMode::Loadout,
        3,
    )
    .expect("candidate ranking succeeds");
    push_scored_top_k(
        &mut candidates,
        scored_candidate(0, 0, 25, stats, 20.0),
        &request,
        &game_data,
        &weapons,
        ResultGroupMode::Loadout,
        3,
    )
    .expect("candidate ranking succeeds");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].metric.score, 20.0);
}

#[test]
fn scored_top_k_discards_equal_score_candidates_with_lower_known_ar() {
    let game_data = load_data();
    let request = base_request();
    let constraints = build_combat_constraints(&request).expect("constraints failed");
    let weapons = prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
    let stats = request.current_stats;
    let mut candidates = Vec::new();

    for physical in [100.0, 200.0, 150.0] {
        let mut candidate = scored_candidate(0, 0, 25, stats, 80.0);
        candidate.metric.ar = Some(DamageBreakdown {
            physical,
            ..DamageBreakdown::default()
        });
        push_scored_top_k(
            &mut candidates,
            candidate,
            &request,
            &game_data,
            &weapons,
            ResultGroupMode::Loadout,
            3,
        )
        .expect("candidate ranking succeeds");
    }

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].metric.ar.expect("AR missing").physical, 200.0);
}

#[test]
fn scored_top_k_completes_hidden_final_tie_breaks_before_truncation() {
    let game_data = load_data();
    let mut request = base_request();
    request.top_k = 1;
    let constraints = build_combat_constraints(&request).expect("constraints failed");
    let weapons = prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
    let all_candidates = (0_u8..=9)
        .map(|upgrade| scored_candidate(0, 0, upgrade, request.current_stats, 100.0))
        .collect::<Vec<_>>();
    let mut retained = Vec::new();
    for candidate in &all_candidates {
        push_scored_top_k(
            &mut retained,
            *candidate,
            &request,
            &game_data,
            &weapons,
            ResultGroupMode::Loadout,
            request.top_k,
        )
        .expect("candidate ranking succeeds");
    }
    assert_eq!(retained.len(), SCORED_TOP_K_LOADOUT_OVERSAMPLE);

    let bounded = materialize_scored_candidates(
        &request,
        &game_data,
        &weapons,
        retained,
        ResultGroupMode::Loadout,
    )
    .expect("bounded candidates materialize");
    let exhaustive = materialize_scored_candidates(
        &request,
        &game_data,
        &weapons,
        all_candidates,
        ResultGroupMode::Loadout,
    )
    .expect("exhaustive candidates materialize");
    assert_eq!(bounded[0].upgrade, exhaustive[0].upgrade);
    assert_eq!(bounded[0].stats, exhaustive[0].stats);
    assert!((bounded[0].ar.total() - exhaustive[0].ar.total()).abs() < 0.001);
}

#[test]
fn top_k_zero_short_circuits_without_progress_callbacks() {
    let game_data = load_data();
    let mut request = base_request();
    request.top_k = 0;
    let mut callback_count = 0;

    let results = optimize_with_progress(&request, &game_data, 1, |_snapshot| {
        callback_count += 1;
        true
    })
    .expect("optimizer failed");

    assert!(results.is_empty());
    assert_eq!(callback_count, 0);
}

#[test]
fn progress_emits_initial_and_final_snapshots() {
    let game_data = load_data();
    let request = base_request();
    let mut snapshots = Vec::new();

    optimize_with_progress(&request, &game_data, u64::MAX, |snapshot| {
        snapshots.push(snapshot);
        true
    })
    .expect("optimizer failed");

    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].checked, 0);
    assert_eq!(snapshots[1].checked, snapshots[1].total);
}

#[test]
fn progress_callback_can_cancel_search() {
    let game_data = load_data();
    let request = base_request();
    let mut callback_count = 0;

    let err = optimize_with_progress(&request, &game_data, 1, |_snapshot| {
        callback_count += 1;
        false
    })
    .expect_err("expected cancellation");

    assert_eq!(err, "cancelled");
    assert_eq!(callback_count, 1);
}

#[test]
fn preparation_callback_can_cancel_before_enumeration() {
    let game_data = load_data();
    let request = base_request();
    let mut callback_count = 0;

    let error = prepare_search_with_cancel(&request, &game_data, || {
        callback_count += 1;
        callback_count < 3
    })
    .expect_err("expected preparation cancellation");

    assert_eq!(error, "cancelled");
    assert_eq!(callback_count, 3);
}

#[test]
fn broad_enumeration_stops_within_cancellation_latency_target() {
    let game_data = load_data();
    let request = broad_request();
    let plan = prepare_search(&request, &game_data).expect("broad preparation succeeds");
    assert!(plan.estimate().combinations >= PARALLEL_SEARCH_MIN_COMBINATIONS);
    let cancelled = AtomicBool::new(false);
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);

    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let mut initial_sent = false;
            optimize_prepared_with_progress(&plan, 1_024, |_snapshot| {
                if !initial_sent {
                    initial_sent = true;
                    started_tx
                        .send(())
                        .expect("test receiver remains available");
                }
                !cancelled.load(Ordering::Acquire)
            })
        });
        started_rx.recv().expect("search emits initial progress");
        let cancellation_started = Instant::now();
        cancelled.store(true, Ordering::Release);
        let error = worker
            .join()
            .expect("search worker does not panic")
            .expect_err("broad search must cancel");
        let elapsed_ms = cancellation_started.elapsed().as_millis() as u64;
        assert_eq!(error, "cancelled");
        assert!(
            elapsed_ms <= CANCELLATION_LATENCY_TARGET_MS,
            "cancellation took {elapsed_ms} ms, target is {CANCELLATION_LATENCY_TARGET_MS} ms"
        );
    });
}

#[test]
fn optimize_respects_exact_stat_lock() {
    let game_data = load_data();
    let mut request = base_request();
    request.standard_max_upgrade = 0;
    request.somber_max_upgrade = 0;
    request.exact_upgrade = true;
    request.locked_combat_stats[STAT_ARC] = Some(8);
    request.locked_combat_stats[STAT_DEX] = Some(15);

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    for row in &results {
        assert_eq!(row.stats.dex, 15);
        assert_eq!(row.stats.arc, 8);
    }
}

#[test]
fn optimize_respects_all_exact_combat_stat_locks() {
    let game_data = load_data();
    let mut request = base_request();
    request.standard_max_upgrade = 0;
    request.somber_max_upgrade = 0;
    request.exact_upgrade = true;
    request.locked_combat_stats = [Some(12), Some(15), Some(9), Some(8), Some(8)];

    let constraints = build_combat_constraints(&request).expect("constraints failed");
    assert_eq!(count_stat_candidates(constraints), 1);

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    for row in &results {
        assert_eq!(row.stats.str, 12);
        assert_eq!(row.stats.dex, 15);
        assert_eq!(row.stats.int, 9);
        assert_eq!(row.stats.fai, 8);
        assert_eq!(row.stats.arc, 8);
    }
}

#[test]
fn relevant_stat_masks_track_scaling_sources() {
    let game_data = load_data();

    assert_eq!(
        active_mask_for(
            &game_data,
            "Giant-Crusher",
            "Heavy",
            OptimizeObjective::MaxAr,
            None
        ),
        [true, false, false, false, false]
    );
    assert_eq!(
        active_mask_for(
            &game_data,
            "Swift Spear",
            "Keen",
            OptimizeObjective::MaxAr,
            None
        ),
        [false, true, false, false, false]
    );
    assert_eq!(
        active_mask_for(
            &game_data,
            "Claymore",
            "Quality",
            OptimizeObjective::MaxAr,
            None
        ),
        [true, true, false, false, false]
    );
    assert_eq!(
        active_mask_for(
            &game_data,
            "Sword Lance",
            "Magic",
            OptimizeObjective::MaxAr,
            None
        ),
        [true, true, true, false, false]
    );
    assert!(
        active_mask_for(
            &game_data,
            "Uchigatana",
            "Blood",
            OptimizeObjective::MaxArPlusBleed,
            Some("Seppuku")
        )[STAT_ARC]
    );
}

#[test]
fn aow_override_rows_contribute_relevant_stats() {
    let game_data = load_data();
    let mask = active_mask_for(
        &game_data,
        "Giant-Crusher",
        "Heavy",
        OptimizeObjective::AowFirstHit,
        Some("Prelate's Charge"),
    );

    assert!(
        mask[STAT_FAI],
        "expected fire override attack rows to activate FAI scaling"
    );
}

#[test]
fn requirement_only_inactive_stats_are_preserved() {
    let game_data = load_data();
    let mut request = base_request();
    request.class_name = "Wretch".to_string();
    request.character_level = 20;
    request.current_stats = Stats {
        vig: 10,
        mnd: 10,
        end: 10,
        str: 10,
        dex: 10,
        int: 10,
        fai: 10,
        arc: 10,
    };
    request.weapon_name = Some("Uchigatana".to_string());
    request.affinity = Some("Heavy".to_string());
    request.aow_name = Some("Unsheathe".to_string());
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.top_k = 1;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert!(results[0].stats.dex >= 15);
}

#[test]
fn exact_locks_override_relevant_stat_pruning() {
    let game_data = load_data();
    let mut request = base_request();
    request.character_level = 31;
    request.weapon_name = Some("Uchigatana".to_string());
    request.affinity = Some("Heavy".to_string());
    request.aow_name = Some("Unsheathe".to_string());
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.locked_combat_stats[STAT_FAI] = Some(30);
    request.top_k = 1;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert_eq!(results[0].stats.fai, 30);
}

#[test]
fn estimate_search_space_uses_relevant_stat_counts() {
    let game_data = load_data();
    let mut request = broad_request();
    request.weapon_type_key = Some("Katana".to_string());
    request.top_k = 5;

    let constraints = build_combat_constraints(&request).expect("constraints failed");
    let broad_stat_count = count_stat_candidates(constraints);
    let prepared_weapons =
        prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
    let broad_slots: u64 = prepared_weapons
        .iter()
        .map(|prepared| (prepared.upgrades.len() * prepared.aow_choices.len()) as u64)
        .sum();
    let broad_combinations = broad_stat_count.saturating_mul(broad_slots);
    let estimate = estimate_search_space(&request, &game_data).expect("estimate failed");
    let prepared_estimate = prepare_search(&request, &game_data)
        .expect("search preparation failed")
        .estimate();

    assert_eq!(
        estimate.weapon_candidates,
        prepared_estimate.weapon_candidates
    );
    assert_eq!(estimate.stat_candidates, prepared_estimate.stat_candidates);
    assert_eq!(estimate.combinations, prepared_estimate.combinations);
    assert!(estimate.combinations < broad_combinations);
    assert!(estimate.stat_candidates < broad_stat_count.saturating_mul(broad_slots));
    assert!(
        !optimize(&request, &game_data)
            .expect("optimizer failed")
            .is_empty()
    );
}

#[test]
fn estimate_search_space_stops_before_preparation_when_cancelled() {
    let game_data = load_data();
    let error = estimate_search_space_with_cancel(&broad_request(), &game_data, || false)
        .expect_err("cancelled estimate must fail closed");
    assert_eq!(error, "cancelled");
}

#[test]
#[ignore = "release-mode estimate benchmark"]
fn benchmark_search_estimate_for_stat_entry_levels() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = None;
    request.affinity = None;
    request.exact_upgrade = false;
    for (label, level) in [
        ("base", 9),
        ("str-99", 96),
        ("str-dex-99", 180),
        ("all-99", 452),
    ] {
        request.character_level = level;
        let started = std::time::Instant::now();
        let estimate = estimate_search_space(&request, &game_data).expect("estimate failed");
        println!(
            "ESTIMATE_BENCH label={label} level={level} combinations={} elapsed_ms={:.3}",
            estimate.combinations,
            started.elapsed().as_secs_f64() * 1_000.0,
        );
    }
}

#[test]
fn optimize_keeps_one_result_per_weapon_setup() {
    let game_data = load_data();
    let mut request = base_request();
    request.character_level = 148;
    request.weapon_name = Some("Lizard Greatsword".to_string());
    request.affinity = Some("Keen".to_string());
    request.aow_name = Some("Seppuku".to_string());
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.two_handing = true;
    request.top_k = 50;

    let results = optimize(&request, &game_data).expect("optimizer failed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].weapon_name, "Lizard Greatsword");
    assert_eq!(results[0].affinity, "Keen");
    assert_eq!(results[0].aow_name.as_deref(), Some("Seppuku"));
    assert_eq!(results[0].upgrade, 25);
}

#[test]
fn exact_stat_locks_reject_unallocatable_remaining_points() {
    let game_data = load_data();
    let mut request = base_request();
    request.character_level = 10;
    request.locked_combat_stats = [Some(12), Some(15), Some(9), Some(8), Some(8)];

    let err = optimize(&request, &game_data).expect_err("expected exact-lock budget error");
    assert!(err.contains("locked combat stats cannot absorb remaining free points"));
}

#[test]
fn optimize_rejects_stat_caps_above_99() {
    let game_data = load_data();

    let mut current = base_request();
    current.current_stats.str = 100;
    let err = optimize(&current, &game_data).expect_err("expected current stat cap error");
    assert!(err.contains("str must be <= 99"));

    let mut minimum = base_request();
    minimum.min_combat_stats[STAT_STR] = 100;
    let err = optimize(&minimum, &game_data).expect_err("expected minimum stat cap error");
    assert!(err.contains("minimum combat stat 0 must be <= 99"));

    let mut locked = base_request();
    locked.locked_combat_stats[STAT_STR] = Some(100);
    let err = optimize(&locked, &game_data).expect_err("expected locked stat cap error");
    assert!(err.contains("locked combat stat 0 must be <= 99"));
}

#[test]
fn available_upgrades_skips_sparse_reinforce_levels() {
    let mut game_data = load_data();
    let weapon = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
        .expect("missing weapon")
        .clone();
    let levels = &mut game_data.reinforce[usize::from(weapon.reinforce_type)];
    assert!(levels[5].take().is_some());

    let mut request = base_request();
    request.exact_upgrade = false;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    let upgrades = available_upgrades(&weapon, &request, &game_data).expect("expected upgrades");

    assert!(upgrades.contains(&4));
    assert!(!upgrades.contains(&5));
    assert!(upgrades.contains(&6));
}

#[test]
fn fixed_upgrade_rejects_missing_sparse_reinforce_level() {
    let mut game_data = load_data();
    let weapon = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
        .expect("missing weapon")
        .clone();
    let levels = &mut game_data.reinforce[usize::from(weapon.reinforce_type)];
    assert!(levels[5].take().is_some());

    let mut request = base_request();
    request.standard_max_upgrade = 5;
    request.somber_max_upgrade = 5;
    request.exact_upgrade = true;

    assert!(available_upgrades(&weapon, &request, &game_data).is_none());
}

#[test]
fn exact_upgrade_uses_weapon_class_caps() {
    let game_data = load_data();
    let standard_weapon = game_data
        .weapons
        .iter()
        .find(|weapon| {
            !weapon.is_somber && weapon.name == "Uchigatana" && weapon.affinity == "Keen"
        })
        .expect("missing standard weapon");
    let somber_weapon = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.is_somber)
        .expect("missing somber weapon");
    let mut request = broad_request();
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;

    assert_eq!(
        available_upgrades(standard_weapon, &request, &game_data).expect("standard upgrades"),
        vec![25],
    );
    assert_eq!(
        available_upgrades(somber_weapon, &request, &game_data).expect("somber upgrades"),
        vec![10],
    );
}

#[test]
fn somber_only_exact_uses_somber_cap_not_standard_cap() {
    let game_data = load_data();
    let mut request = broad_request();
    request.somber_filter = SomberFilter::SomberOnly;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.top_k = 25;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert!(results.iter().all(|result| result.is_somber));
    assert!(results.iter().all(|result| result.upgrade == 10));
}

#[test]
fn optimize_rejects_seppuku_on_cold_affinity() {
    let game_data = load_data();
    let mut request = base_request();
    request.affinity = Some("Cold".to_string());
    request.aow_name = Some("Seppuku".to_string());
    request.objective = OptimizeObjective::MaxArPlusBleed;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(results.is_empty());
}

#[test]
fn paired_weapon_two_handing_does_not_inflate_ar() {
    let game_data = load_data();
    let mut one_hand = base_request();
    one_hand.class_name = "Wretch".to_string();
    one_hand.character_level = 64;
    one_hand.current_stats = Stats {
        vig: 10,
        mnd: 10,
        end: 10,
        str: 68,
        dex: 15,
        int: 10,
        fai: 10,
        arc: 10,
    };
    one_hand.weapon_name = Some("Iron Ball".to_string());
    one_hand.affinity = Some("Heavy".to_string());
    one_hand.aow_name = None;
    one_hand.standard_max_upgrade = 25;
    one_hand.somber_max_upgrade = 10;
    one_hand.exact_upgrade = true;
    one_hand.locked_combat_stats = [Some(68), Some(15), Some(10), Some(10), Some(10)];
    one_hand.two_handing = false;

    let mut two_hand = one_hand.clone();
    two_hand.two_handing = true;

    let one_hand_results = optimize(&one_hand, &game_data).expect("optimizer failed");
    let two_hand_results = optimize(&two_hand, &game_data).expect("optimizer failed");
    assert!(!one_hand_results.is_empty());
    assert!(!two_hand_results.is_empty());
    assert!((one_hand_results[0].ar.total() - two_hand_results[0].ar.total()).abs() < 0.001);
}

#[test]
fn paired_weapon_two_handing_does_not_reduce_requirements() {
    let game_data = load_data();
    let mut request = base_request();
    request.class_name = "Wretch".to_string();
    request.weapon_name = Some("Starscourge Greatsword".to_string());
    request.affinity = Some("Standard".to_string());
    request.aow_name = None;
    request.character_level = 24;
    request.current_stats = Stats {
        vig: 10,
        mnd: 10,
        end: 10,
        str: 26,
        dex: 12,
        int: 15,
        fai: 10,
        arc: 10,
    };
    request.standard_max_upgrade = 10;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.locked_combat_stats = [Some(26), Some(12), Some(15), Some(10), Some(10)];
    request.two_handing = true;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(results.is_empty());
}

#[test]
fn wasted_points_on_zero_scaling_stats_are_filtered() {
    let game_data = load_data();
    let weapon = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Sword Lance" && weapon.affinity == "Magic")
        .expect("missing weapon");
    let mut request = base_request();
    request.weapon_name = Some("Sword Lance".to_string());
    request.affinity = Some("Magic".to_string());
    request.aow_name = Some("Glintstone Pebble".to_string());
    request.current_stats = Stats {
        vig: 40,
        mnd: 11,
        end: 20,
        str: 21,
        dex: 15,
        int: 40,
        fai: 8,
        arc: 8,
    };
    request.character_level = 86;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.top_k = 10;
    let constraints = build_combat_constraints(&request).expect("constraints failed");
    let prepared_weapons =
        prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
    let prepared = prepared_weapons
        .iter()
        .find(|prepared| prepared.weapon.weapon_id == weapon.weapon_id)
        .expect("missing prepared weapon");
    let mut distribution_counts = HashMap::new();
    let search = relevant_stat_search(
        &request,
        &game_data,
        constraints,
        prepared,
        &prepared.aow_choices[0],
        &mut distribution_counts,
    )
    .expect("expected relevant stat search");
    assert!(!search.active[STAT_FAI]);
    assert!(!search.active[STAT_ARC]);
    assert!(search.candidate_count < count_stat_candidates(constraints));

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert!(
        results
            .iter()
            .all(|row| row.stats.fai == 8 && row.stats.arc == 8)
    );
}

#[test]
fn exact_aow_compatibility_is_loaded_from_csv() {
    let game_data = load_data();
    let cold_uchi = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Cold")
        .expect("missing cold uchigatana");
    let fire_uchi = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Fire")
        .expect("missing fire uchigatana");
    let blood_uchi = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Blood")
        .expect("missing blood uchigatana");
    let seppuku = game_data
        .aows
        .iter()
        .find(|aow| aow.name == "Seppuku")
        .expect("missing seppuku");

    assert!(!game_data.aow_compatible_with_weapon(seppuku, cold_uchi));
    assert!(!game_data.aow_compatible_with_weapon(seppuku, fire_uchi));
    assert!(game_data.aow_compatible_with_weapon(seppuku, blood_uchi));
}

#[test]
fn max_ar_plus_bleed_uses_innate_weapon_buildup() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Rivers of Blood".to_string());
    request.affinity = Some("Standard".to_string());
    request.aow_name = None;
    request.objective = OptimizeObjective::MaxArPlusBleed;
    request.standard_max_upgrade = 10;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.current_stats = Stats {
        vig: 40,
        mnd: 11,
        end: 20,
        str: 12,
        dex: 20,
        int: 9,
        fai: 8,
        arc: 20,
    };
    request.character_level = 61;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert!(results[0].bleed_buildup >= 50.0);
    assert_eq!(results[0].score, results[0].bleed_buildup);
}

#[test]
fn max_ar_plus_bleed_score_is_bleed_buildup() {
    let score = score_for(
        OptimizeObjective::MaxArPlusBleed,
        900.0,
        StatusBuildup {
            bleed: 78.0,
            frost: 0.0,
            poison: 0.0,
            scarlet_rot: 0.0,
            sleep: 0.0,
            madness: 0.0,
            death: 0.0,
        },
        0.0,
        0.0,
    );

    assert_eq!(score, 78.0);
}

#[test]
fn max_ar_plus_bleed_prefers_higher_bleed_over_higher_ar_plus_bleed() {
    let high_ar = test_result(1, 25, 900.0, 40.0);
    let high_bleed = test_result(2, 25, 500.0, 60.0);

    assert!(better_result(&high_bleed, &high_ar));
    assert!(!better_result(&high_ar, &high_bleed));
}

#[test]
fn max_ar_plus_bleed_equal_bleed_falls_through_to_higher_ar() {
    let low_ar = test_result(1, 25, 500.0, 60.0);
    let high_ar = test_result(2, 25, 900.0, 60.0);

    assert!(better_result(&high_ar, &low_ar));
    assert!(!better_result(&low_ar, &high_ar));
}

#[test]
fn aow_first_hit_damage_is_loaded_and_scored() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Sword Lance".to_string());
    request.affinity = Some("Magic".to_string());
    request.aow_name = Some("Glintstone Pebble".to_string());
    request.objective = OptimizeObjective::AowFirstHit;
    request.current_stats = Stats {
        vig: 40,
        mnd: 11,
        end: 20,
        str: 21,
        dex: 15,
        int: 40,
        fai: 8,
        arc: 8,
    };
    request.character_level = 84;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert!(results[0].aow_first_hit_damage > 0.0);
    assert!(results[0].aow_full_sequence_damage >= results[0].aow_first_hit_damage);
    assert_eq!(results[0].score, results[0].aow_first_hit_damage);
    let route = results[0].aow_route.as_ref().expect("selected AoW route");
    assert!(!route.route_label.is_empty());
    assert!(!route.actions.is_empty());
    assert!(route.actions.iter().any(|action| !action.hits.is_empty()));
    assert!(route.total_stamina_cost >= 0.0);
}

#[test]
fn seppuku_weapon_buff_affects_ar_and_bleed() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Uchigatana".to_string());
    request.affinity = Some("Blood".to_string());
    request.current_stats = Stats {
        vig: 12,
        mnd: 11,
        end: 13,
        str: 12,
        dex: 15,
        int: 9,
        fai: 8,
        arc: 45,
    };
    request.character_level = 46;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.locked_combat_stats = [Some(12), Some(15), Some(9), Some(8), Some(45)];
    request.objective = OptimizeObjective::MaxAr;
    request.aow_name = Some("Double Slash".to_string());

    let base_results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!base_results.is_empty());

    request.aow_name = Some("Seppuku".to_string());
    let seppuku_results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!seppuku_results.is_empty());

    let base = &base_results[0];
    let buffed = &seppuku_results[0];
    assert!(buffed.ar.total() >= base.ar.total() + 29.9);
    assert!(buffed.bleed_buildup > base.bleed_buildup + 30.0);
}

#[test]
fn open_max_ar_search_considers_compatible_buff_aows() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Uchigatana".to_string());
    request.affinity = Some("Blood".to_string());
    request.current_stats = Stats {
        vig: 12,
        mnd: 11,
        end: 13,
        str: 12,
        dex: 15,
        int: 9,
        fai: 8,
        arc: 45,
    };
    request.character_level = 46;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.locked_combat_stats = [Some(12), Some(15), Some(9), Some(8), Some(45)];
    request.objective = OptimizeObjective::MaxAr;

    let open_results = optimize(&request, &game_data).expect("open optimizer failed");
    assert!(!open_results.is_empty());
    assert_eq!(open_results[0].aow_name.as_deref(), Some("Seppuku"));

    request.aow_name = Some("Seppuku".to_string());
    let locked_results = optimize(&request, &game_data).expect("locked optimizer failed");
    assert!(!locked_results.is_empty());
    assert!(
        (open_results[0].score - locked_results[0].score).abs() < 0.001,
        "expected unlocked Max AR score {} to match Seppuku score {}",
        open_results[0].score,
        locked_results[0].score
    );
}

#[test]
fn open_max_ar_plus_bleed_matches_best_explicit_aow() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Uchigatana".to_string());
    request.affinity = Some("Keen".to_string());
    request.current_stats = Stats {
        vig: 40,
        mnd: 11,
        end: 20,
        str: 12,
        dex: 15,
        int: 9,
        fai: 8,
        arc: 8,
    };
    request.character_level = 112;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.locked_combat_stats = [Some(18), Some(40), Some(9), Some(8), Some(45)];
    request.objective = OptimizeObjective::MaxArPlusBleed;

    let open_results = optimize(&request, &game_data).expect("open optimizer failed");
    assert!(!open_results.is_empty());

    let weapon = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
        .expect("missing keen uchigatana");
    let mut expected_best: Option<OptimizeResult> = None;

    for aow in game_data
        .aows
        .iter()
        .filter(|aow| game_data.aow_compatible_with_weapon(aow, weapon))
    {
        request.aow_name = Some(aow.name.clone());
        let locked_results = optimize(&request, &game_data)
            .unwrap_or_else(|_| panic!("locked optimizer failed for {}", aow.name));
        if let Some(best_row) = locked_results.first()
            && expected_best
                .as_ref()
                .map(|expected| better_result(best_row, expected))
                .unwrap_or(true)
        {
            expected_best = Some(best_row.clone());
        }
    }

    let expected_best =
        expected_best.expect("expected at least one compatible AoW for Keen Uchigatana");
    assert!(
        (open_results[0].score - expected_best.score).abs() < 0.001,
        "expected unlocked Max AR + Bleed score {} to match best explicit score {}",
        open_results[0].score,
        expected_best.score
    );
    assert!(
        (open_results[0].ar.total() - expected_best.ar.total()).abs() < 0.001,
        "expected equal-bleed unlocked Max AR + Bleed AR {} to match best explicit AR {}",
        open_results[0].ar.total(),
        expected_best.ar.total()
    );
    for (label, actual, expected) in [
        (
            "bleed",
            open_results[0].bleed_buildup,
            expected_best.bleed_buildup,
        ),
        (
            "frost",
            open_results[0].frost_buildup,
            expected_best.frost_buildup,
        ),
        (
            "poison",
            open_results[0].poison_buildup,
            expected_best.poison_buildup,
        ),
        (
            "scarlet rot",
            open_results[0].scarlet_rot_buildup,
            expected_best.scarlet_rot_buildup,
        ),
        (
            "sleep",
            open_results[0].sleep_buildup,
            expected_best.sleep_buildup,
        ),
        (
            "madness",
            open_results[0].madness_buildup,
            expected_best.madness_buildup,
        ),
        (
            "death",
            open_results[0].death_buildup,
            expected_best.death_buildup,
        ),
    ] {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {label} unlocked value {actual} to match explicit value {expected}"
        );
    }
    assert!(open_results[0].aow_name.is_some());
}

#[test]
fn bleed_only_calculator_matches_full_status_for_open_choices() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Uchigatana".to_string());
    request.affinity = Some("Blood".to_string());
    request.objective = OptimizeObjective::MaxArPlusBleed;
    request.current_stats.arc = 45;
    request.character_level = 150;
    let constraints = build_combat_constraints(&request).expect("constraints failed");
    let prepared_weapons =
        prepare_weapons(&request, &game_data, constraints).expect("weapon preparation failed");
    let prepared = prepared_weapons.first().expect("missing prepared weapon");
    let stats = request.current_stats;

    for upgrade in [0, 10, 25] {
        if !prepared.upgrades.contains(&upgrade) {
            continue;
        }
        for aow_choice in &prepared.aow_choices {
            let full =
                calculate_status_with_buffs(prepared, aow_choice, upgrade, &stats, &game_data)
                    .expect("full status calculation failed");
            let bleed =
                calculate_bleed_with_buffs(prepared, aow_choice, upgrade, &stats, &game_data)
                    .expect("bleed status calculation failed");
            assert_eq!(bleed, full.bleed, "AoW {:?}", aow_choice.skill_name);
        }
    }
}

#[test]
fn aow_variant_rows_match_weapon_type() {
    let game_data = load_data();
    let weapon = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
        .expect("missing keen uchigatana");
    let sword_dance = game_data
        .aows
        .iter()
        .find(|aow| aow.name == "Sword Dance")
        .expect("missing sword dance");
    let rows = select_aow_attack_rows(sword_dance.aow_id, weapon, &game_data);
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .all(|row| row.variant_weapon_type.is_empty() || row.variant_weapon_type == "Katana")
    );
    assert!(
        rows.iter()
            .any(|row| row.raw_name.starts_with("[Katana] Sword Dance"))
    );
}

#[test]
fn lion_claw_resolves_aow_choice_for_claymore() {
    let game_data = load_data();
    let weapon = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Claymore" && weapon.affinity == "Standard")
        .expect("missing claymore");
    let mut request = base_request();
    request.weapon_name = Some("Claymore".to_string());
    request.affinity = Some("Standard".to_string());
    request.aow_name = Some("Lion's Claw".to_string());
    request.objective = OptimizeObjective::AowFirstHit;
    request.current_stats = Stats {
        vig: 20,
        mnd: 15,
        end: 20,
        str: 40,
        dex: 30,
        int: 10,
        fai: 10,
        arc: 10,
    };
    request.character_level = 76;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.locked_combat_stats = [Some(40), Some(30), Some(10), Some(10), Some(10)];

    let choices = resolve_aow_choices(weapon, &request, &game_data).expect("resolve failed");
    let choices = choices.expect("expected choices");
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].skill_name, Some("Lion's Claw"));
    assert!(
        !choices[0].attack_rows.is_empty(),
        "expected Lion's Claw attack rows for Claymore"
    );
}

#[test]
fn beasts_roar_first_hit_uses_first_positive_damage_row() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Antspur Rapier".to_string());
    request.affinity = Some("Blood".to_string());
    request.aow_name = Some("Beast's Roar".to_string());
    request.objective = OptimizeObjective::AowFirstHit;
    request.current_stats = Stats {
        vig: 20,
        mnd: 20,
        end: 20,
        str: 60,
        dex: 60,
        int: 60,
        fai: 60,
        arc: 60,
    };
    request.character_level = 331;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert!(results[0].aow_first_hit_damage > 0.0);
    assert!(results[0].aow_full_sequence_damage >= results[0].aow_first_hit_damage);
}

#[test]
fn zero_damage_roar_has_no_results_for_damage_objective() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Bandit's Curved Sword".to_string());
    request.affinity = Some("Blood".to_string());
    request.aow_name = Some("Braggart's Roar".to_string());
    request.objective = OptimizeObjective::AowFirstHit;
    request.current_stats = Stats {
        vig: 20,
        mnd: 20,
        end: 20,
        str: 60,
        dex: 60,
        int: 60,
        fai: 60,
        arc: 60,
    };
    request.character_level = 331;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(results.is_empty());
}

#[test]
fn spinning_slash_placeholder_variants_match_greatsword() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Bastard Sword".to_string());
    request.affinity = Some("Standard".to_string());
    request.aow_name = Some("Spinning Slash".to_string());
    request.objective = OptimizeObjective::AowFirstHit;
    request.current_stats = Stats {
        vig: 20,
        mnd: 20,
        end: 20,
        str: 60,
        dex: 60,
        int: 60,
        fai: 60,
        arc: 60,
    };
    request.character_level = 331;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.standard_max_upgrade = 25;
    request.somber_max_upgrade = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert!(results[0].aow_first_hit_damage > 0.0);
}

#[test]
fn somber_weapons_do_not_accept_generic_ashes_of_war() {
    let game_data = load_data();
    let halo_scythe = game_data
        .weapons
        .iter()
        .find(|weapon| weapon.name == "Halo Scythe" && weapon.affinity == "Standard")
        .expect("missing halo scythe");
    let sword_dance = game_data
        .aows
        .iter()
        .find(|aow| aow.name == "Sword Dance")
        .expect("missing sword dance");

    assert!(halo_scythe.disable_gem_attr);
    assert!(!game_data.aow_compatible_with_weapon(sword_dance, halo_scythe));
}

#[test]
fn somber_weapon_native_skill_damage_is_loaded_and_scored() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Halo Scythe".to_string());
    request.affinity = Some("Standard".to_string());
    request.aow_name = None;
    request.objective = OptimizeObjective::AowFirstHit;
    request.current_stats = Stats {
        vig: 40,
        mnd: 11,
        end: 20,
        str: 16,
        dex: 16,
        int: 9,
        fai: 45,
        arc: 8,
    };
    request.character_level = 88;
    request.standard_max_upgrade = 10;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.standard_max_upgrade = 10;
    request.somber_max_upgrade = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert_eq!(
        results[0].aow_name.as_deref(),
        Some("Miquella's Ring of Light")
    );
    assert!(results[0].aow_first_hit_damage > 0.0);
    assert!(results[0].aow_full_sequence_damage >= results[0].aow_first_hit_damage);
    assert_eq!(results[0].score, results[0].aow_first_hit_damage);
}

#[test]
fn somber_weapon_max_ar_keeps_native_skill_metrics() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Halo Scythe".to_string());
    request.affinity = Some("Standard".to_string());
    request.aow_name = None;
    request.objective = OptimizeObjective::MaxAr;
    request.current_stats = Stats {
        vig: 40,
        mnd: 11,
        end: 20,
        str: 18,
        dex: 40,
        int: 9,
        fai: 26,
        arc: 45,
    };
    request.character_level = 150;
    request.standard_max_upgrade = 10;
    request.somber_max_upgrade = 10;
    request.exact_upgrade = true;
    request.standard_max_upgrade = 10;
    request.somber_max_upgrade = 10;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(!results.is_empty());
    assert_eq!(
        results[0].aow_name.as_deref(),
        Some("Miquella's Ring of Light")
    );
    assert!(results[0].aow_first_hit_damage > 0.0);
    assert!(results[0].aow_full_sequence_damage > 0.0);
}

#[test]
fn utility_aow_has_no_results_for_aow_damage_objective() {
    let game_data = load_data();
    let mut request = base_request();
    request.weapon_name = Some("Buckler".to_string());
    request.affinity = Some("Standard".to_string());
    request.aow_name = Some("Parry".to_string());
    request.objective = OptimizeObjective::AowFirstHit;
    request.standard_max_upgrade = 0;
    request.somber_max_upgrade = 0;
    request.exact_upgrade = true;

    let results = optimize(&request, &game_data).expect("optimizer failed");
    assert!(results.is_empty());
}
