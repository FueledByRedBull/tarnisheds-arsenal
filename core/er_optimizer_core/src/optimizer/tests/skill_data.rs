use super::*;

fn fixed_skill_request(weapon: &str, skill: &str) -> OptimizeRequest {
    let stats = Stats {
        vig: 10,
        mnd: 10,
        end: 10,
        str: 20,
        dex: 30,
        int: 30,
        fai: 20,
        arc: 20,
    };
    OptimizeRequest {
        class_name: "Wretch".into(),
        character_level: 71,
        current_stats: stats,
        min_combat_stats: [0; 5],
        locked_combat_stats: stats.combat_array().map(Some),
        standard_max_upgrade: 0,
        somber_max_upgrade: 0,
        exact_upgrade: true,
        two_handing: false,
        dlc_scaling: false,
        scadutree_level: 0,
        weapon_name: Some(weapon.into()),
        affinity: Some("Standard".into()),
        aow_name: Some(skill.into()),
        weapon_type_key: None,
        somber_filter: SomberFilter::All,
        filters: vec![],
        result_grouping: ResultGrouping::Loadout,
        objective: OptimizeObjective::AowFullSequence,
        top_k: 1,
    }
}

#[test]
fn default_skills_do_not_use_other_ashes_attacks() {
    let data = load_data();
    // Hand-checked workbook attacks under the fixed stats above; each expected
    // sequence includes only attacks belonging to the requested skill.
    for (weapon, skill, first, full) in [
        ("Milady", "Impaling Thrust", 284.6826, 284.6826),
        ("Smithscript Dagger", "Piercing Throw", 151.42316, 151.42316),
        ("Beast Claw", "Savage Claws", 100.19273, 607.41846),
    ] {
        let mut request = fixed_skill_request(weapon, skill);
        for objective in [
            OptimizeObjective::MaxAr,
            OptimizeObjective::AowFirstHit,
            OptimizeObjective::AowFullSequence,
        ] {
            request.objective = objective;
            let rows = optimize(&request, &data).unwrap();
            assert_eq!(rows.len(), 1, "{weapon}/{skill}/{objective:?}");
            let row = &rows[0];
            let route = row.aow_route.as_ref().expect("modeled skill route");
            for hit in route.actions.iter().flat_map(|action| &action.hits) {
                assert!(
                    hit.raw_name.starts_with(skill),
                    "{weapon}: {}",
                    hit.raw_name
                );
            }
            assert!((row.aow_first_hit_damage - first).abs() < 0.001);
            assert!((row.aow_full_sequence_damage - full).abs() < 0.001);
        }
    }
}

#[test]
fn weapon_restricted_transferable_skills_have_their_own_damage_routes() {
    let data = load_data();
    for (weapon, skill, skill_id) in [
        ("Milady", "Wing Stance", 4120),
        ("Smithscript Dagger", "Scattershot Throw", 4030),
        ("Beast Claw", "Raging Beast", 4060),
    ] {
        let mut request = fixed_skill_request(weapon, skill);
        for objective in [
            OptimizeObjective::MaxAr,
            OptimizeObjective::AowFirstHit,
            OptimizeObjective::AowFullSequence,
        ] {
            request.objective = objective;
            let rows = optimize(&request, &data).unwrap();
            assert_eq!(rows.len(), 1, "{weapon}/{skill}/{objective:?}");
            let row = &rows[0];
            assert_eq!(row.aow_id, Some(skill_id));
            let route = row.aow_route.as_ref().expect("modeled transferable skill");
            assert!(row.aow_first_hit_damage > 0.0);
            for hit in route.actions.iter().flat_map(|action| &action.hits) {
                assert!(
                    hit.raw_name.starts_with(skill),
                    "{weapon}: {}",
                    hit.raw_name
                );
            }
        }
    }
}

#[test]
fn carian_grandeur_chooses_one_charge_level_for_one_strike() {
    let data = load_data();
    let mut request = fixed_skill_request("Longsword", "Carian Grandeur");
    for objective in [
        OptimizeObjective::AowFirstHit,
        OptimizeObjective::AowFullSequence,
    ] {
        request.objective = objective;
        let rows = optimize(&request, &data).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        let route = row.aow_route.as_ref().unwrap();
        let hits: Vec<_> = route
            .actions
            .iter()
            .flat_map(|action| &action.hits)
            .collect();
        assert_eq!(hits.len(), 1, "charge alternatives must not be summed");
        assert_eq!(hits[0].raw_name, "Carian Grandeur Charged 2");
        assert!((row.aow_first_hit_damage - 441.61002).abs() < 0.001);
        assert!((row.aow_full_sequence_damage - 441.61002).abs() < 0.001);
        assert_eq!(route.total_poise_damage, 45.0);
    }
}

#[test]
fn charged_glintstone_dart_keeps_its_followup_thrust() {
    let data = load_data();
    let mut request = fixed_skill_request("Glintstone Kris", "Glintstone Dart");
    for objective in [
        OptimizeObjective::AowFirstHit,
        OptimizeObjective::AowFullSequence,
    ] {
        request.objective = objective;
        let rows = optimize(&request, &data).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        let route = row.aow_route.as_ref().unwrap();
        let names: Vec<_> = route
            .actions
            .iter()
            .flat_map(|action| &action.hits)
            .map(|hit| hit.raw_name.as_str())
            .collect();
        assert_eq!(
            names,
            ["Glintstone Dart Charged - Bullet", "Glintstone Dart R2"]
        );
        assert!((row.aow_first_hit_damage - 230.53333).abs() < 0.001);
        assert!((row.aow_full_sequence_damage - 488.53675).abs() < 0.001);
        assert_eq!(route.total_poise_damage, 35.5);
    }
}
