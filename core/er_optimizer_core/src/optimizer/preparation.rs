use super::*;

pub(super) fn build_combat_constraints(
    request: &OptimizeRequest,
    data: &GameData,
) -> Result<CombatConstraints, String> {
    validate_stat_caps(request)?;
    let class_info = data
        .class_by_name(&request.class_name)
        .ok_or_else(|| format!("unknown starting class: {}", request.class_name))?;
    let free_points =
        compute_free_points(&class_info, request.character_level, &request.current_stats)?;
    let current = request.current_stats.combat_array();

    let mut mins = [0_u8; COMBAT_STAT_COUNT];
    let mut maxs = [99_u8; COMBAT_STAT_COUNT];
    let mut mandatory_raise: u16 = 0;
    for idx in 0..COMBAT_STAT_COUNT {
        mins[idx] = current[idx].max(request.min_combat_stats[idx]);
        if let Some(locked) = request.locked_combat_stats[idx] {
            if locked < mins[idx] {
                return Err(format!(
                    "locked combat stat {} is below minimum floor {}",
                    idx, mins[idx]
                ));
            }
            mins[idx] = locked;
            maxs[idx] = locked;
        }
        mandatory_raise += u16::from(mins[idx].saturating_sub(current[idx]));
    }
    if mandatory_raise > free_points {
        return Err(format!(
            "combat stat floors require {mandatory_raise} points, but the level budget has {free_points}"
        ));
    }

    let remaining_free = free_points - mandatory_raise;
    let capacity: u16 = maxs
        .iter()
        .zip(mins.iter())
        .map(|(max_v, min_v)| u16::from(*max_v - *min_v))
        .sum();
    if remaining_free > capacity {
        return Err("locked combat stats cannot absorb remaining free points".to_string());
    }

    Ok(CombatConstraints {
        mins,
        maxs,
        remaining_free,
    })
}

pub(super) fn validate_stat_caps(request: &OptimizeRequest) -> Result<(), String> {
    let stats = [
        ("vig", request.current_stats.vig),
        ("mnd", request.current_stats.mnd),
        ("end", request.current_stats.end),
        ("str", request.current_stats.str),
        ("dex", request.current_stats.dex),
        ("int", request.current_stats.int),
        ("fai", request.current_stats.fai),
        ("arc", request.current_stats.arc),
    ];
    for (name, value) in stats {
        if value > 99 {
            return Err(format!("{name} must be <= 99"));
        }
    }
    for idx in 0..COMBAT_STAT_COUNT {
        if request.min_combat_stats[idx] > 99 {
            return Err(format!("minimum combat stat {idx} must be <= 99"));
        }
        if request.locked_combat_stats[idx].is_some_and(|value| value > 99) {
            return Err(format!("locked combat stat {idx} must be <= 99"));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn prepare_weapons<'a>(
    request: &OptimizeRequest,
    data: &'a GameData,
    constraints: CombatConstraints,
) -> Result<Vec<PreparedWeapon<'a>>, String> {
    prepare_weapons_with_cancel(request, data, Some(constraints), &mut || true)
}

// Weapons each Rayon thread prepares between cancellation checks. Scaling the chunk with the
// pool keeps one chunk to a few milliseconds of wall time on any thread count, far inside the
// 250 ms cancellation target, while large pools still get enough work per barrier.
pub(super) const PREPARE_WEAPONS_PER_THREAD: usize = 64;

pub(super) fn prepare_weapon_chunk() -> usize {
    PREPARE_WEAPONS_PER_THREAD * rayon::current_num_threads().max(1)
}

pub(super) fn prepare_weapons_with_cancel<'a>(
    request: &OptimizeRequest,
    data: &'a GameData,
    constraints: Option<CombatConstraints>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Vec<PreparedWeapon<'a>>, String> {
    // Each weapon prepares independently, so chunks run in parallel. Results keep weapon
    // order and the first error by weapon order wins, exactly as a serial pass would.
    let templates = ash_templates(
        data.weapons
            .iter()
            .filter(|weapon| weapon_matches_request(weapon, request, data)),
        request,
        data,
    );
    let mut out = Vec::new();
    for chunk in data.weapons.chunks(prepare_weapon_chunk()) {
        if !should_continue() {
            return Err("cancelled".to_string());
        }
        let prepared: Vec<Result<Option<PreparedWeapon<'a>>, String>> = chunk
            .par_iter()
            .map(|weapon| prepare_weapon(weapon, request, data, constraints, &templates))
            .collect();
        for weapon in prepared {
            out.extend(weapon?);
        }
    }
    Ok(out)
}

pub(super) fn prepare_weapon<'a>(
    weapon: &'a Weapon,
    request: &OptimizeRequest,
    data: &'a GameData,
    constraints: Option<CombatConstraints>,
    templates: &AshTemplates<'a>,
) -> Result<Option<PreparedWeapon<'a>>, String> {
    if !weapon_matches_request(weapon, request, data) || !data.weapon_ar_supported(weapon) {
        return Ok(None);
    }
    // Reusable evaluators can change stat budgets after preparation.
    if constraints
        .is_some_and(|constraints| !weapon_requirements_can_fit(request, constraints, weapon))
    {
        return Ok(None);
    }
    let Some(upgrades) = available_upgrades(weapon, request, data) else {
        return Ok(None);
    };
    let Some(aow_choices) = resolve_aow_choices_cached(weapon, request, data, templates)? else {
        return Ok(None);
    };
    let inputs_dominated_by_next = upgrades
        .windows(2)
        .map(|pair| crate::math::exact::upgrade_inputs_dominated(weapon, pair[0], pair[1], data))
        .collect();
    Ok(Some(PreparedWeapon {
        weapon,
        aow_choices,
        upgrades,
        inputs_dominated_by_next,
    }))
}

pub(super) fn weapon_matches_request(
    weapon: &Weapon,
    request: &OptimizeRequest,
    data: &GameData,
) -> bool {
    if let Some(lock_weapon) = request.weapon_name.as_deref()
        && !weapon.name.eq_ignore_ascii_case(lock_weapon)
    {
        return false;
    }
    if let Some(lock_affinity) = request.affinity.as_deref()
        && !weapon.affinity.eq_ignore_ascii_case(lock_affinity)
    {
        return false;
    }
    if let Some(type_key) = request.weapon_type_key.as_deref()
        && !weapon_type_matches(weapon, type_key)
    {
        return false;
    }
    let reinforcement_matches = match request.somber_filter {
        SomberFilter::All => true,
        SomberFilter::StandardOnly => !weapon.is_somber,
        SomberFilter::SomberOnly => weapon.is_somber,
    };
    if !reinforcement_matches {
        return false;
    }
    if request.filters.is_empty() {
        return true;
    }
    let family_id = weapon.family_filter_id();
    let type_id = weapon.type_filter_id();
    let affinity_id = weapon.affinity_filter_id();
    let reinforcement_id = if weapon.is_somber {
        "reinforcement:somber"
    } else {
        "reinforcement:standard"
    };
    let mut coverage_ids = vec!["coverage:weapon-ar"];
    if data.capabilities.status_buildup {
        coverage_ids.push("coverage:status");
    }
    if data.capabilities.aow_compatibility {
        coverage_ids.push("coverage:aow-compatibility");
    }
    if data.capabilities.aow_damage {
        coverage_ids.push("coverage:aow-damage");
    }
    if data.capabilities.aow_routes {
        coverage_ids.push("coverage:aow-routes");
    }
    filter_dimension_matches(request, FilterDimension::WeaponFamily, |id| {
        id.eq_ignore_ascii_case(&family_id)
    }) && filter_dimension_matches(request, FilterDimension::WeaponType, |id| {
        id.eq_ignore_ascii_case(&type_id)
    }) && filter_dimension_matches(request, FilterDimension::Affinity, |id| {
        id.eq_ignore_ascii_case(&affinity_id)
    }) && filter_dimension_matches(request, FilterDimension::Reinforcement, |id| {
        id.eq_ignore_ascii_case(reinforcement_id)
    }) && filter_dimension_matches(request, FilterDimension::Coverage, |id| {
        coverage_ids
            .iter()
            .any(|value| id.eq_ignore_ascii_case(value))
    })
}

pub(super) fn filter_dimension_matches(
    request: &OptimizeRequest,
    dimension: FilterDimension,
    matches_id: impl Fn(&str) -> bool,
) -> bool {
    let mut has_include = false;
    let mut include_matches = false;
    for filter in request
        .filters
        .iter()
        .filter(|filter| filter.dimension == dimension)
    {
        match filter.mode {
            FilterMode::Exclude if matches_id(&filter.id) => return false,
            FilterMode::Include => {
                has_include = true;
                include_matches |= matches_id(&filter.id);
            }
            FilterMode::Exclude => {}
        }
    }
    !has_include || include_matches
}

pub(super) fn weapon_type_matches(weapon: &Weapon, type_key: &str) -> bool {
    normalize_weapon_type_display(&weapon.weapon_type_name).eq_ignore_ascii_case(type_key)
        || weapon.weapon_type_name.eq_ignore_ascii_case(type_key)
        || weapon
            .weapon_type_keys
            .split('|')
            .any(|key| key.eq_ignore_ascii_case(type_key))
}

pub(super) fn available_upgrades(
    weapon: &Weapon,
    request: &OptimizeRequest,
    data: &GameData,
) -> Option<Vec<u8>> {
    let levels = data.reinforce.get(usize::from(weapon.reinforce_type))?;
    if levels.is_empty() {
        return None;
    }

    let cap = upgrade_cap_for_weapon(weapon, request);

    if request.exact_upgrade {
        let fixed = cap;
        return data
            .reinforce_level(weapon.reinforce_type, fixed)
            .is_some()
            .then(|| vec![fixed]);
    }

    let out: Vec<u8> = levels
        .iter()
        .enumerate()
        .filter_map(|(level, value)| {
            (value.is_some() && level <= usize::from(cap)).then_some(level as u8)
        })
        .collect();
    (!out.is_empty()).then_some(out)
}

pub(super) fn upgrade_cap_for_weapon(weapon: &Weapon, request: &OptimizeRequest) -> u8 {
    if weapon.is_somber {
        request.somber_max_upgrade
    } else {
        request.standard_max_upgrade
    }
}

/// Applicable Ashes, in table order, with the attack rows and handled routes each uses.
/// Compatibility reads only whether the weapon can change Ashes, its affinity and its type
/// keys; rows and routes read its type name (which also fixes forced two-handing) and the
/// request's handling. One list per distinct key serves every matching weapon and thread.
/// Routes are compiled only for Ashes the request can select by name.
pub(super) type AshTemplates<'a> = HashMap<
    (bool, &'a str, &'a str, &'a str),
    Vec<(
        &'a Aow,
        Vec<&'a AowAttackRow>,
        Option<SharedScalarRoutes<'a>>,
    )>,
>;

fn ash_template_key(weapon: &Weapon) -> (bool, &str, &str, &str) {
    (
        weapon.can_change_aow,
        weapon.affinity.as_str(),
        weapon.weapon_type_keys.as_str(),
        weapon.weapon_type_name.as_str(),
    )
}

pub(super) fn ash_templates<'a>(
    weapons: impl Iterator<Item = &'a Weapon>,
    request: &OptimizeRequest,
    data: &'a GameData,
) -> AshTemplates<'a> {
    let mut representatives = HashMap::new();
    for weapon in weapons {
        representatives
            .entry(ash_template_key(weapon))
            .or_insert(weapon);
    }
    // Compatibility per key, then rows and routes per distinct Ash and weapon type name.
    let compatible: Vec<(_, &Weapon, Vec<&Aow>)> = representatives
        .into_par_iter()
        .map(|(key, weapon)| {
            let aows = data
                .aows
                .iter()
                .filter(|aow| data.aow_compatible_with_weapon(aow, weapon))
                .collect();
            (key, weapon, aows)
        })
        .collect();
    let mut needed = HashMap::new();
    for (_, weapon, aows) in &compatible {
        for aow in aows {
            needed
                .entry((aow.aow_id, weapon.weapon_type_name.as_str()))
                .or_insert((*aow, *weapon));
        }
    }
    let selected: HashMap<_, _> = needed
        .into_par_iter()
        .map(|((aow_id, type_name), (aow, weapon))| {
            let rows = select_aow_attack_rows(aow_id, weapon, data);
            let selectable = request
                .aow_name
                .as_deref()
                .is_none_or(|name| aow.name.eq_ignore_ascii_case(name));
            let routes = selectable.then(|| {
                let two_handing = weapon_uses_two_handing(request, weapon);
                Arc::new(handled_scalar_routes(&rows, data, two_handing))
            });
            ((aow_id, type_name), (rows, routes))
        })
        .collect();
    compatible
        .into_iter()
        .map(|(key, weapon, aows)| {
            let template = aows
                .into_iter()
                .map(|aow| {
                    let (rows, routes) = &selected[&(aow.aow_id, weapon.weapon_type_name.as_str())];
                    (aow, rows.clone(), routes.clone())
                })
                .collect();
            (key, template)
        })
        .collect()
}

#[cfg(test)]
pub(super) fn resolve_aow_choices<'a>(
    weapon: &'a Weapon,
    request: &OptimizeRequest,
    data: &'a GameData,
) -> Result<Option<Vec<AowChoice<'a>>>, String> {
    let templates = ash_templates(std::iter::once(weapon), request, data);
    resolve_aow_choices_cached(weapon, request, data, &templates)
}

pub(super) fn resolve_aow_choices_cached<'a>(
    weapon: &'a Weapon,
    request: &OptimizeRequest,
    data: &'a GameData,
    templates: &AshTemplates<'a>,
) -> Result<Option<Vec<AowChoice<'a>>>, String> {
    let native = native_skill_choice_for_weapon(weapon, data);
    let mut choices: Vec<_> = native.clone().into_iter().collect();
    if native.is_none() && weapon.affinity.eq_ignore_ascii_case("Standard") {
        choices.push(AowChoice {
            no_applied_ash: true,
            aow: None,
            skill_id: None,
            skill_name: None,
            attack_rows: Vec::new(),
            scalar_routes: None,
        });
    }
    let template = templates
        .get(&ash_template_key(weapon))
        .ok_or("weapon preparation has no Ash template")?;
    for (aow, rows, routes) in template {
        if let Some(choice) = native
            .as_ref()
            .filter(|choice| choice.skill_id == Some(aow.aow_id))
        {
            if choice.no_applied_ash {
                let mut applied = choice.clone();
                applied.no_applied_ash = false;
                choices.push(applied);
            }
        } else {
            choices.push(AowChoice {
                no_applied_ash: false,
                aow: Some(aow),
                skill_id: Some(aow.aow_id),
                skill_name: Some(aow.name.as_str()),
                attack_rows: rows.clone(),
                scalar_routes: routes.clone(),
            });
        }
    }
    if let Some(name) = request.aow_name.as_deref() {
        choices.retain(|choice| {
            choice
                .skill_name
                .is_some_and(|skill| skill.eq_ignore_ascii_case(name))
        });
        if choices.is_empty() {
            let known = data
                .aows
                .iter()
                .any(|aow| aow.name.eq_ignore_ascii_case(name))
                || data.weapons.iter().any(|candidate| {
                    candidate
                        .native_skill_name
                        .as_deref()
                        .is_some_and(|skill| skill.eq_ignore_ascii_case(name))
                        && data.native_skill_compatible_with_weapon(candidate)
                });
            if !known {
                return Err(format!("unknown AoW: {name}"));
            }
        }
    }
    let mut seen_skills = HashSet::new();
    choices.retain(|choice| {
        if matches!(
            request.objective,
            OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
        ) && choice.attack_rows.is_empty()
        {
            return false;
        }
        if matches!(
            request.objective,
            OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
        ) && choice.attack_rows.iter().any(|row| {
            !row.is_lacking_fp
                && data
                    .aow_effects(row.aow_id, row.sheet_row)
                    .iter()
                    .chain(data.aow_effects(row.aow_id, 0))
                    .any(|effect| !effect.is_supported)
        }) {
            return false;
        }
        // No objective can score routes with per-hit attack power yet; skip the Ash
        // instead of failing the whole search.
        if choice
            .attack_rows
            .iter()
            .any(|row| !row.is_lacking_fp && has_unmodeled_attack_power(row, data))
        {
            return false;
        }
        filter_dimension_matches(request, FilterDimension::Aow, |id| {
            if id.eq_ignore_ascii_case("aow:none") {
                choice.no_applied_ash
            } else {
                choice
                    .skill_id
                    .is_some_and(|skill| id.eq_ignore_ascii_case(&format!("aow:{skill}")))
            }
        }) && seen_skills.insert(choice.skill_id)
    });
    Ok((!choices.is_empty()).then_some(choices))
}

pub(super) fn native_skill_choice_for_weapon<'a>(
    weapon: &'a Weapon,
    data: &'a GameData,
) -> Option<AowChoice<'a>> {
    if !data.native_skill_compatible_with_weapon(weapon) {
        return None;
    }
    let native_skill_id = weapon.native_skill_id?;
    let exact_rows = data.native_skill_attack_rows(weapon.weapon_id);
    let source_rows = if exact_rows.is_empty() {
        data.aow_attack_rows(native_skill_id)
    } else {
        exact_rows
    };
    let attack_rows = select_attack_rows(source_rows, weapon);
    let aow = data.aows.iter().find(|aow| aow.aow_id == native_skill_id);
    let skill_name = weapon
        .native_skill_name
        .as_deref()
        .or_else(|| aow.map(|aow| aow.name.as_str()))
        .or_else(|| source_rows.first().map(|row| row.aow_name.as_str()));
    Some(AowChoice {
        no_applied_ash: weapon.affinity.eq_ignore_ascii_case("Standard"),
        aow,
        skill_id: Some(native_skill_id),
        skill_name,
        attack_rows,
        scalar_routes: None,
    })
}

pub(super) fn select_aow_attack_rows<'a>(
    aow_id: u16,
    weapon: &Weapon,
    data: &'a GameData,
) -> Vec<&'a AowAttackRow> {
    select_attack_rows(data.aow_attack_rows(aow_id), weapon)
}

pub(super) fn select_attack_rows<'a>(
    rows: &'a [AowAttackRow],
    weapon: &Weapon,
) -> Vec<&'a AowAttackRow> {
    if rows.is_empty() {
        return Vec::new();
    }

    let matched_rows: Vec<&AowAttackRow> = rows
        .iter()
        .filter(|row| {
            !row.variant_weapon_type.is_empty()
                && variant_weapon_type_matches(&row.variant_weapon_type, &weapon.weapon_type_name)
        })
        .collect();
    if !matched_rows.is_empty() {
        return matched_rows;
    }

    let generic_rows: Vec<&AowAttackRow> = rows
        .iter()
        .filter(|row| row.variant_weapon_type.is_empty())
        .collect();
    if !generic_rows.is_empty() {
        return generic_rows;
    }

    let placeholder_rows: Vec<&AowAttackRow> = rows
        .iter()
        .filter(|row| is_placeholder_variant(&row.variant_weapon_type))
        .collect();
    if placeholder_rows.is_empty() {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for row in placeholder_rows {
        let key = raw_name_without_variant_prefix(&row.raw_name).to_ascii_lowercase();
        if seen.insert(key) {
            deduped.push(row);
        }
    }
    deduped
}

pub(super) fn variant_weapon_type_matches(variant: &str, weapon_type_name: &str) -> bool {
    if variant.is_empty() {
        return false;
    }
    normalize_type_token(variant) == normalize_type_token(weapon_type_name)
}

pub(super) fn normalize_type_token(value: &str) -> String {
    let mut normalized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    normalized = match normalized.as_str() {
        "backhandblade" => "reversehandblade".to_string(),
        "greatspear" => "heavyspear".to_string(),
        "reaper" => "scythe".to_string(),
        _ => normalized,
    };
    normalized
}

pub(super) fn is_placeholder_variant(variant: &str) -> bool {
    let normalized = normalize_type_token(variant);
    normalized.starts_with("var")
        && normalized.get(3..).is_some_and(|suffix| {
            !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
        })
}

pub(super) fn raw_name_without_variant_prefix(raw_name: &str) -> &str {
    if let Some(remainder) = raw_name
        .strip_prefix('[')
        .and_then(|tail| tail.split_once(']').map(|(_, remainder)| remainder.trim()))
    {
        return remainder;
    }
    raw_name
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct RelevantStatSearch {
    pub(super) mins: [u8; COMBAT_STAT_COUNT],
    pub(super) maxs: [u8; COMBAT_STAT_COUNT],
    pub(super) active: [bool; COMBAT_STAT_COUNT],
    pub(super) remaining_free: u16,
    pub(super) candidate_count: u64,
}

pub(super) fn relevant_stat_search(
    request: &OptimizeRequest,
    data: &GameData,
    constraints: CombatConstraints,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    distribution_counts: &mut HashMap<DistributionCountKey, u64>,
) -> Option<RelevantStatSearch> {
    let active = active_stats_for_choice(prepared, aow_choice, data);
    RelevantStatSearch::new(
        request,
        constraints,
        prepared.weapon,
        active,
        distribution_counts,
    )
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct DistributionCountKey {
    pub(super) mins: [u8; COMBAT_STAT_COUNT],
    pub(super) maxs: [u8; COMBAT_STAT_COUNT],
    pub(super) active: [bool; COMBAT_STAT_COUNT],
    pub(super) remaining_free: u16,
}

impl RelevantStatSearch {
    pub(super) fn shape(&self) -> DpSearchShape {
        DpSearchShape {
            mins: self.mins,
            maxs: self.maxs,
            active: self.active,
        }
    }

    pub(super) fn new(
        request: &OptimizeRequest,
        constraints: CombatConstraints,
        weapon: &Weapon,
        active: [bool; COMBAT_STAT_COUNT],
        distribution_counts: &mut HashMap<DistributionCountKey, u64>,
    ) -> Option<Self> {
        let mut mins = constraints.mins;
        let maxs = constraints.maxs;
        let mut remaining_free = constraints.remaining_free;
        let requirement_mins = weapon_requirement_mins(request, weapon);
        for idx in 0..COMBAT_STAT_COUNT {
            if requirement_mins[idx] > maxs[idx] {
                return None;
            }
            if requirement_mins[idx] > mins[idx] {
                let raise = u16::from(requirement_mins[idx] - mins[idx]);
                if raise > remaining_free {
                    return None;
                }
                mins[idx] = requirement_mins[idx];
                remaining_free -= raise;
            }
        }

        let count_key = DistributionCountKey {
            mins,
            maxs,
            active,
            remaining_free,
        };
        let candidate_count = *distribution_counts
            .entry(count_key)
            .or_insert_with(|| count_relevant_distributions(&mins, &maxs, &active, remaining_free));
        (candidate_count > 0).then_some(Self {
            mins,
            maxs,
            active,
            remaining_free,
            candidate_count,
        })
    }

    pub(super) fn visit<F>(&self, current: &mut [u8; COMBAT_STAT_COUNT], mut visitor: F)
    where
        F: FnMut(&[u8; COMBAT_STAT_COUNT]) -> bool,
    {
        visit_relevant_stat_candidates_inner(0, self.remaining_free, self, current, &mut visitor);
    }

    pub(super) fn inactive_capacity(&self) -> u16 {
        (0..COMBAT_STAT_COUNT)
            .filter(|idx| !self.active[*idx])
            .map(|idx| u16::from(self.maxs[idx] - self.mins[idx]))
            .sum()
    }

    pub(super) fn min_active_spend(&self) -> u16 {
        self.remaining_free.saturating_sub(self.inactive_capacity())
    }

    pub(super) fn max_active_spend(&self) -> u16 {
        let active_capacity: u16 = (0..COMBAT_STAT_COUNT)
            .filter(|idx| self.active[*idx])
            .map(|idx| u16::from(self.maxs[idx] - self.mins[idx]))
            .sum();
        self.remaining_free.min(active_capacity)
    }
}

pub(super) fn count_relevant_distributions(
    mins: &[u8; COMBAT_STAT_COUNT],
    maxs: &[u8; COMBAT_STAT_COUNT],
    active: &[bool; COMBAT_STAT_COUNT],
    remaining_free: u16,
) -> u64 {
    let budget = usize::from(remaining_free);
    let inactive_capacity: usize = (0..COMBAT_STAT_COUNT)
        .filter(|&stat| !active[stat])
        .map(|stat| usize::from(maxs[stat] - mins[stat]))
        .sum();
    let mut counts = vec![0u64; budget + 1];
    counts[0] = 1;
    for stat in 0..COMBAT_STAT_COUNT {
        if !active[stat] {
            continue;
        }
        let cap = usize::from(maxs[stat] - mins[stat]);
        let mut next = vec![0u64; budget + 1];
        let mut window = 0u64;
        for spent in 0..=budget {
            // Five u8 stat domains contain at most 256^5 distributions.
            window += counts[spent];
            if spent > cap {
                window -= counts[spent - cap - 1];
            }
            next[spent] = window;
        }
        counts = next;
    }
    counts[budget.saturating_sub(inactive_capacity)..]
        .iter()
        .sum()
}

pub(super) fn visit_relevant_stat_candidates_inner<F>(
    idx: usize,
    remaining_free: u16,
    search: &RelevantStatSearch,
    current: &mut [u8; COMBAT_STAT_COUNT],
    visitor: &mut F,
) -> bool
where
    F: FnMut(&[u8; COMBAT_STAT_COUNT]) -> bool,
{
    if idx == COMBAT_STAT_COUNT {
        if remaining_free > search.inactive_capacity() {
            return true;
        }
        let mut filled = *current;
        fill_inactive_stats(search, &mut filled, remaining_free);
        return visitor(&filled);
    }

    if !search.active[idx] {
        current[idx] = search.mins[idx];
        return visit_relevant_stat_candidates_inner(
            idx + 1,
            remaining_free,
            search,
            current,
            visitor,
        );
    }

    let cap = u16::from(search.maxs[idx] - search.mins[idx]).min(remaining_free);
    for add in 0..=cap {
        current[idx] = search.mins[idx] + (add as u8);
        if !visit_relevant_stat_candidates_inner(
            idx + 1,
            remaining_free - add,
            search,
            current,
            visitor,
        ) {
            current[idx] = search.mins[idx];
            return false;
        }
    }
    current[idx] = search.mins[idx];
    true
}

#[allow(clippy::needless_range_loop)]
pub(super) fn fill_inactive_stats(
    search: &RelevantStatSearch,
    current: &mut [u8; COMBAT_STAT_COUNT],
    mut remaining_free: u16,
) {
    for idx in 0..COMBAT_STAT_COUNT {
        if search.active[idx] {
            continue;
        }
        let later_capacity: u16 = ((idx + 1)..COMBAT_STAT_COUNT)
            .filter(|later| !search.active[*later])
            .map(|later| u16::from(search.maxs[later] - search.mins[later]))
            .sum();
        let cap = u16::from(search.maxs[idx] - search.mins[idx]).min(remaining_free);
        let add = remaining_free.saturating_sub(later_capacity).min(cap);
        current[idx] = search.mins[idx] + (add as u8);
        remaining_free -= add;
    }
}

pub(super) fn weapon_requirements_can_fit(
    request: &OptimizeRequest,
    constraints: CombatConstraints,
    weapon: &Weapon,
) -> bool {
    let requirement_mins = weapon_requirement_mins(request, weapon);
    let mut remaining_free = constraints.remaining_free;
    let mut capacity = 0_u16;
    for (idx, &requirement_min) in requirement_mins.iter().enumerate() {
        if requirement_min > constraints.maxs[idx] {
            return false;
        }
        let minimum = constraints.mins[idx].max(requirement_min);
        let raise = u16::from(minimum - constraints.mins[idx]);
        if raise > remaining_free {
            return false;
        }
        remaining_free -= raise;
        capacity = capacity.saturating_add(u16::from(constraints.maxs[idx] - minimum));
    }
    remaining_free <= capacity
}

pub(super) fn weapon_requirement_mins(
    request: &OptimizeRequest,
    weapon: &Weapon,
) -> [u8; COMBAT_STAT_COUNT] {
    std::array::from_fn(|idx| {
        if idx == STAT_STR {
            minimum_str_for_requirement(
                weapon.requirements[STAT_STR],
                weapon_uses_two_handing(request, weapon),
                weapon.disable_two_hand_bonus,
            )
        } else {
            weapon.requirements[idx]
        }
    })
}

pub(super) fn active_stats_for_choice(
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    data: &GameData,
) -> [bool; COMBAT_STAT_COUNT] {
    std::array::from_fn(|idx| {
        weapon_stat_can_increase_ar(prepared.weapon, data, idx)
            || stat_can_increase_bleed_for_choice(prepared, aow_choice, data, idx)
            || aow_choice
                .attack_rows
                .iter()
                .any(|row| attack_row_stat_can_increase_damage(prepared.weapon, row, data, idx))
    })
}

pub(super) fn attack_row_stat_can_increase_damage(
    weapon: &Weapon,
    row: &AowAttackRow,
    data: &GameData,
    stat_idx: usize,
) -> bool {
    if row.is_lacking_fp || !row.is_damaging() || row.has_fixed_damage(&data.profile_id) {
        return false;
    }
    DamageType::ALL.iter().any(|damage_type| {
        let damage_idx = damage_type.as_index();
        let has_damage_base = weapon.base[damage_idx] > 0.0 && row.motion_values[damage_idx] > 0.0
            || row.uses_fixed_attack_base() && row.attack_base[damage_idx] > 0.0;
        if !has_damage_base {
            return false;
        }
        damage_type_stat_can_scale(
            weapon,
            data,
            stat_idx,
            *damage_type,
            row.overwrite_attack_element_correct_id,
        )
    })
}

pub(super) fn stat_can_increase_bleed_for_choice(
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    data: &GameData,
    stat_idx: usize,
) -> bool {
    if !data.rules.status_buildup_scales {
        return false;
    }

    prepared.upgrades.iter().any(|upgrade| {
        let mut source = data.weapon_passive(prepared.weapon.weapon_id);
        if let Some(overlay) = data.weapon_passive_overlay(prepared.weapon.weapon_id, *upgrade) {
            crate::math::merge_status_value(
                &mut source.buildup.bleed,
                &mut source.correction_flags.bleed,
                overlay.buildup.bleed,
                overlay.correction_flags.bleed,
            );
        }
        bleed_source_stat_can_scale(source, prepared.weapon, stat_idx)
    }) || aow_choice.aow.is_some_and(|aow| {
        bleed_source_stat_can_scale(aow_status_source(aow), prepared.weapon, stat_idx)
    })
}

pub(super) fn aow_status_source(aow: &Aow) -> StatusEffectSource {
    StatusEffectSource {
        buildup: aow.scaling_status_add,
        correction_flags: aow.scaling_status_flags,
    }
}

pub(super) fn bleed_source_stat_can_scale(
    source: StatusEffectSource,
    weapon: &Weapon,
    stat_idx: usize,
) -> bool {
    if stat_idx != STAT_ARC || weapon.scaling[stat_idx] <= 0.0 {
        return false;
    }
    status_value_can_scale(source.buildup.bleed, source.correction_flags.bleed)
}

pub(super) fn status_value_can_scale(value: f32, flag: Option<bool>) -> bool {
    value > 0.0 && crate::math::uses_status_correction(flag, true)
}

pub(super) fn minimum_str_for_requirement(
    requirement: u8,
    two_handing: bool,
    disable_two_hand_bonus: bool,
) -> u8 {
    if !two_handing || disable_two_hand_bonus {
        return requirement;
    }
    for candidate in 0..=requirement {
        if effective_str(candidate, true, false) >= u16::from(requirement) {
            return candidate;
        }
    }
    requirement
}

pub(super) fn weapon_stat_can_increase_ar(
    weapon: &Weapon,
    data: &GameData,
    stat_idx: usize,
) -> bool {
    DamageType::ALL.iter().any(|damage_type| {
        weapon.base[damage_type.as_index()] > 0.0
            && damage_type_stat_can_scale(weapon, data, stat_idx, *damage_type, None)
    })
}

pub(super) fn damage_type_stat_can_scale(
    weapon: &Weapon,
    data: &GameData,
    stat_idx: usize,
    damage_type: DamageType,
    override_id: Option<usize>,
) -> bool {
    let id = override_id.unwrap_or(weapon.attack_element_correct_id);
    let damage_idx = damage_type.as_index();
    if let Some(ext) = data.attack_element_ext(id) {
        return ext.stat_scales(stat_idx, damage_idx)
            && ext
                .overwrite_rate(stat_idx, damage_idx)
                .unwrap_or(weapon.scaling[stat_idx] * ext.influence_rate(stat_idx, damage_idx))
                > 0.0;
    }
    override_id.is_none()
        && weapon.scaling[stat_idx] > 0.0
        && data
            .attack_element(id)
            .is_none_or(|aec| aec.stat_scales(stat_idx, damage_type))
}

#[cfg(test)]
#[allow(clippy::needless_range_loop)]
pub(super) fn count_stat_candidates(constraints: CombatConstraints) -> u64 {
    let mut caps = [0_u8; COMBAT_STAT_COUNT];
    for idx in 0..COMBAT_STAT_COUNT {
        caps[idx] = constraints.maxs[idx] - constraints.mins[idx];
    }
    let mut memo: HashMap<(usize, u16), u64> = HashMap::new();
    count_distributions(&caps, 0, constraints.remaining_free, &mut memo)
}

#[cfg(test)]
pub(super) fn count_distributions(
    caps: &[u8; COMBAT_STAT_COUNT],
    idx: usize,
    remaining: u16,
    memo: &mut HashMap<(usize, u16), u64>,
) -> u64 {
    if idx == COMBAT_STAT_COUNT {
        return if remaining == 0 { 1 } else { 0 };
    }
    if let Some(value) = memo.get(&(idx, remaining)) {
        return *value;
    }

    let mut total = 0_u64;
    let max_add = u16::from(caps[idx]).min(remaining);
    for add in 0..=max_add {
        total = total.saturating_add(count_distributions(caps, idx + 1, remaining - add, memo));
    }
    memo.insert((idx, remaining), total);
    total
}
