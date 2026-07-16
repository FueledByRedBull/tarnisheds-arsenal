use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::model::{
    Aow, AowAttackRow, AowEffect, AowEffectRole, AowRouteAssignment, AttackElementCorrect,
    AttackElementCorrectExt, COMBAT_STAT_COUNT, DAMAGE_TYPE_COUNT, DataCapabilities, DataRules,
    GameData, PhysicalAttackAttribute, ReinforceLevel, StaminaCostMode, StatusBuildup,
    StatusCorrectionFlags, StatusEffectSource, Weapon,
};
use crate::snapshot::{SnapshotManifest, validate_embedded_snapshot, validate_external_snapshot};

const EMBEDDED_DATA_ROOT: &str = "__er_optimizer_embedded_snapshot__";
const EMBEDDED_VANILLA_ROOT: &str = "__er_optimizer_embedded_snapshot__/vanilla";
const EMBEDDED_CONVERGENCE_ROOT: &str = "__er_optimizer_embedded_snapshot__/convergence";
pub const VANILLA_PROFILE_ID: &str = "vanilla";
pub const CONVERGENCE_PROFILE_ID: &str = "convergence";

fn is_embedded_data_path(path: &Path) -> bool {
    path.starts_with(EMBEDDED_DATA_ROOT)
}

#[derive(Clone, Debug, Default)]
struct AowBuffRow {
    buff_attack_power: [f32; DAMAGE_TYPE_COUNT],
    scaling_status_add: StatusBuildup,
    scaling_status_flags: StatusCorrectionFlags,
    persistent_weapon_status_add: StatusBuildup,
    persistent_on_hit_status_add: StatusBuildup,
    activation_action_id: Option<String>,
}

struct CsvTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl CsvTable {
    fn from_path(path: &Path) -> Result<Self, String> {
        if is_embedded_data_path(path) {
            let content = embedded_csv_for_path(path)
                .ok_or_else(|| format!("missing embedded CSV: {}", csv_file_name(path)))?;
            return Self::from_content(format!("embedded:{}", csv_file_name(path)), content);
        }
        let content = fs::read_to_string(path)
            .map_err(|err| format!("failed reading {}: {err}", path.display()))?;
        Self::from_content(path.display().to_string(), &content)
    }

    fn from_optional_path(path: &Path) -> Result<Option<Self>, String> {
        if is_embedded_data_path(path) {
            return embedded_csv_for_path(path)
                .map(|content| {
                    Self::from_content(format!("embedded:{}", csv_file_name(path)), content)
                })
                .transpose();
        }
        if path.exists() {
            return Self::from_path(path).map(Some);
        }
        Ok(None)
    }

    fn from_content(source: String, content: &str) -> Result<Self, String> {
        let mut reader = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .flexible(false)
            .from_reader(Cursor::new(content));
        let headers = reader
            .headers()
            .map_err(|err| format!("{source} has invalid csv headers: {err}"))?
            .iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        if headers.is_empty() {
            return Err(format!("{source} has no headers"));
        }

        let mut rows = Vec::new();
        for record in reader.records() {
            let record = record.map_err(|err| format!("{source} has invalid csv row: {err}"))?;
            rows.push(record.iter().map(str::to_string).collect());
        }
        Ok(Self { headers, rows })
    }

    fn idx(&self, field: &str) -> Result<usize, String> {
        self.headers
            .iter()
            .position(|header| header == field)
            .ok_or_else(|| format!("missing csv column: {field}"))
    }

    fn get<'a>(&self, row: &'a [String], field: &str) -> Result<&'a str, String> {
        let idx = self.idx(field)?;
        Ok(row[idx].as_str())
    }
}

fn csv_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>")
        .to_string()
}

fn embedded_csv_for_path(path: &Path) -> Option<&'static str> {
    let name = path.file_name().and_then(|name| name.to_str())?;
    if path.starts_with(EMBEDDED_CONVERGENCE_ROOT) {
        return embedded_convergence_csv(name);
    }
    embedded_vanilla_csv(name)
}

fn embedded_vanilla_csv(name: &str) -> Option<&'static str> {
    match name {
        "aow.csv" => Some(include_str!("../../../data/phase1/aow.csv")),
        "aow_attack_data.csv" => Some(include_str!("../../../data/phase1/aow_attack_data.csv")),
        "aow_route_assignments.csv" => Some(include_str!(
            "../../../data/phase1/aow_route_assignments.csv"
        )),
        "aow_effect_data.csv" => Some(include_str!("../../../data/phase1/aow_effect_data.csv")),
        "aow_weapon_compat.csv" => Some(include_str!("../../../data/phase1/aow_weapon_compat.csv")),
        "attack_element_correct.csv" => Some(include_str!(
            "../../../data/phase1/attack_element_correct.csv"
        )),
        "attack_element_correct_ext.csv" => Some(include_str!(
            "../../../data/phase1/attack_element_correct_ext.csv"
        )),
        "calc_correct.csv" => Some(include_str!("../../../data/phase1/calc_correct.csv")),
        "native_skill_attack_data.csv" => Some(include_str!(
            "../../../data/phase1/native_skill_attack_data.csv"
        )),
        "reinforce.csv" => Some(include_str!("../../../data/phase1/reinforce.csv")),
        "weapon_passive_overlays.csv" => Some(include_str!(
            "../../../data/phase1/weapon_passive_overlays.csv"
        )),
        "weapon_passives.csv" => Some(include_str!("../../../data/phase1/weapon_passives.csv")),
        "weapons.csv" => Some(include_str!("../../../data/phase1/weapons.csv")),
        _ => None,
    }
}

fn embedded_convergence_csv(name: &str) -> Option<&'static str> {
    match name {
        "aow.csv" => Some(include_str!("../../../data/profiles/convergence/aow.csv")),
        "aow_attack_data.csv" => Some(include_str!(
            "../../../data/profiles/convergence/aow_attack_data.csv"
        )),
        "aow_route_assignments.csv" => Some(include_str!(
            "../../../data/profiles/convergence/aow_route_assignments.csv"
        )),
        "aow_effect_data.csv" => Some(include_str!(
            "../../../data/profiles/convergence/aow_effect_data.csv"
        )),
        "aow_weapon_compat.csv" => Some(include_str!(
            "../../../data/profiles/convergence/aow_weapon_compat.csv"
        )),
        "attack_element_correct.csv" => Some(include_str!(
            "../../../data/profiles/convergence/attack_element_correct.csv"
        )),
        "attack_element_correct_ext.csv" => Some(include_str!(
            "../../../data/profiles/convergence/attack_element_correct_ext.csv"
        )),
        "calc_correct.csv" => Some(include_str!(
            "../../../data/profiles/convergence/calc_correct.csv"
        )),
        "native_skill_attack_data.csv" => Some(include_str!(
            "../../../data/profiles/convergence/native_skill_attack_data.csv"
        )),
        "reinforce.csv" => Some(include_str!(
            "../../../data/profiles/convergence/reinforce.csv"
        )),
        "weapon_passive_overlays.csv" => Some(include_str!(
            "../../../data/profiles/convergence/weapon_passive_overlays.csv"
        )),
        "weapon_passives.csv" => Some(include_str!(
            "../../../data/profiles/convergence/weapon_passives.csv"
        )),
        "weapons.csv" => Some(include_str!(
            "../../../data/profiles/convergence/weapons.csv"
        )),
        _ => None,
    }
}

fn parse_u8(value: &str, field: &str) -> Result<u8, String> {
    value
        .parse::<u8>()
        .map_err(|err| format!("invalid u8 for {field}: {value} ({err})"))
}

fn parse_u16(value: &str, field: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|err| format!("invalid u16 for {field}: {value} ({err})"))
}

fn parse_u32(value: &str, field: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|err| format!("invalid u32 for {field}: {value} ({err})"))
}

fn parse_usize(value: &str, field: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|err| format!("invalid usize for {field}: {value} ({err})"))
}

fn parse_f32(value: &str, field: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .map_err(|err| format!("invalid f32 for {field}: {value} ({err})"))
}

fn parse_bool_u8(value: &str, field: &str) -> Result<bool, String> {
    Ok(parse_u8(value, field)? != 0)
}

fn parse_physical_attack_attribute(value: &str) -> Result<PhysicalAttackAttribute, String> {
    match value.trim() {
        "standard" => Ok(PhysicalAttackAttribute::Standard),
        "strike" => Ok(PhysicalAttackAttribute::Strike),
        "slash" => Ok(PhysicalAttackAttribute::Slash),
        "pierce" => Ok(PhysicalAttackAttribute::Pierce),
        "adaptive_primary" => Ok(PhysicalAttackAttribute::AdaptivePrimary),
        "adaptive_secondary" => Ok(PhysicalAttackAttribute::AdaptiveSecondary),
        other => Err(format!("invalid physical attack attribute: {other}")),
    }
}

fn parse_optional_bool_u8(
    table: &CsvTable,
    row: &[String],
    field: &str,
) -> Result<Option<bool>, String> {
    let Ok(idx) = table.idx(field) else {
        return Ok(None);
    };
    let value = row[idx].trim();
    if value.is_empty() {
        return Ok(None);
    }
    Ok(Some(parse_u8(value, field)? != 0))
}

pub fn load_game_data(data_dir: impl AsRef<Path>) -> Result<GameData, String> {
    load_game_data_with_manifest(data_dir).map(|(data, _)| data)
}

pub fn load_game_data_with_manifest(
    data_dir: impl AsRef<Path>,
) -> Result<(GameData, SnapshotManifest), String> {
    let data_dir = data_dir.as_ref();
    if is_embedded_data_path(data_dir) {
        return Err("embedded snapshots must be loaded explicitly".to_string());
    }
    let manifest = validate_external_snapshot(data_dir)?;
    load_validated_game_data(data_dir, manifest)
}

fn load_validated_game_data(
    data_dir: &Path,
    manifest: SnapshotManifest,
) -> Result<(GameData, SnapshotManifest), String> {
    let weapons = load_weapons(data_dir.join("weapons.csv"))?;
    let reinforce = load_reinforce(data_dir.join("reinforce.csv"))?;
    let calc_correct = load_calc_correct(data_dir.join("calc_correct.csv"))?;
    let attack_element_correct =
        load_attack_element_correct(data_dir.join("attack_element_correct.csv"))?;
    let attack_element_correct_ext =
        load_attack_element_correct_ext_optional(data_dir.join("attack_element_correct_ext.csv"))?;
    let aow_effects = load_aow_effects(data_dir.join("aow_effect_data.csv"))?;
    let aow_buffs = derive_aow_buffs(&aow_effects)?;
    let aows = load_aows(data_dir.join("aow.csv"), &aow_buffs)?;
    let aow_attack_rows = load_aow_attack_rows_optional(data_dir.join("aow_attack_data.csv"))?;
    let native_skill_attack_rows =
        load_native_skill_attack_rows_optional(data_dir.join("native_skill_attack_data.csv"))?;
    let aow_route_assignments =
        load_aow_route_assignments(data_dir.join("aow_route_assignments.csv"))?;
    let weapon_passives = load_weapon_passives_optional(data_dir.join("weapon_passives.csv"))?;
    let weapon_passive_overlays =
        load_weapon_passive_overlays_optional(data_dir.join("weapon_passive_overlays.csv"))?;
    let exact_aow_compat = load_exact_aow_compat_optional(data_dir.join("aow_weapon_compat.csv"))?;

    let data = GameData {
        snapshot_schema_version: manifest.schema_version,
        dataset_version: manifest.dataset_version.clone(),
        model_version: manifest.model_version.clone(),
        profile_id: manifest.profile.id.clone(),
        profile_display_name: manifest.profile.display_name.clone(),
        capabilities: DataCapabilities {
            weapon_ar: manifest.capabilities.weapon_ar,
            status_buildup: manifest.capabilities.status_buildup,
            weapon_passives: manifest.capabilities.weapon_passives,
            aow_compatibility: manifest.capabilities.aow_compatibility,
            aow_damage: manifest.capabilities.aow_damage,
            aow_routes: manifest.capabilities.aow_routes,
        },
        rules: DataRules {
            standard_max_upgrade: manifest.rules.standard_max_upgrade,
            somber_max_upgrade: manifest.rules.somber_max_upgrade,
            separate_upgrade_caps: manifest.rules.separate_upgrade_caps,
            scadutree_scaling: manifest.rules.scadutree_scaling,
            zero_attack_element_uses_weapon_scaling: manifest
                .rules
                .zero_attack_element_uses_weapon_scaling,
            extended_scaling_grades: manifest.rules.extended_scaling_grades,
            status_buildup_scales: manifest.rules.status_buildup_scales,
        },
        weapons,
        reinforce,
        calc_correct,
        attack_element_correct,
        attack_element_correct_ext,
        aows,
        aow_attack_rows,
        native_skill_attack_rows,
        aow_route_assignments,
        aow_effects,
        weapon_passives,
        weapon_passive_overlays,
        exact_aow_compat,
    };
    Ok((data, manifest))
}

pub fn load_embedded_game_data() -> Result<GameData, String> {
    load_embedded_game_profile(VANILLA_PROFILE_ID)
}

pub fn load_embedded_game_data_with_manifest() -> Result<(GameData, SnapshotManifest), String> {
    load_embedded_game_profile_with_manifest(VANILLA_PROFILE_ID)
}

pub fn load_embedded_game_profile(profile_id: &str) -> Result<GameData, String> {
    load_embedded_game_profile_with_manifest(profile_id).map(|(data, _)| data)
}

pub fn load_embedded_game_profile_with_manifest(
    profile_id: &str,
) -> Result<(GameData, SnapshotManifest), String> {
    let (root, manifest_bytes): (&str, &'static [u8]) = match profile_id {
        VANILLA_PROFILE_ID => (
            EMBEDDED_VANILLA_ROOT,
            include_bytes!("../../../data/phase1/manifest.json"),
        ),
        CONVERGENCE_PROFILE_ID => (
            EMBEDDED_CONVERGENCE_ROOT,
            include_bytes!("../../../data/profiles/convergence/manifest.json"),
        ),
        other => return Err(format!("unknown embedded game profile: {other}")),
    };
    let root_path = Path::new(root);
    let manifest = validate_embedded_snapshot(manifest_bytes, |name| {
        embedded_csv_for_path(&root_path.join(name)).map(str::as_bytes)
    })?;
    if manifest.profile.id != profile_id {
        return Err(format!(
            "embedded profile id mismatch: requested {profile_id}, manifest contains {}",
            manifest.profile.id
        ));
    }
    load_validated_game_data(root_path, manifest)
}

fn load_weapons(path: PathBuf) -> Result<Vec<Weapon>, String> {
    let table = CsvTable::from_path(&path)?;
    let mut out = Vec::with_capacity(table.rows.len());

    for row in &table.rows {
        let base = [
            parse_f32(table.get(row, "base_physical")?, "base_physical")?,
            parse_f32(table.get(row, "base_magic")?, "base_magic")?,
            parse_f32(table.get(row, "base_fire")?, "base_fire")?,
            parse_f32(table.get(row, "base_lightning")?, "base_lightning")?,
            parse_f32(table.get(row, "base_holy")?, "base_holy")?,
        ];
        let scaling = [
            parse_f32(table.get(row, "str_scaling")?, "str_scaling")?,
            parse_f32(table.get(row, "dex_scaling")?, "dex_scaling")?,
            parse_f32(table.get(row, "int_scaling")?, "int_scaling")?,
            parse_f32(table.get(row, "fai_scaling")?, "fai_scaling")?,
            parse_f32(table.get(row, "arc_scaling")?, "arc_scaling")?,
        ];
        let requirements = [
            parse_u8(table.get(row, "req_str")?, "req_str")?,
            parse_u8(table.get(row, "req_dex")?, "req_dex")?,
            parse_u8(table.get(row, "req_int")?, "req_int")?,
            parse_u8(table.get(row, "req_fai")?, "req_fai")?,
            parse_u8(table.get(row, "req_arc")?, "req_arc")?,
        ];
        let damage_curve_ids = [
            parse_usize(table.get(row, "curve_id_physical")?, "curve_id_physical")?,
            parse_usize(table.get(row, "curve_id_magic")?, "curve_id_magic")?,
            parse_usize(table.get(row, "curve_id_fire")?, "curve_id_fire")?,
            parse_usize(table.get(row, "curve_id_lightning")?, "curve_id_lightning")?,
            parse_usize(table.get(row, "curve_id_holy")?, "curve_id_holy")?,
        ];
        out.push(Weapon {
            weapon_id: parse_u32(table.get(row, "weapon_id")?, "weapon_id")?,
            name: table.get(row, "name")?.to_string(),
            affinity: table.get(row, "affinity")?.to_string(),
            native_skill_id: match table.idx("native_skill_id") {
                Ok(_) => {
                    let value = table.get(row, "native_skill_id")?.trim();
                    if value.is_empty() {
                        None
                    } else {
                        Some(parse_u16(value, "native_skill_id")?)
                    }
                }
                Err(_) => None,
            },
            native_skill_name: match table.idx("native_skill_name") {
                Ok(_) => {
                    let value = table.get(row, "native_skill_name")?.trim();
                    if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    }
                }
                Err(_) => None,
            },
            weapon_type_id: parse_u16(table.get(row, "weapon_type_id")?, "weapon_type_id")?,
            weapon_type_name: table.get(row, "weapon_type_name")?.to_string(),
            weapon_type_keys: table.get(row, "weapon_type_keys")?.to_string(),
            stamina_consumption_rate: parse_f32(
                table.get(row, "stamina_consumption_rate")?,
                "stamina_consumption_rate",
            )?,
            physical_attributes: [
                parse_physical_attack_attribute(table.get(row, "physical_attribute_primary")?)?,
                parse_physical_attack_attribute(table.get(row, "physical_attribute_secondary")?)?,
            ],
            base,
            scaling,
            requirements,
            reinforce_type: parse_u16(table.get(row, "reinforce_type")?, "reinforce_type")?,
            attack_element_correct_id: parse_usize(
                table.get(row, "attack_element_correct_id")?,
                "attack_element_correct_id",
            )?,
            damage_curve_ids,
            bleed_curve_id: match table.idx("curve_id_blood") {
                Ok(_) => parse_usize(table.get(row, "curve_id_blood")?, "curve_id_blood")?,
                Err(_) => 6,
            },
            disable_gem_attr: match table.idx("disable_gem_attr") {
                Ok(_) => parse_bool_u8(table.get(row, "disable_gem_attr")?, "disable_gem_attr")?,
                Err(_) => false,
            },
            is_somber: parse_bool_u8(table.get(row, "is_somber")?, "is_somber")?,
            disable_two_hand_bonus: match table.idx("disable_two_hand_bonus") {
                Ok(_) => parse_bool_u8(
                    table.get(row, "disable_two_hand_bonus")?,
                    "disable_two_hand_bonus",
                )?,
                Err(_) => false,
            },
        });
    }
    Ok(out)
}

fn load_reinforce(path: PathBuf) -> Result<Vec<Vec<Option<ReinforceLevel>>>, String> {
    let table = CsvTable::from_path(&path)?;
    let mut entries = Vec::with_capacity(table.rows.len());
    let mut max_type = 0usize;
    let mut max_level_by_type: HashMap<usize, usize> = HashMap::new();

    for row in &table.rows {
        let reinforce_type = parse_usize(table.get(row, "reinforce_type")?, "reinforce_type")?;
        let level = parse_usize(table.get(row, "level")?, "level")?;
        let damage_mult = [
            parse_f32(
                table.get(row, "physical_damage_mult")?,
                "physical_damage_mult",
            )?,
            parse_f32(table.get(row, "magic_damage_mult")?, "magic_damage_mult")?,
            parse_f32(table.get(row, "fire_damage_mult")?, "fire_damage_mult")?,
            parse_f32(
                table.get(row, "lightning_damage_mult")?,
                "lightning_damage_mult",
            )?,
            parse_f32(table.get(row, "holy_damage_mult")?, "holy_damage_mult")?,
        ];
        let scaling_mult = [
            parse_f32(table.get(row, "str_scaling_mult")?, "str_scaling_mult")?,
            parse_f32(table.get(row, "dex_scaling_mult")?, "dex_scaling_mult")?,
            parse_f32(table.get(row, "int_scaling_mult")?, "int_scaling_mult")?,
            parse_f32(table.get(row, "fai_scaling_mult")?, "fai_scaling_mult")?,
            parse_f32(table.get(row, "arc_scaling_mult")?, "arc_scaling_mult")?,
        ];
        max_type = max_type.max(reinforce_type);
        max_level_by_type
            .entry(reinforce_type)
            .and_modify(|value| *value = (*value).max(level))
            .or_insert(level);
        entries.push((
            reinforce_type,
            level,
            ReinforceLevel {
                damage_mult,
                scaling_mult,
                base_attack_mult: parse_f32(
                    table.get(row, "base_attack_mult")?,
                    "base_attack_mult",
                )?,
            },
        ));
    }

    let mut reinforce = vec![Vec::<Option<ReinforceLevel>>::new(); max_type + 1];
    for (reinforce_type, max_level) in &max_level_by_type {
        reinforce[*reinforce_type] = vec![None; *max_level + 1];
    }
    for (reinforce_type, level, value) in entries {
        if let Some(levels) = reinforce.get_mut(reinforce_type)
            && level < levels.len()
        {
            levels[level] = Some(value);
        }
    }
    Ok(reinforce)
}

fn load_calc_correct(path: PathBuf) -> Result<Vec<Vec<f32>>, String> {
    let table = CsvTable::from_path(&path)?;
    let mut entries = Vec::with_capacity(table.rows.len());
    let mut max_curve_id = 0usize;

    for row in &table.rows {
        let curve_id = parse_usize(table.get(row, "curve_id")?, "curve_id")?;
        let stat_value = parse_usize(table.get(row, "stat_value")?, "stat_value")?;
        let multiplier = parse_f32(table.get(row, "multiplier")?, "multiplier")?;
        max_curve_id = max_curve_id.max(curve_id);
        entries.push((curve_id, stat_value, multiplier));
    }

    let max_stat_value = entries
        .iter()
        .map(|(_, stat_value, _)| *stat_value)
        .max()
        .unwrap_or(0);
    let mut out = vec![vec![0.0_f32; max_stat_value + 1]; max_curve_id + 1];
    for (curve_id, stat_value, multiplier) in entries {
        out[curve_id][stat_value] = multiplier;
    }
    Ok(out)
}

fn load_attack_element_correct(path: PathBuf) -> Result<Vec<Option<AttackElementCorrect>>, String> {
    let table = CsvTable::from_path(&path)?;
    let mut entries = Vec::with_capacity(table.rows.len());
    let mut max_id = 0usize;

    let fields = [
        [
            "str_scales_physical",
            "str_scales_magic",
            "str_scales_fire",
            "str_scales_lightning",
            "str_scales_holy",
        ],
        [
            "dex_scales_physical",
            "dex_scales_magic",
            "dex_scales_fire",
            "dex_scales_lightning",
            "dex_scales_holy",
        ],
        [
            "int_scales_physical",
            "int_scales_magic",
            "int_scales_fire",
            "int_scales_lightning",
            "int_scales_holy",
        ],
        [
            "fai_scales_physical",
            "fai_scales_magic",
            "fai_scales_fire",
            "fai_scales_lightning",
            "fai_scales_holy",
        ],
        [
            "arc_scales_physical",
            "arc_scales_magic",
            "arc_scales_fire",
            "arc_scales_lightning",
            "arc_scales_holy",
        ],
    ];

    for row in &table.rows {
        let row_id = parse_usize(
            table.get(row, "attack_element_correct_id")?,
            "attack_element_correct_id",
        )?;
        let mut scales = [[false; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        for stat_idx in 0..COMBAT_STAT_COUNT {
            for damage_idx in 0..DAMAGE_TYPE_COUNT {
                let value = parse_u8(table.get(row, fields[stat_idx][damage_idx])?, "aec_scale")?;
                scales[stat_idx][damage_idx] = value != 0;
            }
        }
        max_id = max_id.max(row_id);
        entries.push((row_id, AttackElementCorrect { scales }));
    }

    let mut out = vec![None; max_id + 1];
    for (row_id, value) in entries {
        out[row_id] = Some(value);
    }
    Ok(out)
}

fn load_aows(path: PathBuf, buff_rows: &HashMap<u16, AowBuffRow>) -> Result<Vec<Aow>, String> {
    let table = CsvTable::from_path(&path)?;
    let mut out = Vec::with_capacity(table.rows.len());

    for row in &table.rows {
        let aow_id = parse_u16(table.get(row, "aow_id")?, "aow_id")?;
        let buff_row = buff_rows.get(&aow_id).cloned().unwrap_or_default();
        out.push(Aow {
            aow_id,
            name: table.get(row, "name")?.to_string(),
            bleed_buildup_add: parse_f32(
                table.get(row, "bleed_buildup_add")?,
                "bleed_buildup_add",
            )?,
            frost_buildup_add: parse_f32(
                table.get(row, "frost_buildup_add")?,
                "frost_buildup_add",
            )?,
            poison_buildup_add: parse_f32(
                table.get(row, "poison_buildup_add")?,
                "poison_buildup_add",
            )?,
            scarlet_rot_buildup_add: match table.idx("scarlet_rot_buildup_add") {
                Ok(_) => parse_f32(
                    table.get(row, "scarlet_rot_buildup_add")?,
                    "scarlet_rot_buildup_add",
                )?,
                Err(_) => 0.0,
            },
            valid_weapon_types: table.get(row, "valid_weapon_types")?.to_string(),
            buff_attack_power: buff_row.buff_attack_power,
            scaling_status_add: buff_row.scaling_status_add,
            scaling_status_flags: buff_row.scaling_status_flags,
            persistent_weapon_status_add: buff_row.persistent_weapon_status_add,
            persistent_on_hit_status_add: buff_row.persistent_on_hit_status_add,
            buff_activation_action_id: buff_row.activation_action_id,
        });
    }
    Ok(out)
}

fn parse_aow_effect_role(value: &str) -> Result<AowEffectRole, String> {
    match value {
        "persistent_setup" => Ok(AowEffectRole::PersistentSetup),
        "persistent_weapon_buff" => Ok(AowEffectRole::PersistentWeaponBuff),
        "persistent_on_hit" => Ok(AowEffectRole::PersistentOnHit),
        "per_hit_status" => Ok(AowEffectRole::PerHitStatus),
        "per_hit_attack_power" => Ok(AowEffectRole::PerHitAttackPower),
        "self_buff" => Ok(AowEffectRole::SelfBuff),
        "self_mechanic" => Ok(AowEffectRole::SelfMechanic),
        "replacement_or_chained" => Ok(AowEffectRole::ReplacementOrChained),
        "visual_or_non_gameplay" => Ok(AowEffectRole::VisualOrNonGameplay),
        other => Err(format!("invalid AoW effect role: {other}")),
    }
}

fn parse_pipe_u32(value: &str, field: &str) -> Result<Vec<u32>, String> {
    value
        .split('|')
        .filter(|part| !part.trim().is_empty())
        .map(|part| parse_u32(part.trim(), field))
        .collect()
}

fn load_aow_effects(path: PathBuf) -> Result<HashMap<(u16, u16), Vec<AowEffect>>, String> {
    let table = CsvTable::from_path(&path)?;
    let mut out: HashMap<(u16, u16), Vec<AowEffect>> = HashMap::new();
    let mut record_ids = HashSet::with_capacity(table.rows.len());
    for row in &table.rows {
        let record_id = parse_u32(table.get(row, "record_id")?, "record_id")?;
        if !record_ids.insert(record_id) {
            return Err(format!("duplicate AoW effect record_id: {record_id}"));
        }
        let aow_id = parse_u16(table.get(row, "aow_id")?, "aow_id")?;
        let sheet_row = parse_u16(table.get(row, "sheet_row")?, "sheet_row")?;
        let parent_effect_id = parse_u32(table.get(row, "parent_effect_id")?, "parent_effect_id")?;
        out.entry((aow_id, sheet_row)).or_default().push(AowEffect {
            record_id,
            aow_id,
            sheet_row,
            source_kind: table.get(row, "source_kind")?.to_string(),
            source_param_ids: parse_pipe_u32(
                table.get(row, "source_param_ids")?,
                "source_param_ids",
            )?,
            effect_id: parse_u32(table.get(row, "effect_id")?, "effect_id")?,
            effect_name: table.get(row, "effect_name")?.to_string(),
            parent_effect_id: (parent_effect_id != 0).then_some(parent_effect_id),
            link_kind: table.get(row, "link_kind")?.to_string(),
            role: parse_aow_effect_role(table.get(row, "role")?)?,
            activation_action_id: table.get(row, "activation_action_id")?.to_string(),
            activation_timing: table.get(row, "activation_timing")?.to_string(),
            hand_variant: table.get(row, "hand_variant")?.to_string(),
            is_canonical: parse_optional_bool_u8(&table, row, "is_canonical")?,
            is_supported: parse_bool_u8(table.get(row, "is_supported")?, "is_supported")?,
            reason: table.get(row, "reason")?.to_string(),
            duration_seconds: parse_f32(table.get(row, "duration_seconds")?, "duration_seconds")?,
            attack_power: [
                parse_f32(
                    table.get(row, "physical_attack_power")?,
                    "physical_attack_power",
                )?,
                parse_f32(table.get(row, "magic_attack_power")?, "magic_attack_power")?,
                parse_f32(table.get(row, "fire_attack_power")?, "fire_attack_power")?,
                parse_f32(
                    table.get(row, "lightning_attack_power")?,
                    "lightning_attack_power",
                )?,
                parse_f32(table.get(row, "holy_attack_power")?, "holy_attack_power")?,
            ],
            status_buildup: StatusBuildup {
                bleed: parse_f32(table.get(row, "bleed_buildup")?, "bleed_buildup")?,
                frost: parse_f32(table.get(row, "frost_buildup")?, "frost_buildup")?,
                poison: parse_f32(table.get(row, "poison_buildup")?, "poison_buildup")?,
                scarlet_rot: parse_f32(
                    table.get(row, "scarlet_rot_buildup")?,
                    "scarlet_rot_buildup",
                )?,
                sleep: parse_f32(table.get(row, "sleep_buildup")?, "sleep_buildup")?,
                madness: parse_f32(table.get(row, "madness_buildup")?, "madness_buildup")?,
                death: parse_f32(table.get(row, "death_buildup")?, "death_buildup")?,
            },
            uses_status_correction: parse_bool_u8(
                table.get(row, "uses_status_correction")?,
                "uses_status_correction",
            )?,
            uses_attack_correction: parse_bool_u8(
                table.get(row, "uses_attack_correction")?,
                "uses_attack_correction",
            )?,
        });
    }
    for effects in out.values_mut() {
        effects.sort_by_key(|effect| effect.record_id);
    }
    Ok(out)
}

fn derive_aow_buffs(
    effects_by_hit: &HashMap<(u16, u16), Vec<AowEffect>>,
) -> Result<HashMap<u16, AowBuffRow>, String> {
    let mut out = HashMap::<u16, AowBuffRow>::new();
    for ((aow_id, sheet_row), effects) in effects_by_hit {
        if *sheet_row != 0 {
            continue;
        }
        for effect in effects
            .iter()
            .filter(|effect| effect.is_supported && effect.is_canonical == Some(true))
        {
            let row = out.entry(*aow_id).or_default();
            if !effect.activation_action_id.is_empty() {
                match &row.activation_action_id {
                    Some(existing) if existing != &effect.activation_action_id => {
                        return Err(format!(
                            "conflicting activation actions for AoW {aow_id}: {existing} vs {}",
                            effect.activation_action_id
                        ));
                    }
                    None => row.activation_action_id = Some(effect.activation_action_id.clone()),
                    _ => {}
                }
            }
            match effect.role {
                AowEffectRole::PersistentWeaponBuff => {
                    for (total, value) in row.buff_attack_power.iter_mut().zip(effect.attack_power)
                    {
                        *total += value;
                    }
                    row.persistent_weapon_status_add = row
                        .persistent_weapon_status_add
                        .combined_with(effect.status_buildup);
                }
                AowEffectRole::PersistentOnHit => {
                    row.persistent_on_hit_status_add = row
                        .persistent_on_hit_status_add
                        .combined_with(effect.status_buildup);
                }
                AowEffectRole::PersistentSetup => continue,
                _ => {
                    return Err(format!(
                        "unexpected canonical persistent role for AoW {aow_id}: {:?}",
                        effect.role
                    ));
                }
            }
            row.scaling_status_add = row.scaling_status_add.combined_with(effect.status_buildup);
            merge_status_correction_flags(
                &mut row.scaling_status_flags,
                effect.status_buildup,
                effect.uses_status_correction,
            );
        }
    }
    Ok(out)
}

fn merge_status_correction_flags(
    flags: &mut StatusCorrectionFlags,
    status: StatusBuildup,
    uses_correction: bool,
) {
    let merge = |slot: &mut Option<bool>, value: f32| {
        if value > 0.0 {
            *slot = Some(slot.unwrap_or(false) || uses_correction);
        }
    };
    merge(&mut flags.bleed, status.bleed);
    merge(&mut flags.frost, status.frost);
    merge(&mut flags.poison, status.poison);
    merge(&mut flags.scarlet_rot, status.scarlet_rot);
    merge(&mut flags.sleep, status.sleep);
    merge(&mut flags.madness, status.madness);
    merge(&mut flags.death, status.death);
}

fn load_attack_element_correct_ext_optional(
    path: PathBuf,
) -> Result<HashMap<usize, AttackElementCorrectExt>, String> {
    let Some(table) = CsvTable::from_optional_path(&path)? else {
        return Ok(HashMap::new());
    };
    let mut out = HashMap::with_capacity(table.rows.len());
    for row in &table.rows {
        let row_id = parse_usize(
            table.get(row, "attack_element_correct_id")?,
            "attack_element_correct_id",
        )?;
        let mut scales = [[false; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        let mut overwrite = [[None; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        let mut influence = [[100.0_f32; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        for (stat_idx, stat_key) in ["str", "dex", "int", "fai", "arc"].iter().enumerate() {
            for (damage_idx, damage_key) in ["physical", "magic", "fire", "lightning", "holy"]
                .iter()
                .enumerate()
            {
                let scale_field = format!("{stat_key}_scales_{damage_key}");
                let overwrite_field = format!("{stat_key}_overwrite_{damage_key}");
                let influence_field = format!("{stat_key}_influence_{damage_key}");
                scales[stat_idx][damage_idx] =
                    parse_bool_u8(table.get(row, &scale_field)?, &scale_field)?;
                let overwrite_value =
                    parse_f32(table.get(row, &overwrite_field)?, &overwrite_field)?;
                if overwrite_value >= 0.0 {
                    overwrite[stat_idx][damage_idx] = Some(overwrite_value / 100.0);
                }
                influence[stat_idx][damage_idx] =
                    parse_f32(table.get(row, &influence_field)?, &influence_field)? / 100.0;
            }
        }
        out.insert(
            row_id,
            AttackElementCorrectExt {
                scales,
                overwrite,
                influence,
            },
        );
    }
    Ok(out)
}

fn load_aow_attack_rows_optional(path: PathBuf) -> Result<HashMap<u16, Vec<AowAttackRow>>, String> {
    let Some(table) = CsvTable::from_optional_path(&path)? else {
        return Ok(HashMap::new());
    };
    let mut out: HashMap<u16, Vec<AowAttackRow>> = HashMap::new();
    for row in &table.rows {
        let aow_id = parse_u16(table.get(row, "aow_id")?, "aow_id")?;
        out.entry(aow_id)
            .or_default()
            .push(parse_aow_attack_row(&table, row, aow_id)?);
    }

    for rows in out.values_mut() {
        rows.sort_by_key(|row| row.sheet_row);
    }
    Ok(out)
}

fn load_native_skill_attack_rows_optional(
    path: PathBuf,
) -> Result<HashMap<u32, Vec<AowAttackRow>>, String> {
    let Some(table) = CsvTable::from_optional_path(&path)? else {
        return Ok(HashMap::new());
    };
    let mut out: HashMap<u32, Vec<AowAttackRow>> = HashMap::new();
    for row in &table.rows {
        let weapon_id = parse_u32(table.get(row, "weapon_id")?, "weapon_id")?;
        let aow_id = parse_u16(table.get(row, "aow_id")?, "aow_id")?;
        out.entry(weapon_id)
            .or_default()
            .push(parse_aow_attack_row(&table, row, aow_id)?);
    }

    for rows in out.values_mut() {
        rows.sort_by_key(|row| row.sheet_row);
    }
    Ok(out)
}

fn parse_aow_attack_row(
    table: &CsvTable,
    row: &[String],
    aow_id: u16,
) -> Result<AowAttackRow, String> {
    let overwrite_raw = table
        .get(row, "overwrite_attack_element_correct_id")?
        .parse::<i32>()
        .map_err(|err| {
            format!(
                "invalid i32 for overwrite_attack_element_correct_id: {} ({err})",
                table
                    .get(row, "overwrite_attack_element_correct_id")
                    .unwrap_or_default()
            )
        })?;
    Ok(AowAttackRow {
        sheet_row: parse_u16(table.get(row, "sheet_row")?, "sheet_row")?,
        aow_id,
        aow_name: table.get(row, "aow_name")?.to_string(),
        raw_name: table.get(row, "raw_name")?.to_string(),
        variant_weapon_type: table.get(row, "variant_weapon_type")?.to_string(),
        sequence_variant: table.get(row, "sequence_variant")?.to_string(),
        hit_kind: table.get(row, "hit_kind")?.to_string(),
        hit_order: parse_u16(table.get(row, "hit_order")?, "hit_order")?,
        is_lacking_fp: parse_bool_u8(table.get(row, "is_lacking_fp")?, "is_lacking_fp")?,
        atk_id: parse_u32(table.get(row, "atk_id")?, "atk_id")?,
        overwrite_attack_element_correct_id: (overwrite_raw > 0).then_some(overwrite_raw as usize),
        is_disable_both_hands_bonus: parse_bool_u8(
            table.get(row, "is_disable_both_hands_bonus")?,
            "is_disable_both_hands_bonus",
        )?,
        is_add_base_atk: parse_bool_u8(table.get(row, "is_add_base_atk")?, "is_add_base_atk")?,
        is_arrow_attack: parse_bool_u8(table.get(row, "is_arrow_attack")?, "is_arrow_attack")?,
        physical_attack_attribute: parse_physical_attack_attribute(
            table.get(row, "physical_attack_attribute")?,
        )?,
        motion_values: [
            parse_f32(table.get(row, "physical_mv")?, "physical_mv")?,
            parse_f32(table.get(row, "magic_mv")?, "magic_mv")?,
            parse_f32(table.get(row, "fire_mv")?, "fire_mv")?,
            parse_f32(table.get(row, "lightning_mv")?, "lightning_mv")?,
            parse_f32(table.get(row, "holy_mv")?, "holy_mv")?,
        ],
        attack_base: [
            parse_f32(
                table.get(row, "attack_base_physical")?,
                "attack_base_physical",
            )?,
            parse_f32(table.get(row, "attack_base_magic")?, "attack_base_magic")?,
            parse_f32(table.get(row, "attack_base_fire")?, "attack_base_fire")?,
            parse_f32(
                table.get(row, "attack_base_lightning")?,
                "attack_base_lightning",
            )?,
            parse_f32(table.get(row, "attack_base_holy")?, "attack_base_holy")?,
        ],
        status_mv: parse_f32(table.get(row, "status_mv")?, "status_mv")?,
        weapon_buff_mv: parse_f32(table.get(row, "weapon_buff_mv")?, "weapon_buff_mv")?,
        stamina_cost: parse_f32(table.get(row, "stamina_cost")?, "stamina_cost")?,
        stamina_cost_mode: match table.get(row, "stamina_cost_mode")? {
            "weapon_scaled" => StaminaCostMode::WeaponScaled,
            "precalculated" => StaminaCostMode::Precalculated,
            other => return Err(format!("invalid stamina_cost_mode: {other}")),
        },
    })
}

fn load_aow_route_assignments(
    path: PathBuf,
) -> Result<HashMap<(u16, u16), Vec<AowRouteAssignment>>, String> {
    let table = CsvTable::from_path(&path)?;
    let mut out: HashMap<(u16, u16), Vec<AowRouteAssignment>> = HashMap::new();
    for row in &table.rows {
        let aow_id = parse_u16(table.get(row, "aow_id")?, "aow_id")?;
        let sheet_row = parse_u16(table.get(row, "sheet_row")?, "sheet_row")?;
        out.entry((aow_id, sheet_row))
            .or_default()
            .push(AowRouteAssignment {
                route_id: table.get(row, "route_id")?.to_string(),
                route_label: table.get(row, "route_label")?.to_string(),
                route_priority: parse_u16(table.get(row, "route_priority")?, "route_priority")?,
                action_id: table.get(row, "action_id")?.to_string(),
                action_order: parse_u16(table.get(row, "action_order")?, "action_order")?,
                hit_order: parse_u16(table.get(row, "hit_order")?, "hit_order")?,
            });
    }
    for assignments in out.values_mut() {
        assignments.sort_by(|left, right| {
            left.route_priority
                .cmp(&right.route_priority)
                .then_with(|| left.route_id.cmp(&right.route_id))
                .then_with(|| left.action_order.cmp(&right.action_order))
                .then_with(|| left.hit_order.cmp(&right.hit_order))
        });
    }
    Ok(out)
}

fn load_weapon_passives_optional(
    path: PathBuf,
) -> Result<HashMap<u32, StatusEffectSource>, String> {
    let Some(table) = CsvTable::from_optional_path(&path)? else {
        return Ok(HashMap::new());
    };
    let mut out = HashMap::with_capacity(table.rows.len());
    for row in &table.rows {
        let weapon_id = parse_u32(table.get(row, "weapon_id")?, "weapon_id")?;
        out.insert(weapon_id, parse_status_effect_source(&table, row)?);
    }
    Ok(out)
}

fn parse_status_effect_source(
    table: &CsvTable,
    row: &[String],
) -> Result<StatusEffectSource, String> {
    Ok(StatusEffectSource {
        buildup: StatusBuildup {
            bleed: parse_f32(table.get(row, "bleed")?, "bleed")?,
            frost: parse_f32(table.get(row, "frost")?, "frost")?,
            poison: parse_f32(table.get(row, "poison")?, "poison")?,
            scarlet_rot: match table.idx("scarlet_rot") {
                Ok(_) => parse_f32(table.get(row, "scarlet_rot")?, "scarlet_rot")?,
                Err(_) => 0.0,
            },
            sleep: parse_f32(table.get(row, "sleep")?, "sleep")?,
            madness: parse_f32(table.get(row, "madness")?, "madness")?,
            death: parse_f32(table.get(row, "death")?, "death")?,
        },
        correction_flags: StatusCorrectionFlags {
            bleed: parse_optional_bool_u8(table, row, "bleed_uses_status_correction")?,
            frost: parse_optional_bool_u8(table, row, "frost_uses_status_correction")?,
            poison: parse_optional_bool_u8(table, row, "poison_uses_status_correction")?,
            scarlet_rot: parse_optional_bool_u8(table, row, "scarlet_rot_uses_status_correction")?,
            sleep: parse_optional_bool_u8(table, row, "sleep_uses_status_correction")?,
            madness: parse_optional_bool_u8(table, row, "madness_uses_status_correction")?,
            death: parse_optional_bool_u8(table, row, "death_uses_status_correction")?,
        },
    })
}

fn load_weapon_passive_overlays_optional(
    path: PathBuf,
) -> Result<HashMap<u32, Vec<Option<StatusEffectSource>>>, String> {
    let Some(table) = CsvTable::from_optional_path(&path)? else {
        return Ok(HashMap::new());
    };
    let mut max_level_by_weapon = HashMap::<u32, usize>::new();
    let mut entries = Vec::<(u32, usize, StatusEffectSource)>::with_capacity(table.rows.len());
    for row in &table.rows {
        let weapon_id = parse_u32(table.get(row, "weapon_id")?, "weapon_id")?;
        let level = parse_usize(table.get(row, "level")?, "level")?;
        let source = parse_status_effect_source(&table, row)?;
        max_level_by_weapon
            .entry(weapon_id)
            .and_modify(|value| *value = (*value).max(level))
            .or_insert(level);
        entries.push((weapon_id, level, source));
    }

    let mut out =
        HashMap::<u32, Vec<Option<StatusEffectSource>>>::with_capacity(max_level_by_weapon.len());
    for (weapon_id, max_level) in max_level_by_weapon {
        out.insert(weapon_id, vec![None; max_level + 1]);
    }
    for (weapon_id, level, source) in entries {
        if let Some(levels) = out.get_mut(&weapon_id) {
            levels[level] = Some(source);
        }
    }
    Ok(out)
}

fn load_exact_aow_compat_optional(path: PathBuf) -> Result<HashSet<(u16, u32)>, String> {
    let Some(table) = CsvTable::from_optional_path(&path)? else {
        return Ok(HashSet::new());
    };
    let mut out = HashSet::with_capacity(table.rows.len());
    for row in &table.rows {
        let aow_id = parse_u16(table.get(row, "aow_id")?, "aow_id")?;
        let weapon_id = parse_u32(table.get(row, "weapon_id")?, "weapon_id")?;
        out.insert((aow_id, weapon_id));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{
        CONVERGENCE_PROFILE_ID, load_embedded_game_data, load_embedded_game_profile, load_game_data,
    };
    use crate::model::AowEffectRole;

    #[test]
    fn explicit_embedded_snapshot_loads_all_runtime_tables() {
        let data = load_embedded_game_data().expect("embedded snapshot loads");
        assert!(data.weapons.len() > 3000);
        assert!(data.aows.len() > 100);
        assert!(!data.exact_aow_compat.is_empty());
        assert!(!data.aow_effects.is_empty());
    }

    #[test]
    fn convergence_snapshot_is_isolated_and_declares_partial_aow_coverage() {
        let data = load_embedded_game_profile(CONVERGENCE_PROFILE_ID)
            .expect("Convergence embedded snapshot loads");
        assert_eq!(data.profile_id, CONVERGENCE_PROFILE_ID);
        assert_eq!(data.dataset_version, "convergence-3.0.0.1");
        assert_eq!(data.rules.standard_max_upgrade, 15);
        assert_eq!(data.rules.somber_max_upgrade, 15);
        assert!(!data.rules.separate_upgrade_caps);
        assert!(!data.rules.scadutree_scaling);
        assert!(data.rules.zero_attack_element_uses_weapon_scaling);
        assert!(data.rules.extended_scaling_grades);
        assert!(!data.rules.status_buildup_scales);
        assert_eq!(data.weapons.len(), 3189);
        assert!(data.weapons.iter().any(|weapon| weapon.affinity == "Glint"));
        assert!(data.weapons.iter().any(|weapon| {
            weapon.weapon_id == 10_200_000 && weapon.name == "Galvanic Culling Blade [Twinblade]"
        }));
        let galvanic = data
            .weapons
            .iter()
            .find(|weapon| weapon.weapon_id == 10_200_000)
            .expect("Galvanic twinblade");
        let plus_thirteen = data
            .reinforce_level(galvanic.reinforce_type, 13)
            .expect("Galvanic +13 reinforcement");
        assert!((galvanic.scaling[0] * plus_thirteen.scaling_mult[0] - 1.089).abs() < 0.001);
        assert!((galvanic.scaling[1] * plus_thirteen.scaling_mult[1] - 1.386).abs() < 0.001);
        assert!((galvanic.scaling[2] * plus_thirteen.scaling_mult[2] - 2.277).abs() < 0.001);
        assert!(
            !data
                .weapons
                .iter()
                .any(|weapon| weapon.weapon_id == 10_205_000)
        );
        assert!(
            data.aows
                .iter()
                .any(|aow| aow.aow_id == 105 && aow.name == "Ancient Thunderclap")
        );
        assert!(data.weapons.iter().any(|weapon| {
            weapon.native_skill_id == Some(105)
                && weapon.native_skill_name.as_deref() == Some("Ancient Thunderclap")
        }));
        assert!(!data.exact_aow_compat.is_empty());
        assert!(!data.capabilities.aow_damage);
        assert!(!data.capabilities.aow_routes);
        let fallback = data
            .attack_element(0)
            .expect("Convergence correction fallback");
        assert!(fallback.scales.iter().flatten().all(|enabled| *enabled));
        assert!(data.aow_attack_rows.is_empty());
        assert!(data.aow_route_assignments.is_empty());
    }

    #[test]
    fn missing_runtime_snapshot_fails_closed() {
        let error = match load_game_data("__missing_phase1_data_dir__") {
            Ok(_) => panic!("missing external snapshot must not fall back to embedded data"),
            Err(error) => error,
        };
        assert!(error.contains("failed reading"));
        assert!(error.contains("manifest.json"));
    }

    #[test]
    fn persistent_status_effect_roles_remain_separate() {
        let data = load_embedded_game_data().expect("embedded snapshot loads");
        let chilling_mist = data
            .aows
            .iter()
            .find(|aow| aow.aow_id == 227)
            .expect("Chilling Mist");
        assert_eq!(chilling_mist.persistent_weapon_status_add.frost, 30.0);
        assert_eq!(chilling_mist.persistent_on_hit_status_add.frost, 60.0);
        assert_eq!(chilling_mist.scaling_status_add.frost, 90.0);
        assert_eq!(
            chilling_mist.buff_activation_action_id.as_deref(),
            Some("activation")
        );

        let projectile = data
            .aow_effects(227, 1485)
            .iter()
            .find(|effect| effect.effect_id == 881)
            .expect("Chilling Mist projectile status");
        assert_eq!(projectile.role, AowEffectRole::PerHitStatus);
        assert_eq!(projectile.status_buildup.frost, 60.0);
    }

    #[test]
    fn conditional_replacement_effects_are_explicitly_unsupported() {
        let data = load_embedded_game_data().expect("embedded snapshot loads");
        let poison_moth_replacement = data
            .aow_effects(119, 1442)
            .iter()
            .find(|effect| effect.effect_id == 1622)
            .expect("Poison Moth replacement effect");
        assert_eq!(
            poison_moth_replacement.role,
            AowEffectRole::ReplacementOrChained
        );
        assert!(!poison_moth_replacement.is_supported);
        assert_eq!(poison_moth_replacement.status_buildup.poison, 250.0);
    }

    #[test]
    fn branch_specific_status_effects_stay_attached_to_their_hits() {
        let data = load_embedded_game_data().expect("embedded snapshot loads");
        let hoarfrost_spike = data
            .aow_effects(501, 1498)
            .iter()
            .find(|effect| effect.effect_id == 1800)
            .expect("Hoarfrost spike");
        let hoarfrost_shatter = data
            .aow_effects(501, 1499)
            .iter()
            .find(|effect| effect.effect_id == 1801)
            .expect("Hoarfrost shatter");
        assert_eq!(hoarfrost_spike.status_buildup.frost, 70.0);
        assert_eq!(hoarfrost_shatter.status_buildup.frost, 110.0);

        let ghostflame_r1 = data
            .aow_effects(4220, 1491)
            .iter()
            .find(|effect| effect.effect_id == 20_001_091)
            .expect("Ghostflame R1");
        let ghostflame_r2 = data
            .aow_effects(4220, 1494)
            .iter()
            .find(|effect| effect.effect_id == 20_001_092)
            .expect("Ghostflame R2");
        assert_eq!(ghostflame_r1.status_buildup.frost, 20.0);
        assert_eq!(ghostflame_r2.status_buildup.frost, 80.0);
    }
}
