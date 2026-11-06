//! Turns a client request into the core's request. A command validates the client's
//! request here once, at entry. Every rewrite it applies afterwards (a pinned loadout,
//! locked stats, clamped upgrade caps) derives from that validated request in this module.
//! Errors name a field, never the client's value.

use std::collections::HashSet;

use er_optimizer_core::model::COMBAT_STAT_COUNT;
use er_optimizer_core::{FilterDimension, OptimizeRequest, SomberFilter, StableFilter};

use crate::commands::data::{class_metadata, uses_class_budget, weapon_upgrade_cap};
use crate::dto::{CombatStateDto, OptimizeRequestDto, check_len};
use crate::errors::AppError;
use crate::{JobRegistry, ProfileData};

pub const STAT_LABELS: [&str; COMBAT_STAT_COUNT] = ["str", "dex", "int", "fai", "arc"];

/// The client's request, validated against the profile it names. A cap the client left
/// out is the profile's own maximum.
pub fn validate(
    dto: &OptimizeRequestDto,
    profile: &ProfileData,
) -> Result<OptimizeRequest, AppError> {
    let rules = &profile.data.rules;
    let request = dto.to_core(rules.standard_max_upgrade, rules.somber_max_upgrade)?;
    let profile_id = &profile.data_manifest.profile.id;
    if *profile_id != dto.profile_id {
        return Err(AppError::new(
            "profileId is not an available game profile. Reload the catalog and choose an available profile.",
        ));
    }
    if !class_metadata(profile)
        .iter()
        .any(|class_info| class_info.name.eq_ignore_ascii_case(&request.class_name))
    {
        return Err(AppError::new(format!(
            "the starting class is not available for profile '{profile_id}'"
        )));
    }
    if !uses_class_budget(profile, &request.class_name) {
        check_custom_stats(profile_id, &request)?;
    }
    if request.exact_upgrade {
        for (kind, requested, supported) in [
            (
                "standard",
                request.standard_max_upgrade,
                rules.standard_max_upgrade,
            ),
            (
                "somber",
                request.somber_max_upgrade,
                rules.somber_max_upgrade,
            ),
        ] {
            if requested > supported {
                return Err(AppError::new(format!(
                    "profile '{profile_id}' does not support exact {kind} +{requested}; its maximum is +{supported}"
                )));
            }
        }
    }
    if let Some(aow_name) = &request.aow_name {
        check_aow_known(profile, aow_name)?;
    }
    check_weapon_families(profile, &request.filters)?;
    Ok(request)
}

/// A Custom stats request states its eight stats exactly: the level is their total and
/// every combat stat is locked to its current value.
fn check_custom_stats(profile_id: &str, request: &OptimizeRequest) -> Result<(), AppError> {
    let stats = request.current_stats;
    let level_from_stats = [
        stats.vig, stats.mnd, stats.end, stats.str, stats.dex, stats.int, stats.fai, stats.arc,
    ]
    .into_iter()
    .map(u16::from)
    .sum::<u16>();
    if request.character_level != level_from_stats {
        return Err(AppError::new(format!(
            "custom stat profile '{profile_id}' requires character level {level_from_stats} for the selected stats; got {}",
            request.character_level
        )));
    }
    for (label, lock, current) in STAT_LABELS
        .into_iter()
        .zip(request.locked_combat_stats)
        .zip(stats.combat_array())
        .map(|((label, lock), current)| (label, lock, current))
    {
        if lock != Some(current) {
            return Err(AppError::new(format!(
                "custom stat profile '{profile_id}' requires {label} to be locked to its current value"
            )));
        }
    }
    Ok(())
}

/// Rejects an Ash of War the profile does not have, as the core would, without echoing it.
pub fn check_aow_known(profile: &ProfileData, name: &str) -> Result<(), AppError> {
    let data = &profile.data;
    let known = data
        .aows
        .iter()
        .any(|aow| aow.name.eq_ignore_ascii_case(name))
        || data.weapons.iter().any(|weapon| {
            data.native_skill_compatible_with_weapon(weapon)
                && weapon
                    .native_skill_name
                    .as_deref()
                    .or_else(|| {
                        data.native_skill_attack_rows(weapon.weapon_id)
                            .first()
                            .map(|row| row.aow_name.as_str())
                    })
                    .is_some_and(|skill| skill.eq_ignore_ascii_case(name))
        });
    if known {
        Ok(())
    } else {
        Err(AppError::new(
            "aowName is not an Ash of War of this profile",
        ))
    }
}

/// Rejects a weapon family filter the profile no longer has, as the core would, without
/// echoing it.
fn check_weapon_families(profile: &ProfileData, filters: &[StableFilter]) -> Result<(), AppError> {
    let mut families: Option<HashSet<String>> = None;
    for filter in filters
        .iter()
        .filter(|filter| filter.dimension == FilterDimension::WeaponFamily)
    {
        let families = families.get_or_insert_with(|| {
            profile
                .data
                .weapons
                .iter()
                .map(|weapon| weapon.family_filter_id().to_ascii_lowercase())
                .collect()
        });
        if !families.contains(&filter.id.to_ascii_lowercase()) {
            return Err(AppError::new(
                "A weapon family filter is no longer valid for this profile; remove it and reselect the weapon family",
            ));
        }
    }
    Ok(())
}

/// Fixes the request to one loadout and drops the discovery query around it.
pub fn pin_loadout(
    request: &mut OptimizeRequest,
    weapon_name: &str,
    affinity: Option<&str>,
    aow_name: Option<&str>,
) {
    request.weapon_name = Some(weapon_name.to_owned());
    request.affinity = affinity.map(str::to_owned);
    request.aow_name = aow_name.map(str::to_owned);
    request.weapon_type_key = None;
    request.somber_filter = SomberFilter::All;
    request.filters.clear();
    request.top_k = 1;
}

pub fn lock_to_stats(request: &mut OptimizeRequest, stats: CombatStateDto) {
    request.locked_combat_stats = stats.to_array().map(Some);
}

pub fn lock_to_current_stats(request: &mut OptimizeRequest) {
    request.locked_combat_stats = request.current_stats.combat_array().map(Some);
}

pub fn unlock_stats(request: &mut OptimizeRequest) {
    request.locked_combat_stats = [None; COMBAT_STAT_COUNT];
}

pub fn set_min_stats(request: &mut OptimizeRequest, mins: [u8; COMBAT_STAT_COUNT]) {
    request.min_combat_stats = mins;
}

pub fn set_exact_upgrade(request: &mut OptimizeRequest, upgrade: u8, is_somber: bool) {
    if is_somber {
        request.somber_max_upgrade = upgrade;
    } else {
        request.standard_max_upgrade = upgrade;
    }
    request.exact_upgrade = true;
}

/// Asks for every upgrade through `max_upgrade` of one reinforcement type; the other
/// type takes the profile's own maximum.
pub fn set_upgrade_range(
    request: &mut OptimizeRequest,
    profile: &ProfileData,
    is_somber: bool,
    max_upgrade: u8,
) {
    request.exact_upgrade = false;
    request.standard_max_upgrade = profile.data.rules.standard_max_upgrade;
    request.somber_max_upgrade = profile.data.rules.somber_max_upgrade;
    if is_somber {
        request.somber_max_upgrade = max_upgrade;
    } else {
        request.standard_max_upgrade = max_upgrade;
    }
}

/// Fits the upgrade caps to the profile and to the pinned weapon. An exact upgrade is
/// never lowered: a weapon without that level is rejected.
pub fn settle_upgrade_caps(
    request: &mut OptimizeRequest,
    profile: &ProfileData,
) -> Result<(), AppError> {
    let weapon = match request.weapon_name.as_deref() {
        Some(weapon_name) => Some(weapon_reinforcement_info(
            profile,
            weapon_name,
            request.affinity.as_deref(),
        )?),
        None => None,
    };
    if request.exact_upgrade {
        let Some((is_somber, _)) = weapon else {
            return Ok(());
        };
        let requested = if is_somber {
            request.somber_max_upgrade
        } else {
            request.standard_max_upgrade
        };
        let available = profile.data.weapons.iter().any(|weapon| {
            request
                .weapon_name
                .as_deref()
                .is_some_and(|name| weapon.name.eq_ignore_ascii_case(name))
                && request
                    .affinity
                    .as_deref()
                    .is_none_or(|affinity| weapon.affinity.eq_ignore_ascii_case(affinity))
                && profile
                    .data
                    .reinforce_level(weapon.reinforce_type, requested)
                    .is_some()
        });
        if !available {
            return Err(AppError::new(format!(
                "the selected weapon does not support exact +{requested}; choose an available reinforcement level or use range mode"
            )));
        }
        return Ok(());
    }
    request.standard_max_upgrade = request
        .standard_max_upgrade
        .min(profile.data.rules.standard_max_upgrade);
    request.somber_max_upgrade = request
        .somber_max_upgrade
        .min(profile.data.rules.somber_max_upgrade);
    match weapon {
        Some((true, weapon_cap)) => {
            request.somber_max_upgrade = request.somber_max_upgrade.min(weapon_cap);
        }
        Some((false, weapon_cap)) => {
            request.standard_max_upgrade = request.standard_max_upgrade.min(weapon_cap);
        }
        None => {}
    }
    Ok(())
}

/// Whether the named weapon is somber, and the highest upgrade it supports.
fn weapon_reinforcement_info(
    profile: &ProfileData,
    weapon_name: &str,
    affinity: Option<&str>,
) -> Result<(bool, u8), AppError> {
    let mut matches = profile.data.weapons.iter().filter(|weapon| {
        weapon.name.eq_ignore_ascii_case(weapon_name)
            && affinity.is_none_or(|value| weapon.affinity.eq_ignore_ascii_case(value))
    });
    let Some(first) = matches.next() else {
        return Err(AppError::new("weapon not found in profile data"));
    };
    let is_somber = first.is_somber;
    if matches.any(|weapon| weapon.is_somber != is_somber) {
        return Err(AppError::new(
            "the weapon has mixed reinforcement types; specify an affinity",
        ));
    }
    let profile_upgrade_cap = weapon_upgrade_cap(&profile.catalog_index, weapon_name, affinity)?;
    Ok((is_somber, profile_upgrade_cap))
}

/// Cancels a job after checking the client's ID, which would otherwise reach the registry unbounded.
pub fn cancel_job<T: Clone + Send + 'static>(
    jobs: &JobRegistry<T>,
    job_id: &str,
) -> Result<bool, AppError> {
    check_len("jobId", job_id)?;
    jobs.cancel(job_id)
}

/// Reads a job's status after checking the client's ID.
pub fn job_status<T: Clone + Send + 'static>(
    jobs: &JobRegistry<T>,
    job_id: &str,
) -> Result<Option<T>, AppError> {
    check_len("jobId", job_id)?;
    jobs.status(job_id)
}
