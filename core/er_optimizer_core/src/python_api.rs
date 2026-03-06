#![allow(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::data::load_game_data;
use crate::model::Stats;
use crate::optimizer::{
    aow_compatible_with_weapon, estimate_search_space, optimize, optimize_with_progress,
    OptimizeObjective, OptimizeRequest, OptimizeResult, SearchEstimate, SomberFilter,
};

#[pyclass(name = "GameData")]
pub struct PyGameData {
    inner: crate::model::GameData,
}

#[pymethods]
impl PyGameData {
    #[getter]
    fn weapon_count(&self) -> usize {
        self.inner.weapons.len()
    }

    #[getter]
    fn aow_count(&self) -> usize {
        self.inner.aows.len()
    }

    fn weapon_names(&self) -> Vec<String> {
        let mut set = BTreeSet::new();
        for weapon in &self.inner.weapons {
            set.insert(weapon.name.clone());
        }
        set.into_iter().collect()
    }

    fn affinities_for_weapon(&self, weapon_name: &str) -> Vec<String> {
        let mut set = BTreeSet::new();
        for weapon in &self.inner.weapons {
            if weapon.name.eq_ignore_ascii_case(weapon_name) {
                set.insert(weapon.affinity.clone());
            }
        }
        set.into_iter().collect()
    }

    fn aow_names(&self) -> Vec<String> {
        let mut set = BTreeSet::new();
        for aow in &self.inner.aows {
            set.insert(aow.name.clone());
        }
        set.into_iter().collect()
    }

    #[pyo3(signature = (weapon_name, affinity=None))]
    fn compatible_aow_names(&self, weapon_name: &str, affinity: Option<&str>) -> Vec<String> {
        let mut set = BTreeSet::new();
        for weapon in &self.inner.weapons {
            if !weapon.name.eq_ignore_ascii_case(weapon_name) {
                continue;
            }
            if let Some(aff) = affinity {
                if !weapon.affinity.eq_ignore_ascii_case(aff) {
                    continue;
                }
            }
            for aow in &self.inner.aows {
                if aow_compatible_with_weapon(aow, weapon, &self.inner) {
                    set.insert(aow.name.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    #[pyo3(signature = (affinity=None))]
    fn compatible_aow_names_for_affinity(&self, affinity: Option<&str>) -> Vec<String> {
        let mut set = BTreeSet::new();
        for weapon in &self.inner.weapons {
            if let Some(aff) = affinity {
                if !weapon.affinity.eq_ignore_ascii_case(aff) {
                    continue;
                }
            }
            for aow in &self.inner.aows {
                if aow_compatible_with_weapon(aow, weapon, &self.inner) {
                    set.insert(aow.name.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    fn weapon_scaling(&self, weapon_name: &str, affinity: &str) -> PyResult<(f32, f32, f32, f32, f32)> {
        let Some(weapon) = self
            .inner
            .weapons
            .iter()
            .find(|weapon| {
                weapon.name.eq_ignore_ascii_case(weapon_name)
                    && weapon.affinity.eq_ignore_ascii_case(affinity)
            })
        else {
            return Err(PyValueError::new_err(format!(
                "weapon not found for scaling lookup: {weapon_name} | {affinity}"
            )));
        };
        Ok((
            weapon.scaling[0],
            weapon.scaling[1],
            weapon.scaling[2],
            weapon.scaling[3],
            weapon.scaling[4],
        ))
    }

    fn weapon_scaling_for_upgrade(
        &self,
        weapon_name: &str,
        affinity: &str,
        upgrade: u8,
    ) -> PyResult<(f32, f32, f32, f32, f32)> {
        let Some(weapon) = self
            .inner
            .weapons
            .iter()
            .find(|weapon| {
                weapon.name.eq_ignore_ascii_case(weapon_name)
                    && weapon.affinity.eq_ignore_ascii_case(affinity)
            })
        else {
            return Err(PyValueError::new_err(format!(
                "weapon not found for scaling lookup: {weapon_name} | {affinity}"
            )));
        };
        let Some(reinforce) = self.inner.reinforce_level(weapon.reinforce_type, upgrade) else {
            return Err(PyValueError::new_err(format!(
                "missing reinforce level for scaling lookup: type={} level={upgrade}",
                weapon.reinforce_type
            )));
        };
        Ok((
            weapon.scaling[0] * reinforce.scaling_mult[0],
            weapon.scaling[1] * reinforce.scaling_mult[1],
            weapon.scaling[2] * reinforce.scaling_mult[2],
            weapon.scaling[3] * reinforce.scaling_mult[3],
            weapon.scaling[4] * reinforce.scaling_mult[4],
        ))
    }

    fn weapon_type_keys(&self) -> Vec<String> {
        let mut set = BTreeSet::new();
        for weapon in &self.inner.weapons {
            for key in weapon.weapon_type_keys.split('|') {
                if !key.is_empty() {
                    set.insert(key.to_string());
                }
            }
        }
        set.into_iter().collect()
    }

    #[pyo3(signature = (weapon_type_key=None))]
    fn weapon_names_for_type(&self, weapon_type_key: Option<&str>) -> Vec<String> {
        let mut set = BTreeSet::new();
        for weapon in &self.inner.weapons {
            if let Some(key) = weapon_type_key {
                let matches = weapon
                    .weapon_type_keys
                    .split('|')
                    .any(|value| value.eq_ignore_ascii_case(key));
                if !matches {
                    continue;
                }
            }
            set.insert(weapon.name.clone());
        }
        set.into_iter().collect()
    }

    #[pyo3(signature = (weapon_name, affinity=None))]
    fn weapon_requirements(
        &self,
        weapon_name: &str,
        affinity: Option<&str>,
    ) -> PyResult<(u8, u8, u8, u8, u8)> {
        let mut best: Option<[u8; 5]> = None;
        for weapon in &self.inner.weapons {
            if !weapon.name.eq_ignore_ascii_case(weapon_name) {
                continue;
            }
            if let Some(aff) = affinity {
                if !weapon.affinity.eq_ignore_ascii_case(aff) {
                    continue;
                }
            }
            best = Some(match best {
                Some(current) => [
                    current[0].max(weapon.requirements[0]),
                    current[1].max(weapon.requirements[1]),
                    current[2].max(weapon.requirements[2]),
                    current[3].max(weapon.requirements[3]),
                    current[4].max(weapon.requirements[4]),
                ],
                None => weapon.requirements,
            });
        }

        let Some(reqs) = best else {
            return Err(PyValueError::new_err(format!(
                "weapon not found for requirements lookup: {weapon_name}"
            )));
        };
        Ok((reqs[0], reqs[1], reqs[2], reqs[3], reqs[4]))
    }
}

#[pyclass(name = "SearchEstimate", get_all)]
pub struct PySearchEstimate {
    pub weapon_candidates: usize,
    pub stat_candidates: u64,
    pub combinations: u64,
}

impl From<SearchEstimate> for PySearchEstimate {
    fn from(value: SearchEstimate) -> Self {
        Self {
            weapon_candidates: value.weapon_candidates,
            stat_candidates: value.stat_candidates,
            combinations: value.combinations,
        }
    }
}

#[pyclass(name = "OptimizeResult", get_all)]
#[derive(Clone)]
pub struct PyOptimizeResult {
    pub weapon_id: u32,
    pub weapon_name: String,
    pub affinity: String,
    pub is_somber: bool,
    pub upgrade: u8,
    pub vig: u8,
    pub mnd: u8,
    pub end: u8,
    pub str_stat: u8,
    pub dex: u8,
    pub int_stat: u8,
    pub fai: u8,
    pub arc: u8,
    pub ar_total: f32,
    pub ar_physical: f32,
    pub ar_magic: f32,
    pub ar_fire: f32,
    pub ar_lightning: f32,
    pub ar_holy: f32,
    pub aow_id: Option<u16>,
    pub aow_name: Option<String>,
    pub bleed_buildup: f32,
    pub bleed_buildup_add: f32,
    pub frost_buildup: f32,
    pub poison_buildup: f32,
    pub aow_first_hit_damage: f32,
    pub aow_full_sequence_damage: f32,
    pub score: f32,
}

impl From<OptimizeResult> for PyOptimizeResult {
    fn from(value: OptimizeResult) -> Self {
        Self {
            weapon_id: value.weapon_id,
            weapon_name: value.weapon_name,
            affinity: value.affinity,
            is_somber: value.is_somber,
            upgrade: value.upgrade,
            vig: value.stats.vig,
            mnd: value.stats.mnd,
            end: value.stats.end,
            str_stat: value.stats.str,
            dex: value.stats.dex,
            int_stat: value.stats.int,
            fai: value.stats.fai,
            arc: value.stats.arc,
            ar_total: value.ar.total(),
            ar_physical: value.ar.physical,
            ar_magic: value.ar.magic,
            ar_fire: value.ar.fire,
            ar_lightning: value.ar.lightning,
            ar_holy: value.ar.holy,
            aow_id: value.aow_id,
            aow_name: value.aow_name,
            bleed_buildup: value.bleed_buildup,
            bleed_buildup_add: value.bleed_buildup_add,
            frost_buildup: value.frost_buildup,
            poison_buildup: value.poison_buildup,
            aow_first_hit_damage: value.aow_first_hit_damage,
            aow_full_sequence_damage: value.aow_full_sequence_damage,
            score: value.score,
        }
    }
}

#[pyfunction(name = "load_game_data")]
pub fn py_load_game_data(data_dir: &str) -> PyResult<PyGameData> {
    load_game_data(data_dir)
        .map(|inner| PyGameData { inner })
        .map_err(PyValueError::new_err)
}

#[pyfunction(
    name = "estimate_search_space",
    signature = (
        data,
        class_name,
        character_level,
        vig,
        mnd,
        end,
        str_stat,
        dex,
        int_stat,
        fai,
        arc,
        max_upgrade,
        two_handing=false,
        weapon_name=None,
        affinity=None,
        aow_name=None,
        objective="max_ar",
        weapon_type_key=None,
        somber_filter="all",
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        lock_str=None,
        lock_dex=None,
        lock_int=None,
        lock_fai=None,
        lock_arc=None,
        fixed_upgrade=None
    )
)]
#[allow(clippy::too_many_arguments)]
pub fn py_estimate_search_space(
    data: &PyGameData,
    class_name: &str,
    character_level: u16,
    vig: u8,
    mnd: u8,
    end: u8,
    str_stat: u8,
    dex: u8,
    int_stat: u8,
    fai: u8,
    arc: u8,
    max_upgrade: u8,
    two_handing: bool,
    weapon_name: Option<String>,
    affinity: Option<String>,
    aow_name: Option<String>,
    objective: &str,
    weapon_type_key: Option<String>,
    somber_filter: &str,
    min_str: u8,
    min_dex: u8,
    min_int: u8,
    min_fai: u8,
    min_arc: u8,
    lock_str: Option<u8>,
    lock_dex: Option<u8>,
    lock_int: Option<u8>,
    lock_fai: Option<u8>,
    lock_arc: Option<u8>,
    fixed_upgrade: Option<u8>,
) -> PyResult<PySearchEstimate> {
    let request = build_request(
        class_name,
        character_level,
        Stats {
            vig,
            mnd,
            end,
            str: str_stat,
            dex,
            int: int_stat,
            fai,
            arc,
        },
        max_upgrade,
        fixed_upgrade,
        two_handing,
        weapon_name,
        affinity,
        aow_name,
        objective,
        weapon_type_key,
        somber_filter,
        [min_str, min_dex, min_int, min_fai, min_arc],
        [lock_str, lock_dex, lock_int, lock_fai, lock_arc],
        1,
    )
    .map_err(PyValueError::new_err)?;

    estimate_search_space(&request, &data.inner)
        .map(PySearchEstimate::from)
        .map_err(PyValueError::new_err)
}

#[pyfunction(
    name = "optimize_builds",
    signature = (
        data,
        class_name,
        character_level,
        vig,
        mnd,
        end,
        str_stat,
        dex,
        int_stat,
        fai,
        arc,
        max_upgrade,
        two_handing=false,
        weapon_name=None,
        affinity=None,
        aow_name=None,
        objective="max_ar",
        top_k=10,
        weapon_type_key=None,
        somber_filter="all",
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        lock_str=None,
        lock_dex=None,
        lock_int=None,
        lock_fai=None,
        lock_arc=None,
        fixed_upgrade=None,
        progress_every=5000,
        progress_cb=None
    )
)]
#[allow(clippy::too_many_arguments)]
pub fn py_optimize_builds(
    py: Python<'_>,
    data: &PyGameData,
    class_name: &str,
    character_level: u16,
    vig: u8,
    mnd: u8,
    end: u8,
    str_stat: u8,
    dex: u8,
    int_stat: u8,
    fai: u8,
    arc: u8,
    max_upgrade: u8,
    two_handing: bool,
    weapon_name: Option<String>,
    affinity: Option<String>,
    aow_name: Option<String>,
    objective: &str,
    top_k: usize,
    weapon_type_key: Option<String>,
    somber_filter: &str,
    min_str: u8,
    min_dex: u8,
    min_int: u8,
    min_fai: u8,
    min_arc: u8,
    lock_str: Option<u8>,
    lock_dex: Option<u8>,
    lock_int: Option<u8>,
    lock_fai: Option<u8>,
    lock_arc: Option<u8>,
    fixed_upgrade: Option<u8>,
    progress_every: u64,
    progress_cb: Option<Py<PyAny>>,
) -> PyResult<Vec<PyOptimizeResult>> {
    let request = build_request(
        class_name,
        character_level,
        Stats {
            vig,
            mnd,
            end,
            str: str_stat,
            dex,
            int: int_stat,
            fai,
            arc,
        },
        max_upgrade,
        fixed_upgrade,
        two_handing,
        weapon_name,
        affinity,
        aow_name,
        objective,
        weapon_type_key,
        somber_filter,
        [min_str, min_dex, min_int, min_fai, min_arc],
        [lock_str, lock_dex, lock_int, lock_fai, lock_arc],
        top_k,
    )
    .map_err(PyValueError::new_err)?;

    let callback_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let callback_error_ref = Arc::clone(&callback_error);
    let data_ref = &data.inner;

    let raw_result = py.allow_threads(|| {
        if let Some(callback) = progress_cb {
            optimize_with_progress(&request, data_ref, progress_every, |snapshot| {
                if callback_error_ref.lock().ok().and_then(|v| v.clone()).is_some() {
                    return;
                }
                Python::with_gil(|gil| {
                    if let Err(err) = callback.call1(
                        gil,
                        (
                            snapshot.checked,
                            snapshot.total,
                            snapshot.eligible,
                            snapshot.best_score,
                            snapshot.elapsed_ms,
                        ),
                    ) {
                        if let Ok(mut guard) = callback_error_ref.lock() {
                            *guard = Some(err.to_string());
                        }
                    }
                });
            })
        } else {
            optimize(&request, data_ref)
        }
    });

    if let Ok(guard) = callback_error.lock() {
        if let Some(message) = guard.as_ref() {
            return Err(PyValueError::new_err(message.clone()));
        }
    }

    raw_result
        .map(|rows| rows.into_iter().map(PyOptimizeResult::from).collect())
        .map_err(PyValueError::new_err)
}

#[allow(clippy::too_many_arguments)]
fn build_request(
    class_name: &str,
    character_level: u16,
    current_stats: Stats,
    max_upgrade: u8,
    fixed_upgrade: Option<u8>,
    two_handing: bool,
    weapon_name: Option<String>,
    affinity: Option<String>,
    aow_name: Option<String>,
    objective: &str,
    weapon_type_key: Option<String>,
    somber_filter: &str,
    min_combat_stats: [u8; 5],
    locked_combat_stats: [Option<u8>; 5],
    top_k: usize,
) -> Result<OptimizeRequest, String> {
    let parsed_objective = parse_objective(objective)?;
    let parsed_somber_filter = parse_somber_filter(somber_filter)?;
    Ok(OptimizeRequest {
        class_name: class_name.to_string(),
        character_level,
        current_stats,
        min_combat_stats,
        locked_combat_stats,
        max_upgrade,
        fixed_upgrade,
        two_handing,
        weapon_name,
        affinity,
        aow_name,
        weapon_type_key,
        somber_filter: parsed_somber_filter,
        objective: parsed_objective,
        top_k,
    })
}

fn parse_objective(raw: &str) -> Result<OptimizeObjective, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "max_ar" => Ok(OptimizeObjective::MaxAr),
        "max_ar_plus_bleed" | "max_ar+bleed" | "max_ar_plus_bleed_buildup" => {
            Ok(OptimizeObjective::MaxArPlusBleed)
        }
        "aow_first_hit" | "max_aow_first_hit" => Ok(OptimizeObjective::AowFirstHit),
        "aow_full_sequence" | "max_aow_full_sequence" | "aow_full" => {
            Ok(OptimizeObjective::AowFullSequence)
        }
        _ => Err(format!(
            "invalid objective '{raw}', expected 'max_ar', 'max_ar_plus_bleed', 'aow_first_hit', or 'aow_full_sequence'"
        )),
    }
}

fn parse_somber_filter(raw: &str) -> Result<SomberFilter, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(SomberFilter::All),
        "standard_only" | "standard" => Ok(SomberFilter::StandardOnly),
        "somber_only" | "somber" => Ok(SomberFilter::SomberOnly),
        _ => Err(format!(
            "invalid somber_filter '{raw}', expected 'all', 'standard_only', or 'somber_only'"
        )),
    }
}

#[pymodule]
pub fn er_optimizer_core(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyGameData>()?;
    module.add_class::<PySearchEstimate>()?;
    module.add_class::<PyOptimizeResult>()?;
    module.add_function(wrap_pyfunction!(py_load_game_data, module)?)?;
    module.add_function(wrap_pyfunction!(py_estimate_search_space, module)?)?;
    module.add_function(wrap_pyfunction!(py_optimize_builds, module)?)?;
    Ok(())
}
