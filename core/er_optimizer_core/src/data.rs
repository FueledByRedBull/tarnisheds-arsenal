use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::Path;

use crate::math::{CUSTOM_STATS_CLASS_NAME, STARTING_CLASSES, StartingClass};
use crate::model::Stats;
use crate::model::{
    Aow, AowAttackRow, AowEffect, AowEffectRole, AowRouteAssignment, AttackElementCorrect,
    AttackElementCorrectExt, COMBAT_STAT_COUNT, DAMAGE_TYPE_COUNT, DataCapabilities, DataRules,
    DisplayPoiseDamage, GameData, PhysicalAttackAttribute, ReinforceLevel, StaminaCostMode,
    StatusBuildup, StatusCorrectionFlags, StatusCurveIds, StatusEffectSource, Weapon,
};
use crate::snapshot::{
    CLASS_CATALOG_FILE, SnapshotManifest, requires_class_catalog, validate_embedded_snapshot,
    validate_external_snapshot,
};

pub const VANILLA_PROFILE_ID: &str = "vanilla";
pub const CONVERGENCE_PROFILE_ID: &str = "convergence";
/// Calc-correct curves are tabulated through stat 999; ids bound the per-curve allocation.
const MAX_CURVE_STAT_VALUE: usize = 999;

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
    headers: HashMap<String, usize>,
    rows: Vec<csv::StringRecord>,
}

impl CsvTable {
    fn from_bytes(source: String, content: &[u8]) -> Result<Self, String> {
        let content = std::str::from_utf8(content)
            .map_err(|error| format!("{source} is not valid UTF-8: {error}"))?;
        Self::from_content(source, content)
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
        let mut indexed_headers = HashMap::with_capacity(headers.len());
        for (index, header) in headers.into_iter().enumerate() {
            if indexed_headers.insert(header.clone(), index).is_some() {
                return Err(format!(
                    "{source} has duplicate csv column header: {header}"
                ));
            }
        }

        let mut rows = Vec::new();
        for record in reader.records() {
            let record = record.map_err(|err| format!("{source} has invalid csv row: {err}"))?;
            rows.push(record);
        }
        Ok(Self {
            headers: indexed_headers,
            rows,
        })
    }

    fn idx(&self, field: &str) -> Result<usize, String> {
        self.headers
            .get(field)
            .copied()
            .ok_or_else(|| format!("missing csv column: {field}"))
    }

    fn columns<const N: usize>(&self, fields: [&str; N]) -> Result<[usize; N], String> {
        let mut columns = [0; N];
        for (slot, field) in columns.iter_mut().zip(fields) {
            *slot = self.idx(field)?;
        }
        Ok(columns)
    }

    fn optional_columns<const N: usize>(&self, fields: [&str; N]) -> [Option<usize>; N] {
        fields.map(|field| self.headers.get(field).copied())
    }
}

/// (profile, table, brotli-compressed CSV) for every embedded runtime table; see build.rs.
const EMBEDDED_TABLES: &[(&str, &str, &[u8])] =
    include!(concat!(env!("OUT_DIR"), "/embedded_tables.rs"));

fn embedded_tables(profile_id: &str) -> Result<HashMap<&'static str, Vec<u8>>, String> {
    EMBEDDED_TABLES
        .iter()
        .filter(|(profile, _, _)| *profile == profile_id)
        .map(|(_, table, compressed)| {
            let mut bytes = Vec::new();
            brotli_decompressor::BrotliDecompress(&mut &compressed[..], &mut bytes)
                .map_err(|error| format!("embedded {profile_id} {table} is corrupt: {error}"))?;
            Ok((*table, bytes))
        })
        .collect()
}

/// Decompressed embedded tables, so tests read exactly the bytes build.rs embeds.
#[cfg(test)]
fn embedded_csv(profile_id: &str, name: &str) -> Option<&'static str> {
    type Tables = HashMap<&'static str, HashMap<&'static str, &'static str>>;
    static TABLES: std::sync::OnceLock<Tables> = std::sync::OnceLock::new();
    TABLES
        .get_or_init(|| {
            [VANILLA_PROFILE_ID, CONVERGENCE_PROFILE_ID]
                .into_iter()
                .map(|profile| {
                    let tables = embedded_tables(profile)
                        .unwrap()
                        .into_iter()
                        .map(|(table, bytes)| (table, &*String::from_utf8(bytes).unwrap().leak()))
                        .collect();
                    (profile, tables)
                })
                .collect()
        })
        .get(profile_id)?
        .get(name)
        .copied()
}

#[cfg(test)]
fn embedded_vanilla_csv(name: &str) -> Option<&'static str> {
    embedded_csv(VANILLA_PROFILE_ID, name)
}

#[cfg(test)]
fn embedded_convergence_csv(name: &str) -> Option<&'static str> {
    embedded_csv(CONVERGENCE_PROFILE_ID, name)
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
    let parsed = value
        .parse::<f32>()
        .map_err(|err| format!("invalid f32 for {field}: {value} ({err})"))?;
    if !parsed.is_finite() {
        return Err(format!("{field} must be finite: {value}"));
    }
    Ok(parsed)
}

fn optional_string(row: &csv::StringRecord, index: Option<usize>) -> String {
    index.map_or("", |index| &row[index]).to_string()
}

fn parse_status_buildup(value: &str, field: &str) -> Result<f32, String> {
    let parsed = parse_f32(value, field)?;
    if parsed == -99999.0 {
        return Ok(0.0);
    }
    if parsed < 0.0 {
        return Err(format!(
            "{field} must be a finite non-negative value or the -99999 missing-value sentinel: {value}"
        ));
    }
    Ok(parsed)
}

fn parse_bool_u8(value: &str, field: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!(
            "invalid flag for {field}: {value} (expected 0 or 1)"
        )),
    }
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

fn parse_optional_bool_u8(value: &str, field: &str) -> Result<Option<bool>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    parse_bool_u8(value, field).map(Some)
}

pub fn load_game_data(data_dir: impl AsRef<Path>) -> Result<GameData, String> {
    load_game_data_with_manifest(data_dir).map(|(data, _)| data)
}

pub fn load_game_data_with_manifest(
    data_dir: impl AsRef<Path>,
) -> Result<(GameData, SnapshotManifest), String> {
    let snapshot = validate_external_snapshot(data_dir.as_ref())?;
    load_validated_game_data(snapshot.manifest, |name| {
        snapshot.runtime_files.get(name).map(Vec::as_slice)
    })
}

fn load_validated_game_data<'a>(
    manifest: SnapshotManifest,
    content_for_file: impl Fn(&str) -> Option<&'a [u8]>,
) -> Result<(GameData, SnapshotManifest), String> {
    // The manifest requires every runtime table. Parse the same immutable bytes
    // that were verified, including tables whose supported row set can be empty.
    let table = |name: &str| {
        let bytes = content_for_file(name)
            .ok_or_else(|| format!("missing verified runtime CSV: {name}"))?;
        CsvTable::from_bytes(name.to_string(), bytes)
    };
    let weapons = load_weapons(table("weapons.csv")?)?;
    let reinforce = load_reinforce(table("reinforce.csv")?)?;
    let calc_correct = load_calc_correct(table("calc_correct.csv")?)?;
    let attack_element_correct = load_attack_element_correct(table("attack_element_correct.csv")?)?;
    let attack_element_correct_ext =
        load_attack_element_correct_ext(table("attack_element_correct_ext.csv")?)?;
    let aow_effects = load_aow_effects(table("aow_effect_data.csv")?)?;
    let aow_buffs = derive_aow_buffs(&aow_effects)?;
    let aows = load_aows(table("aow.csv")?, &aow_buffs)?;
    let aow_attack_rows = load_aow_attack_rows(table("aow_attack_data.csv")?)?;
    let native_skill_attack_rows =
        load_native_skill_attack_rows(table("native_skill_attack_data.csv")?)?;
    let aow_route_assignments = load_aow_route_assignments(table("aow_route_assignments.csv")?)?;
    let weapon_passives = load_weapon_passives(table("weapon_passives.csv")?)?;
    let weapon_passive_overlays =
        load_weapon_passive_overlays(table("weapon_passive_overlays.csv")?)?;
    let starting_classes = if !manifest.capabilities.class_budget {
        Vec::new()
    } else if requires_class_catalog(&manifest) {
        load_starting_classes(table(CLASS_CATALOG_FILE)?)?
    } else {
        STARTING_CLASSES.to_vec()
    };

    let data = GameData {
        dataset_version: manifest.dataset_version.clone(),
        model_version: crate::runtime_model_version(&manifest.model_version),
        profile_id: manifest.profile.id.clone(),
        profile_display_name: manifest.profile.display_name.clone(),
        capabilities: DataCapabilities {
            weapon_ar: manifest.capabilities.weapon_ar,
            weapon_ar_for_ammunition: manifest.capabilities.weapon_ar_for_ammunition,
            class_budget: manifest.capabilities.class_budget,
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
        starting_classes,
    };
    validate_cross_table_references(&data)?;
    Ok((data, manifest))
}

/// Rejects ambiguous keys, dangling references and capability/table disagreement at load time
/// instead of failing per weapon during a search.
fn validate_cross_table_references(data: &GameData) -> Result<(), String> {
    let mut weapon_ids = HashSet::with_capacity(data.weapons.len());
    for weapon in &data.weapons {
        let id = weapon.weapon_id;
        if !weapon_ids.insert(id) {
            return Err(format!("duplicate weapon_id: {id}"));
        }
        if data
            .reinforce
            .get(usize::from(weapon.reinforce_type))
            .is_none_or(Vec::is_empty)
        {
            return Err(format!(
                "weapon {id} references missing reinforce_type {}",
                weapon.reinforce_type
            ));
        }
        if data
            .attack_element(weapon.attack_element_correct_id)
            .is_none()
        {
            return Err(format!(
                "weapon {id} references missing attack_element_correct_id {}",
                weapon.attack_element_correct_id
            ));
        }
        let status = weapon.status_curve_ids;
        for curve_id in weapon.damage_curve_ids.into_iter().chain([
            status.poison,
            status.blood,
            status.sleep,
            status.madness,
        ]) {
            if data.calc_correct.get(curve_id).is_none_or(Option::is_none) {
                return Err(format!(
                    "weapon {id} references missing curve_id {curve_id}"
                ));
            }
        }
    }
    // A missing passive row would otherwise read as zero status buildup.
    if data.capabilities.status_buildup && !data.capabilities.weapon_passives {
        return Err("manifest statusBuildup requires weaponPassives".to_string());
    }
    if data.capabilities.weapon_passives {
        if let Some(weapon) = data
            .weapons
            .iter()
            .find(|weapon| !data.weapon_passives.contains_key(&weapon.weapon_id))
        {
            return Err(format!(
                "weapon {} has no weapon_passives.csv row",
                weapon.weapon_id
            ));
        }
    } else if !data.weapon_passives.is_empty() || !data.weapon_passive_overlays.is_empty() {
        return Err(
            "manifest weaponPassives=false disagrees with weapon passive tables".to_string(),
        );
    }
    let mut aow_ids = HashSet::with_capacity(data.aows.len());
    if let Some(aow) = data.aows.iter().find(|aow| !aow_ids.insert(aow.aow_id)) {
        return Err(format!("duplicate aow_id: {}", aow.aow_id));
    }
    if data.capabilities.aow_damage == data.aow_attack_rows.is_empty() {
        return Err(format!(
            "manifest aowDamage={} disagrees with aow_attack_data.csv",
            data.capabilities.aow_damage
        ));
    }
    if data.capabilities.aow_routes == data.aow_route_assignments.is_empty() {
        return Err(format!(
            "manifest aowRoutes={} disagrees with aow_route_assignments.csv",
            data.capabilities.aow_routes
        ));
    }
    Ok(())
}

pub fn load_embedded_game_profile(profile_id: &str) -> Result<GameData, String> {
    load_embedded_game_profile_with_manifest(profile_id).map(|(data, _)| data)
}

pub fn load_embedded_game_profile_with_manifest(
    profile_id: &str,
) -> Result<(GameData, SnapshotManifest), String> {
    let manifest_bytes: &'static [u8] = match profile_id {
        VANILLA_PROFILE_ID => include_bytes!("../../../data/phase1/manifest.json"),
        CONVERGENCE_PROFILE_ID => {
            include_bytes!("../../../data/profiles/convergence/manifest.json")
        }
        other => return Err(format!("unknown embedded game profile: {other}")),
    };
    let tables = embedded_tables(profile_id)?;
    let content = |name: &str| tables.get(name).map(Vec::as_slice);
    let manifest = validate_embedded_snapshot(manifest_bytes, content)?;
    if manifest.profile.id != profile_id {
        return Err(format!(
            "embedded profile id mismatch: requested {profile_id}, manifest contains {}",
            manifest.profile.id
        ));
    }
    load_validated_game_data(manifest, content)
}

/// Rune Level is the eight-attribute total minus this offset for every class.
const CLASS_LEVEL_OFFSET: u16 = 79;

fn load_starting_classes(table: CsvTable) -> Result<Vec<StartingClass>, String> {
    let [
        name_idx,
        level_idx,
        vig,
        mnd,
        end,
        str_idx,
        dex,
        int,
        fai,
        arc,
    ] = table.columns([
        "name",
        "base_level",
        "vig",
        "mnd",
        "end",
        "str",
        "dex",
        "int",
        "fai",
        "arc",
    ])?;
    table.idx("class_id")?;
    let mut classes: Vec<StartingClass> = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        let stat = |index: usize, field: &str| -> Result<u8, String> {
            let value = parse_u8(&row[index], field)?;
            if !(1..=99).contains(&value) {
                return Err(format!(
                    "{CLASS_CATALOG_FILE} {field} must be 1..99: {value}"
                ));
            }
            Ok(value)
        };
        let base_stats = Stats {
            vig: stat(vig, "vig")?,
            mnd: stat(mnd, "mnd")?,
            end: stat(end, "end")?,
            str: stat(str_idx, "str")?,
            dex: stat(dex, "dex")?,
            int: stat(int, "int")?,
            fai: stat(fai, "fai")?,
            arc: stat(arc, "arc")?,
        };
        let name = row[name_idx].to_string();
        let base_level = parse_u16(&row[level_idx], "base_level")?;
        let base_total = base_stats.sum_all_8();
        if base_level + CLASS_LEVEL_OFFSET != base_total {
            return Err(format!(
                "{CLASS_CATALOG_FILE} class {name} level {base_level} does not match its attribute total {base_total}"
            ));
        }
        if name.is_empty()
            || name.eq_ignore_ascii_case(CUSTOM_STATS_CLASS_NAME)
            || classes
                .iter()
                .any(|class_info| class_info.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!(
                "{CLASS_CATALOG_FILE} class name is empty, reserved, or duplicated: {name:?}"
            ));
        }
        classes.push(StartingClass {
            name: name.into(),
            base_level,
            base_total,
            base_stats,
        });
    }
    if classes.is_empty() {
        return Err(format!("{CLASS_CATALOG_FILE} contains no classes"));
    }
    Ok(classes)
}

fn load_weapons(table: CsvTable) -> Result<Vec<Weapon>, String> {
    let [
        native_skill_name,
        one_hand_light_poise,
        one_hand_heavy_poise,
        one_hand_charged_heavy_poise,
        one_hand_jumping_light_poise,
        one_hand_jumping_heavy_poise,
        two_hand_light_poise,
        two_hand_heavy_poise,
        two_hand_charged_heavy_poise,
        two_hand_jumping_light_poise,
        two_hand_jumping_heavy_poise,
    ] = table.optional_columns([
        "native_skill_name",
        "one_hand_light_poise",
        "one_hand_heavy_poise",
        "one_hand_charged_heavy_poise",
        "one_hand_jumping_light_poise",
        "one_hand_jumping_heavy_poise",
        "two_hand_light_poise",
        "two_hand_heavy_poise",
        "two_hand_charged_heavy_poise",
        "two_hand_jumping_light_poise",
        "two_hand_jumping_heavy_poise",
    ]);
    let [
        base_physical,
        base_magic,
        base_fire,
        base_lightning,
        base_holy,
        str_scaling,
        dex_scaling,
        int_scaling,
        fai_scaling,
        arc_scaling,
        req_str,
        req_dex,
        req_int,
        req_fai,
        req_arc,
        curve_id_physical,
        curve_id_magic,
        curve_id_fire,
        curve_id_lightning,
        curve_id_holy,
        weapon_id,
        name,
        affinity,
        native_skill_id,
        weapon_type_id,
        weapon_type_name,
        weapon_type_keys,
        weight,
        base_poise,
        critical_damage_percent,
        stamina_consumption_rate,
        move_count,
        physical_attribute_primary,
        physical_attribute_secondary,
        reinforce_type,
        attack_element_correct_id,
        curve_id_poison,
        curve_id_blood,
        curve_id_sleep,
        curve_id_madness,
        can_change_aow,
        disable_gem_attr,
        is_somber,
        disable_two_hand_bonus,
    ] = table.columns([
        "base_physical",
        "base_magic",
        "base_fire",
        "base_lightning",
        "base_holy",
        "str_scaling",
        "dex_scaling",
        "int_scaling",
        "fai_scaling",
        "arc_scaling",
        "req_str",
        "req_dex",
        "req_int",
        "req_fai",
        "req_arc",
        "curve_id_physical",
        "curve_id_magic",
        "curve_id_fire",
        "curve_id_lightning",
        "curve_id_holy",
        "weapon_id",
        "name",
        "affinity",
        "native_skill_id",
        "weapon_type_id",
        "weapon_type_name",
        "weapon_type_keys",
        "weight",
        "base_poise",
        "critical_damage_percent",
        "stamina_consumption_rate",
        "move_count",
        "physical_attribute_primary",
        "physical_attribute_secondary",
        "reinforce_type",
        "attack_element_correct_id",
        "curve_id_poison",
        "curve_id_blood",
        "curve_id_sleep",
        "curve_id_madness",
        "can_change_aow",
        "disable_gem_attr",
        "is_somber",
        "disable_two_hand_bonus",
    ])?;
    let mut out = Vec::with_capacity(table.rows.len());

    for row in &table.rows {
        let base = [
            parse_f32(&row[base_physical], "base_physical")?,
            parse_f32(&row[base_magic], "base_magic")?,
            parse_f32(&row[base_fire], "base_fire")?,
            parse_f32(&row[base_lightning], "base_lightning")?,
            parse_f32(&row[base_holy], "base_holy")?,
        ];
        let scaling = [
            parse_f32(&row[str_scaling], "str_scaling")?,
            parse_f32(&row[dex_scaling], "dex_scaling")?,
            parse_f32(&row[int_scaling], "int_scaling")?,
            parse_f32(&row[fai_scaling], "fai_scaling")?,
            parse_f32(&row[arc_scaling], "arc_scaling")?,
        ];
        let requirements = [
            parse_u8(&row[req_str], "req_str")?,
            parse_u8(&row[req_dex], "req_dex")?,
            parse_u8(&row[req_int], "req_int")?,
            parse_u8(&row[req_fai], "req_fai")?,
            parse_u8(&row[req_arc], "req_arc")?,
        ];
        let damage_curve_ids = [
            parse_usize(&row[curve_id_physical], "curve_id_physical")?,
            parse_usize(&row[curve_id_magic], "curve_id_magic")?,
            parse_usize(&row[curve_id_fire], "curve_id_fire")?,
            parse_usize(&row[curve_id_lightning], "curve_id_lightning")?,
            parse_usize(&row[curve_id_holy], "curve_id_holy")?,
        ];
        out.push(Weapon {
            weapon_id: parse_u32(&row[weapon_id], "weapon_id")?,
            name: row[name].to_string(),
            affinity: row[affinity].to_string(),
            native_skill_id: {
                let value = &row[native_skill_id];
                if value.is_empty() {
                    None
                } else {
                    Some(parse_u16(value, "native_skill_id")?)
                }
            },
            native_skill_name: native_skill_name
                .and_then(|index| (!row[index].is_empty()).then(|| row[index].to_string())),
            weapon_type_id: parse_u16(&row[weapon_type_id], "weapon_type_id")?,
            weapon_type_name: row[weapon_type_name].to_string(),
            weapon_type_keys: row[weapon_type_keys].to_string(),
            weight: parse_f32(&row[weight], "weight")?,
            base_poise: parse_f32(&row[base_poise], "base_poise")?,
            critical_damage_percent: parse_u16(
                &row[critical_damage_percent],
                "critical_damage_percent",
            )?,
            stamina_consumption_rate: parse_f32(
                &row[stamina_consumption_rate],
                "stamina_consumption_rate",
            )?,
            move_count: parse_u16(&row[move_count], "move_count")?,
            one_handed_poise: DisplayPoiseDamage {
                light: optional_string(row, one_hand_light_poise),
                heavy: optional_string(row, one_hand_heavy_poise),
                charged_heavy: optional_string(row, one_hand_charged_heavy_poise),
                jumping_light: optional_string(row, one_hand_jumping_light_poise),
                jumping_heavy: optional_string(row, one_hand_jumping_heavy_poise),
            },
            two_handed_poise: DisplayPoiseDamage {
                light: optional_string(row, two_hand_light_poise),
                heavy: optional_string(row, two_hand_heavy_poise),
                charged_heavy: optional_string(row, two_hand_charged_heavy_poise),
                jumping_light: optional_string(row, two_hand_jumping_light_poise),
                jumping_heavy: optional_string(row, two_hand_jumping_heavy_poise),
            },
            physical_attributes: [
                parse_physical_attack_attribute(&row[physical_attribute_primary])?,
                parse_physical_attack_attribute(&row[physical_attribute_secondary])?,
            ],
            base,
            scaling,
            requirements,
            reinforce_type: parse_u16(&row[reinforce_type], "reinforce_type")?,
            attack_element_correct_id: parse_usize(
                &row[attack_element_correct_id],
                "attack_element_correct_id",
            )?,
            damage_curve_ids,
            status_curve_ids: StatusCurveIds {
                poison: parse_usize(&row[curve_id_poison], "curve_id_poison")?,
                blood: parse_usize(&row[curve_id_blood], "curve_id_blood")?,
                sleep: parse_usize(&row[curve_id_sleep], "curve_id_sleep")?,
                madness: parse_usize(&row[curve_id_madness], "curve_id_madness")?,
            },
            can_change_aow: parse_bool_u8(&row[can_change_aow], "can_change_aow")?,
            disable_gem_attr: parse_bool_u8(&row[disable_gem_attr], "disable_gem_attr")?,
            is_somber: parse_bool_u8(&row[is_somber], "is_somber")?,
            disable_two_hand_bonus: parse_bool_u8(
                &row[disable_two_hand_bonus],
                "disable_two_hand_bonus",
            )?,
        });
    }
    Ok(out)
}

fn load_reinforce(table: CsvTable) -> Result<Vec<Vec<Option<ReinforceLevel>>>, String> {
    let [
        reinforce_type,
        level,
        physical_damage_mult,
        magic_damage_mult,
        fire_damage_mult,
        lightning_damage_mult,
        holy_damage_mult,
        str_scaling_mult,
        dex_scaling_mult,
        int_scaling_mult,
        fai_scaling_mult,
        arc_scaling_mult,
        base_attack_mult,
    ] = table.columns([
        "reinforce_type",
        "level",
        "physical_damage_mult",
        "magic_damage_mult",
        "fire_damage_mult",
        "lightning_damage_mult",
        "holy_damage_mult",
        "str_scaling_mult",
        "dex_scaling_mult",
        "int_scaling_mult",
        "fai_scaling_mult",
        "arc_scaling_mult",
        "base_attack_mult",
    ])?;
    let mut entries = Vec::with_capacity(table.rows.len());
    let mut max_type = 0usize;
    let mut max_level_by_type: HashMap<usize, usize> = HashMap::new();

    for row in &table.rows {
        let reinforce_type = usize::from(parse_u16(&row[reinforce_type], "reinforce_type")?);
        let level = usize::from(parse_u8(&row[level], "level")?);
        let damage_mult = [
            parse_f32(&row[physical_damage_mult], "physical_damage_mult")?,
            parse_f32(&row[magic_damage_mult], "magic_damage_mult")?,
            parse_f32(&row[fire_damage_mult], "fire_damage_mult")?,
            parse_f32(&row[lightning_damage_mult], "lightning_damage_mult")?,
            parse_f32(&row[holy_damage_mult], "holy_damage_mult")?,
        ];
        let scaling_mult = [
            parse_f32(&row[str_scaling_mult], "str_scaling_mult")?,
            parse_f32(&row[dex_scaling_mult], "dex_scaling_mult")?,
            parse_f32(&row[int_scaling_mult], "int_scaling_mult")?,
            parse_f32(&row[fai_scaling_mult], "fai_scaling_mult")?,
            parse_f32(&row[arc_scaling_mult], "arc_scaling_mult")?,
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
                base_attack_mult: parse_f32(&row[base_attack_mult], "base_attack_mult")?,
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
            && levels[level].replace(value).is_some()
        {
            return Err(format!(
                "duplicate reinforce entry reinforce_type={reinforce_type} level={level}"
            ));
        }
    }
    Ok(reinforce)
}

fn load_calc_correct(table: CsvTable) -> Result<Vec<Option<Vec<Option<f32>>>>, String> {
    let [curve_id, stat_value, multiplier] =
        table.columns(["curve_id", "stat_value", "multiplier"])?;
    let mut entries = Vec::with_capacity(table.rows.len());
    let mut max_curve_id = 0usize;

    for row in &table.rows {
        let curve_id = usize::from(parse_u16(&row[curve_id], "curve_id")?);
        let stat_value = usize::from(parse_u16(&row[stat_value], "stat_value")?);
        if stat_value > MAX_CURVE_STAT_VALUE {
            return Err(format!(
                "calc-correct stat_value {stat_value} exceeds {MAX_CURVE_STAT_VALUE}"
            ));
        }
        let multiplier = parse_f32(&row[multiplier], "multiplier")?;
        max_curve_id = max_curve_id.max(curve_id);
        entries.push((curve_id, stat_value, multiplier));
    }

    let max_stat_value = entries
        .iter()
        .map(|(_, stat_value, _)| *stat_value)
        .max()
        .unwrap_or(0);
    let mut out = vec![None; max_curve_id + 1];
    for (curve_id, stat_value, multiplier) in entries {
        let curve = out[curve_id].get_or_insert_with(|| vec![None; max_stat_value + 1]);
        if curve[stat_value].replace(multiplier).is_some() {
            return Err(format!(
                "duplicate calc-correct entry curve_id={curve_id} stat_value={stat_value}"
            ));
        }
    }
    Ok(out)
}

fn load_attack_element_correct(
    table: CsvTable,
) -> Result<Vec<(usize, AttackElementCorrect)>, String> {
    let [attack_element_correct_id] = table.columns(["attack_element_correct_id"])?;
    let mut entries = Vec::with_capacity(table.rows.len());

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

    let mut columns = [[0; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
    for (indices, names) in columns.iter_mut().zip(fields) {
        *indices = table.columns(names)?;
    }

    for row in &table.rows {
        let row_id = parse_usize(&row[attack_element_correct_id], "attack_element_correct_id")?;
        let mut scales = [[false; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        for stat_idx in 0..COMBAT_STAT_COUNT {
            for damage_idx in 0..DAMAGE_TYPE_COUNT {
                scales[stat_idx][damage_idx] = parse_bool_u8(
                    &row[columns[stat_idx][damage_idx]],
                    fields[stat_idx][damage_idx],
                )?;
            }
        }
        entries.push((row_id, AttackElementCorrect { scales }));
    }

    entries.sort_by_key(|(row_id, _)| *row_id);
    if let Some(pair) = entries.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(format!(
            "duplicate attack-element-correct entry id={}",
            pair[0].0
        ));
    }
    Ok(entries)
}

fn load_aows(table: CsvTable, buff_rows: &HashMap<u16, AowBuffRow>) -> Result<Vec<Aow>, String> {
    let [
        aow_id,
        name,
        bleed_buildup_add,
        frost_buildup_add,
        poison_buildup_add,
        scarlet_rot_buildup_add,
        valid_weapon_types,
        valid_affinities,
    ] = table.columns([
        "aow_id",
        "name",
        "bleed_buildup_add",
        "frost_buildup_add",
        "poison_buildup_add",
        "scarlet_rot_buildup_add",
        "valid_weapon_types",
        "valid_affinities",
    ])?;
    let mut out = Vec::with_capacity(table.rows.len());

    for row in &table.rows {
        let aow_id = parse_u16(&row[aow_id], "aow_id")?;
        let buff_row = buff_rows.get(&aow_id).cloned().unwrap_or_default();
        out.push(Aow {
            aow_id,
            name: row[name].to_string(),
            bleed_buildup_add: parse_status_buildup(&row[bleed_buildup_add], "bleed_buildup_add")?,
            frost_buildup_add: parse_status_buildup(&row[frost_buildup_add], "frost_buildup_add")?,
            poison_buildup_add: parse_status_buildup(
                &row[poison_buildup_add],
                "poison_buildup_add",
            )?,
            scarlet_rot_buildup_add: parse_status_buildup(
                &row[scarlet_rot_buildup_add],
                "scarlet_rot_buildup_add",
            )?,
            valid_weapon_types: row[valid_weapon_types].to_string(),
            valid_affinities: row[valid_affinities].to_string(),
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
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value
        .split('|')
        .map(|part| parse_u32(part.trim(), field))
        .collect()
}

fn load_aow_effects(table: CsvTable) -> Result<HashMap<(u16, u16), Vec<AowEffect>>, String> {
    let [
        record_id,
        aow_id,
        sheet_row,
        parent_effect_id,
        _,
        source_param_ids,
        effect_id,
        effect_name,
        _,
        role,
        activation_action_id,
        activation_timing,
        _,
        is_canonical,
        is_supported,
        reason,
        duration_seconds,
        physical_attack_power,
        magic_attack_power,
        fire_attack_power,
        lightning_attack_power,
        holy_attack_power,
        bleed_buildup,
        frost_buildup,
        poison_buildup,
        scarlet_rot_buildup,
        sleep_buildup,
        madness_buildup,
        death_buildup,
        uses_status_correction,
        uses_attack_correction,
    ] = table.columns([
        "record_id",
        "aow_id",
        "sheet_row",
        "parent_effect_id",
        "source_kind",
        "source_param_ids",
        "effect_id",
        "effect_name",
        "link_kind",
        "role",
        "activation_action_id",
        "activation_timing",
        "hand_variant",
        "is_canonical",
        "is_supported",
        "reason",
        "duration_seconds",
        "physical_attack_power",
        "magic_attack_power",
        "fire_attack_power",
        "lightning_attack_power",
        "holy_attack_power",
        "bleed_buildup",
        "frost_buildup",
        "poison_buildup",
        "scarlet_rot_buildup",
        "sleep_buildup",
        "madness_buildup",
        "death_buildup",
        "uses_status_correction",
        "uses_attack_correction",
    ])?;
    let mut out: HashMap<(u16, u16), Vec<AowEffect>> = HashMap::new();
    let mut record_ids = HashSet::with_capacity(table.rows.len());
    for row in &table.rows {
        let record_id = parse_u32(&row[record_id], "record_id")?;
        if !record_ids.insert(record_id) {
            return Err(format!("duplicate AoW effect record_id: {record_id}"));
        }
        let aow_id = parse_u16(&row[aow_id], "aow_id")?;
        let sheet_row = parse_u16(&row[sheet_row], "sheet_row")?;
        // Unused by the model, but still validated so malformed snapshots fail closed.
        parse_u32(&row[parent_effect_id], "parent_effect_id")?;
        parse_pipe_u32(&row[source_param_ids], "source_param_ids")?;
        parse_f32(&row[duration_seconds], "duration_seconds")?;
        parse_bool_u8(&row[uses_attack_correction], "uses_attack_correction")?;
        out.entry((aow_id, sheet_row)).or_default().push(AowEffect {
            record_id,
            aow_id,
            sheet_row,
            effect_id: parse_u32(&row[effect_id], "effect_id")?,
            effect_name: row[effect_name].to_string(),
            role: parse_aow_effect_role(&row[role])?,
            activation_action_id: row[activation_action_id].to_string(),
            activation_timing: row[activation_timing].to_string(),
            is_canonical: parse_optional_bool_u8(&row[is_canonical], "is_canonical")?,
            is_supported: parse_bool_u8(&row[is_supported], "is_supported")?,
            reason: row[reason].to_string(),
            attack_power: [
                parse_f32(&row[physical_attack_power], "physical_attack_power")?,
                parse_f32(&row[magic_attack_power], "magic_attack_power")?,
                parse_f32(&row[fire_attack_power], "fire_attack_power")?,
                parse_f32(&row[lightning_attack_power], "lightning_attack_power")?,
                parse_f32(&row[holy_attack_power], "holy_attack_power")?,
            ],
            status_buildup: StatusBuildup {
                bleed: parse_status_buildup(&row[bleed_buildup], "bleed_buildup")?,
                frost: parse_status_buildup(&row[frost_buildup], "frost_buildup")?,
                poison: parse_status_buildup(&row[poison_buildup], "poison_buildup")?,
                scarlet_rot: parse_status_buildup(
                    &row[scarlet_rot_buildup],
                    "scarlet_rot_buildup",
                )?,
                sleep: parse_status_buildup(&row[sleep_buildup], "sleep_buildup")?,
                madness: parse_status_buildup(&row[madness_buildup], "madness_buildup")?,
                death: parse_status_buildup(&row[death_buildup], "death_buildup")?,
            },
            uses_status_correction: parse_bool_u8(
                &row[uses_status_correction],
                "uses_status_correction",
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

fn load_attack_element_correct_ext(
    table: CsvTable,
) -> Result<HashMap<usize, AttackElementCorrectExt>, String> {
    let [attack_element_correct_id] = table.columns(["attack_element_correct_id"])?;
    let mut out = HashMap::with_capacity(table.rows.len());
    let mut columns = Vec::with_capacity(COMBAT_STAT_COUNT * DAMAGE_TYPE_COUNT);
    for stat_key in ["str", "dex", "int", "fai", "arc"] {
        for damage_key in ["physical", "magic", "fire", "lightning", "holy"] {
            let names = [
                format!("{stat_key}_scales_{damage_key}"),
                format!("{stat_key}_overwrite_{damage_key}"),
                format!("{stat_key}_influence_{damage_key}"),
            ];
            let indices = table.columns(names.each_ref().map(String::as_str))?;
            columns.push((names, indices));
        }
    }
    for row in &table.rows {
        let row_id = parse_usize(&row[attack_element_correct_id], "attack_element_correct_id")?;
        let mut scales = [[false; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        let mut overwrite = [[None; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        let mut influence = [[100.0_f32; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        for (position, (names, indices)) in columns.iter().enumerate() {
            let stat_idx = position / DAMAGE_TYPE_COUNT;
            let damage_idx = position % DAMAGE_TYPE_COUNT;
            scales[stat_idx][damage_idx] = parse_bool_u8(&row[indices[0]], &names[0])?;
            let overwrite_value = parse_f32(&row[indices[1]], &names[1])?;
            if overwrite_value >= 0.0 {
                overwrite[stat_idx][damage_idx] = Some(overwrite_value / 100.0);
            }
            influence[stat_idx][damage_idx] = parse_f32(&row[indices[2]], &names[2])? / 100.0;
        }
        if out
            .insert(
                row_id,
                AttackElementCorrectExt {
                    scales,
                    overwrite,
                    influence,
                },
            )
            .is_some()
        {
            return Err(format!(
                "duplicate attack-element-correct-ext entry id={row_id}"
            ));
        }
    }
    Ok(out)
}

fn load_aow_attack_rows(table: CsvTable) -> Result<HashMap<u16, Vec<AowAttackRow>>, String> {
    let attack_columns = aow_attack_columns(&table)?;
    let [aow_id] = table.columns(["aow_id"])?;
    let mut out: HashMap<u16, Vec<AowAttackRow>> = HashMap::new();
    for row in &table.rows {
        let aow_id = parse_u16(&row[aow_id], "aow_id")?;
        out.entry(aow_id)
            .or_default()
            .push(parse_aow_attack_row(&attack_columns, row, aow_id)?);
    }

    for rows in out.values_mut() {
        rows.sort_by_key(|row| row.sheet_row);
    }
    Ok(out)
}

fn load_native_skill_attack_rows(
    table: CsvTable,
) -> Result<HashMap<u32, Vec<AowAttackRow>>, String> {
    let attack_columns = aow_attack_columns(&table)?;
    let [weapon_id, aow_id] = table.columns(["weapon_id", "aow_id"])?;
    let mut out: HashMap<u32, Vec<AowAttackRow>> = HashMap::new();
    for row in &table.rows {
        let weapon_id = parse_u32(&row[weapon_id], "weapon_id")?;
        let aow_id = parse_u16(&row[aow_id], "aow_id")?;
        out.entry(weapon_id)
            .or_default()
            .push(parse_aow_attack_row(&attack_columns, row, aow_id)?);
    }

    for rows in out.values_mut() {
        rows.sort_by_key(|row| row.sheet_row);
    }
    Ok(out)
}

fn aow_attack_columns(table: &CsvTable) -> Result<[usize; 32], String> {
    table.columns([
        "poise_base",
        "overwrite_attack_element_correct_id",
        "sheet_row",
        "aow_name",
        "raw_name",
        "variant_weapon_type",
        "sequence_variant",
        "hit_kind",
        "hit_order",
        "is_lacking_fp",
        "atk_id",
        "is_disable_both_hands_bonus",
        "is_add_base_atk",
        "is_arrow_attack",
        "is_bullet_attack",
        "is_throw_attack",
        "physical_attack_attribute",
        "physical_mv",
        "magic_mv",
        "fire_mv",
        "lightning_mv",
        "holy_mv",
        "attack_base_physical",
        "attack_base_magic",
        "attack_base_fire",
        "attack_base_lightning",
        "attack_base_holy",
        "status_mv",
        "weapon_buff_mv",
        "poise_mv",
        "stamina_cost",
        "stamina_cost_mode",
    ])
}

fn parse_aow_attack_row(
    columns: &[usize; 32],
    row: &csv::StringRecord,
    aow_id: u16,
) -> Result<AowAttackRow, String> {
    let [
        poise_base,
        overwrite_attack_element_correct_id,
        sheet_row,
        aow_name,
        raw_name,
        variant_weapon_type,
        _,
        _,
        hit_order,
        is_lacking_fp,
        atk_id,
        is_disable_both_hands_bonus,
        is_add_base_atk,
        is_arrow_attack,
        is_bullet_attack,
        is_throw_attack,
        physical_attack_attribute,
        physical_mv,
        magic_mv,
        fire_mv,
        lightning_mv,
        holy_mv,
        attack_base_physical,
        attack_base_magic,
        attack_base_fire,
        attack_base_lightning,
        attack_base_holy,
        status_mv,
        weapon_buff_mv,
        poise_mv,
        stamina_cost,
        stamina_cost_mode,
    ] = *columns;
    let poise_base = parse_f32(&row[poise_base], "poise_base")?;
    if poise_base < 0.0 {
        return Err("poise_base must be nonnegative".into());
    }
    let overwrite_raw = row[overwrite_attack_element_correct_id]
        .parse::<i32>()
        .map_err(|err| {
            format!(
                "invalid i32 for overwrite_attack_element_correct_id: {} ({err})",
                &row[overwrite_attack_element_correct_id]
            )
        })?;
    Ok(AowAttackRow {
        sheet_row: parse_u16(&row[sheet_row], "sheet_row")?,
        aow_id,
        aow_name: row[aow_name].to_string(),
        raw_name: row[raw_name].to_string(),
        variant_weapon_type: row[variant_weapon_type].to_string(),
        hit_order: parse_u16(&row[hit_order], "hit_order")?,
        is_lacking_fp: parse_bool_u8(&row[is_lacking_fp], "is_lacking_fp")?,
        atk_id: parse_u32(&row[atk_id], "atk_id")?,
        overwrite_attack_element_correct_id: (overwrite_raw > 0).then_some(overwrite_raw as usize),
        is_disable_both_hands_bonus: parse_bool_u8(
            &row[is_disable_both_hands_bonus],
            "is_disable_both_hands_bonus",
        )?,
        is_add_base_atk: parse_bool_u8(&row[is_add_base_atk], "is_add_base_atk")?,
        is_arrow_attack: parse_bool_u8(&row[is_arrow_attack], "is_arrow_attack")?,
        is_bullet_attack: parse_bool_u8(&row[is_bullet_attack], "is_bullet_attack")?,
        is_throw_attack: parse_bool_u8(&row[is_throw_attack], "is_throw_attack")?,
        physical_attack_attribute: parse_physical_attack_attribute(
            &row[physical_attack_attribute],
        )?,
        motion_values: [
            parse_f32(&row[physical_mv], "physical_mv")?,
            parse_f32(&row[magic_mv], "magic_mv")?,
            parse_f32(&row[fire_mv], "fire_mv")?,
            parse_f32(&row[lightning_mv], "lightning_mv")?,
            parse_f32(&row[holy_mv], "holy_mv")?,
        ],
        attack_base: [
            parse_f32(&row[attack_base_physical], "attack_base_physical")?,
            parse_f32(&row[attack_base_magic], "attack_base_magic")?,
            parse_f32(&row[attack_base_fire], "attack_base_fire")?,
            parse_f32(&row[attack_base_lightning], "attack_base_lightning")?,
            parse_f32(&row[attack_base_holy], "attack_base_holy")?,
        ],
        status_mv: parse_f32(&row[status_mv], "status_mv")?,
        weapon_buff_mv: parse_f32(&row[weapon_buff_mv], "weapon_buff_mv")?,
        poise_mv: parse_f32(&row[poise_mv], "poise_mv")?,
        poise_base,
        stamina_cost: parse_f32(&row[stamina_cost], "stamina_cost")?,
        stamina_cost_mode: match &row[stamina_cost_mode] {
            "weapon_scaled" => StaminaCostMode::WeaponScaled,
            "precalculated" => StaminaCostMode::Precalculated,
            other => return Err(format!("invalid stamina_cost_mode: {other}")),
        },
    })
}

fn load_aow_route_assignments(
    table: CsvTable,
) -> Result<HashMap<(u16, u16), Vec<AowRouteAssignment>>, String> {
    let [
        aow_id,
        sheet_row,
        hit_count,
        route_id,
        route_label,
        route_priority,
        action_id,
        action_order,
        hit_order,
    ] = table.columns([
        "aow_id",
        "sheet_row",
        "hit_count",
        "route_id",
        "route_label",
        "route_priority",
        "action_id",
        "action_order",
        "hit_order",
    ])?;
    let mut out: HashMap<(u16, u16), Vec<AowRouteAssignment>> = HashMap::new();
    for row in &table.rows {
        let aow_id = parse_u16(&row[aow_id], "aow_id")?;
        let sheet_row = parse_u16(&row[sheet_row], "sheet_row")?;
        let hit_count = parse_u16(&row[hit_count], "hit_count")?;
        if hit_count == 0 {
            return Err("route hit_count must be positive".into());
        }
        out.entry((aow_id, sheet_row))
            .or_default()
            .push(AowRouteAssignment {
                route_id: row[route_id].to_string(),
                route_label: row[route_label].to_string(),
                route_priority: parse_u16(&row[route_priority], "route_priority")?,
                action_id: row[action_id].to_string(),
                action_order: parse_u16(&row[action_order], "action_order")?,
                hit_order: parse_u16(&row[hit_order], "hit_order")?,
                hit_count,
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

fn load_weapon_passives(table: CsvTable) -> Result<HashMap<u32, StatusEffectSource>, String> {
    let status_columns = status_effect_columns(&table)?;
    let [weapon_id] = table.columns(["weapon_id"])?;
    let mut out = HashMap::with_capacity(table.rows.len());
    for row in &table.rows {
        let weapon_id = parse_u32(&row[weapon_id], "weapon_id")?;
        if out
            .insert(weapon_id, parse_status_effect_source(&status_columns, row)?)
            .is_some()
        {
            return Err(format!(
                "duplicate weapon passive entry weapon_id={weapon_id}"
            ));
        }
    }
    Ok(out)
}

fn status_effect_columns(table: &CsvTable) -> Result<[usize; 14], String> {
    table.columns([
        "bleed",
        "frost",
        "poison",
        "scarlet_rot",
        "sleep",
        "madness",
        "death",
        "bleed_uses_status_correction",
        "frost_uses_status_correction",
        "poison_uses_status_correction",
        "scarlet_rot_uses_status_correction",
        "sleep_uses_status_correction",
        "madness_uses_status_correction",
        "death_uses_status_correction",
    ])
}

fn parse_status_effect_source(
    columns: &[usize; 14],
    row: &csv::StringRecord,
) -> Result<StatusEffectSource, String> {
    let [
        bleed,
        frost,
        poison,
        scarlet_rot,
        sleep,
        madness,
        death,
        bleed_uses_status_correction,
        frost_uses_status_correction,
        poison_uses_status_correction,
        scarlet_rot_uses_status_correction,
        sleep_uses_status_correction,
        madness_uses_status_correction,
        death_uses_status_correction,
    ] = *columns;
    Ok(StatusEffectSource {
        buildup: StatusBuildup {
            bleed: parse_status_buildup(&row[bleed], "bleed")?,
            frost: parse_status_buildup(&row[frost], "frost")?,
            poison: parse_status_buildup(&row[poison], "poison")?,
            scarlet_rot: parse_status_buildup(&row[scarlet_rot], "scarlet_rot")?,
            sleep: parse_status_buildup(&row[sleep], "sleep")?,
            madness: parse_status_buildup(&row[madness], "madness")?,
            death: parse_status_buildup(&row[death], "death")?,
        },
        correction_flags: StatusCorrectionFlags {
            bleed: parse_optional_bool_u8(
                &row[bleed_uses_status_correction],
                "bleed_uses_status_correction",
            )?,
            frost: parse_optional_bool_u8(
                &row[frost_uses_status_correction],
                "frost_uses_status_correction",
            )?,
            poison: parse_optional_bool_u8(
                &row[poison_uses_status_correction],
                "poison_uses_status_correction",
            )?,
            scarlet_rot: parse_optional_bool_u8(
                &row[scarlet_rot_uses_status_correction],
                "scarlet_rot_uses_status_correction",
            )?,
            sleep: parse_optional_bool_u8(
                &row[sleep_uses_status_correction],
                "sleep_uses_status_correction",
            )?,
            madness: parse_optional_bool_u8(
                &row[madness_uses_status_correction],
                "madness_uses_status_correction",
            )?,
            death: parse_optional_bool_u8(
                &row[death_uses_status_correction],
                "death_uses_status_correction",
            )?,
        },
    })
}

fn load_weapon_passive_overlays(
    table: CsvTable,
) -> Result<HashMap<u32, Vec<Option<StatusEffectSource>>>, String> {
    let status_columns = status_effect_columns(&table)?;
    let [weapon_id, level] = table.columns(["weapon_id", "level"])?;
    let mut max_level_by_weapon = HashMap::<u32, usize>::new();
    let mut entries = Vec::<(u32, usize, StatusEffectSource)>::with_capacity(table.rows.len());
    for row in &table.rows {
        let weapon_id = parse_u32(&row[weapon_id], "weapon_id")?;
        let level = usize::from(parse_u8(&row[level], "level")?);
        let source = parse_status_effect_source(&status_columns, row)?;
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
        if let Some(levels) = out.get_mut(&weapon_id)
            && levels[level].replace(source).is_some()
        {
            return Err(format!(
                "duplicate weapon passive overlay entry weapon_id={weapon_id} level={level}"
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::fs;

    mod csv_contract {
        use super::super::*;
        use sha2::{Digest, Sha256};
        use std::collections::BTreeMap;

        fn load_table(name: &str, table: CsvTable) -> Result<(), String> {
            match name {
                "weapons.csv" => load_weapons(table).map(drop),
                "reinforce.csv" => load_reinforce(table).map(drop),
                "calc_correct.csv" => load_calc_correct(table).map(drop),
                "attack_element_correct.csv" => load_attack_element_correct(table).map(drop),
                "attack_element_correct_ext.csv" => {
                    load_attack_element_correct_ext(table).map(drop)
                }
                "aow.csv" => load_aows(table, &HashMap::new()).map(drop),
                "aow_effect_data.csv" => load_aow_effects(table).map(drop),
                "aow_attack_data.csv" => load_aow_attack_rows(table).map(drop),
                "native_skill_attack_data.csv" => load_native_skill_attack_rows(table).map(drop),
                "aow_route_assignments.csv" => load_aow_route_assignments(table).map(drop),
                "weapon_passives.csv" => load_weapon_passives(table).map(drop),
                "weapon_passive_overlays.csv" => load_weapon_passive_overlays(table).map(drop),
                _ => panic!("unknown test table: {name}"),
            }
        }

        fn without_columns(content: &str, removed: &[&str], headers_only: bool) -> String {
            let mut reader = csv::Reader::from_reader(content.as_bytes());
            let headers = reader.headers().unwrap().clone();
            let indices = headers
                .iter()
                .enumerate()
                .filter(|(_, field)| !removed.contains(field))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer
                .write_record(indices.iter().map(|index| &headers[*index]))
                .unwrap();
            if !headers_only {
                for row in reader.records() {
                    let row = row.unwrap();
                    writer
                        .write_record(indices.iter().map(|index| &row[*index]))
                        .unwrap();
                }
            }
            String::from_utf8(writer.into_inner().unwrap()).unwrap()
        }

        #[test]
        fn required_headers_are_checked_even_without_rows_for_both_profiles() {
            for embedded in [embedded_vanilla_csv, embedded_convergence_csv] {
                for name in crate::snapshot::RUNTIME_DATA_FILES {
                    let content = embedded(name).unwrap();
                    let header = content.lines().next().unwrap();
                    let field = header.split(',').next().unwrap();
                    let valid_empty =
                        CsvTable::from_content(name.into(), &format!("{header}\n")).unwrap();
                    load_table(name, valid_empty).unwrap();
                    let invalid = without_columns(content, &[field], true);
                    let table = CsvTable::from_content(name.into(), &invalid).unwrap();
                    let error = load_table(name, table)
                        .expect_err("empty rows must not hide missing required headers");
                    assert!(error.contains(field), "{name}: {error}");
                }
            }
        }

        #[test]
        fn weapon_numeric_and_handling_headers_never_become_defaults() {
            for embedded in [embedded_vanilla_csv, embedded_convergence_csv] {
                let content = embedded("weapons.csv").unwrap();
                for field in [
                    "disable_gem_attr",
                    "disable_two_hand_bonus",
                    "native_skill_id",
                    "weight",
                    "base_poise",
                    "move_count",
                ] {
                    let invalid = without_columns(content, &[field], false);
                    let table = CsvTable::from_content("weapons.csv".into(), &invalid).unwrap();
                    let error = match load_weapons(table) {
                        Ok(_) => panic!("required field {field} must not be fabricated"),
                        Err(error) => error,
                    };
                    assert!(error.contains(field), "{field}: {error}");
                }
            }
        }

        #[test]
        fn malformed_current_schema_fails_after_its_hash_is_verified() {
            for (embedded, manifest_bytes) in [
                (
                    embedded_vanilla_csv as fn(&str) -> Option<&'static str>,
                    include_bytes!("../../../data/phase1/manifest.json").as_slice(),
                ),
                (
                    embedded_convergence_csv,
                    include_bytes!("../../../data/profiles/convergence/manifest.json").as_slice(),
                ),
            ] {
                let invalid = without_columns(
                    embedded("weapons.csv").unwrap(),
                    &["disable_two_hand_bonus"],
                    false,
                );
                let invalid: &'static [u8] = Box::leak(invalid.into_bytes().into_boxed_slice());
                let mut manifest: serde_json::Value =
                    serde_json::from_slice(manifest_bytes).unwrap();
                let record = manifest["runtimeFiles"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|file| file["path"] == "weapons.csv")
                    .unwrap();
                record["size"] = invalid.len().into();
                record["sha256"] = format!("{:x}", Sha256::digest(invalid)).into();
                let content = |name: &str| {
                    if name == "weapons.csv" {
                        Some(invalid)
                    } else {
                        embedded(name).map(str::as_bytes)
                    }
                };
                let verified =
                    validate_embedded_snapshot(&serde_json::to_vec(&manifest).unwrap(), content)
                        .unwrap();
                let error = match load_validated_game_data(verified, content) {
                    Ok(_) => panic!("verified hash must not substitute for required headers"),
                    Err(error) => error,
                };
                assert!(error.contains("disable_two_hand_bonus"), "{error}");
            }
        }

        #[test]
        fn weapon_missing_from_passives_fails_closed_instead_of_zero_status() {
            let passives = embedded_vanilla_csv("weapon_passives.csv").unwrap();
            let mut lines = passives.lines();
            let header = lines.next().unwrap();
            let dropped = lines.next().unwrap();
            let weapon_id = dropped.split(',').next().unwrap();
            let rest = lines.collect::<Vec<_>>().join("\n");
            let invalid: &'static [u8] = Box::leak(
                format!("{header}\n{rest}\n")
                    .into_bytes()
                    .into_boxed_slice(),
            );
            let mut manifest: serde_json::Value =
                serde_json::from_slice(include_bytes!("../../../data/phase1/manifest.json"))
                    .unwrap();
            let record = manifest["runtimeFiles"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|file| file["path"] == "weapon_passives.csv")
                .unwrap();
            record["size"] = invalid.len().into();
            record["sha256"] = format!("{:x}", Sha256::digest(invalid)).into();
            let content = |name: &str| {
                if name == "weapon_passives.csv" {
                    Some(invalid)
                } else {
                    embedded_vanilla_csv(name).map(str::as_bytes)
                }
            };
            let verified =
                validate_embedded_snapshot(&serde_json::to_vec(&manifest).unwrap(), content)
                    .unwrap();
            let error = match load_validated_game_data(verified, content) {
                Ok(_) => panic!("a weapon without passive data must not load as zero status"),
                Err(error) => error,
            };
            assert!(
                error.contains(weapon_id) && error.contains("weapon_passives.csv"),
                "{error}"
            );
        }

        #[test]
        fn weapon_passive_capability_must_agree_with_status_and_tables() {
            let mut data = load_embedded_game_profile(VANILLA_PROFILE_ID).unwrap();
            validate_cross_table_references(&data).unwrap();
            data.capabilities.weapon_passives = false;
            assert!(validate_cross_table_references(&data).is_err());
            data.capabilities.status_buildup = false;
            let error = validate_cross_table_references(&data).unwrap_err();
            assert!(error.contains("weaponPassives=false"), "{error}");
            data.weapon_passives.clear();
            data.weapon_passive_overlays.clear();
            validate_cross_table_references(&data).unwrap();
        }

        #[test]
        fn unused_effect_columns_still_fail_closed() {
            let content = embedded_vanilla_csv("aow_effect_data.csv").unwrap();
            let mut lines = content.lines();
            let header = lines.next().unwrap();
            let first = lines.next().unwrap();
            for field in [
                "parent_effect_id",
                "source_param_ids",
                "duration_seconds",
                "uses_attack_correction",
            ] {
                let index = header.split(',').position(|name| name == field).unwrap();
                let mut row = first.split(',').collect::<Vec<_>>();
                row[index] = "x";
                let invalid = format!("{header}\n{}\n", row.join(","));
                let table = CsvTable::from_content("aow_effect_data.csv".into(), &invalid).unwrap();
                let error = load_aow_effects(table).unwrap_err();
                assert!(error.contains(field), "{field}: {error}");
            }
        }

        #[test]
        fn embedded_tables_are_exactly_the_manifest_runtime_files() {
            for (profile, extra) in [
                (VANILLA_PROFILE_ID, &[][..]),
                (CONVERGENCE_PROFILE_ID, &[CLASS_CATALOG_FILE][..]),
            ] {
                let names = EMBEDDED_TABLES
                    .iter()
                    .filter(|(embedded, _, _)| *embedded == profile)
                    .map(|(_, table, _)| *table)
                    .collect::<Vec<_>>();
                let expected = crate::snapshot::RUNTIME_DATA_FILES
                    .iter()
                    .chain(extra)
                    .copied()
                    .collect::<Vec<_>>();
                assert_eq!(names, expected, "{profile}");
            }
        }

        #[test]
        fn attack_element_lookup_sorts_rows_and_misses_cleanly() {
            let content = embedded_vanilla_csv("attack_element_correct.csv").unwrap();
            let mut lines = content.lines();
            let header = lines.next().unwrap();
            let mut rows = lines.collect::<Vec<_>>();
            rows.reverse();
            let table =
                CsvTable::from_content("aec".into(), &format!("{header}\n{}\n", rows.join("\n")))
                    .unwrap();
            let entries = load_attack_element_correct(table).unwrap();
            assert!(entries.windows(2).all(|pair| pair[0].0 < pair[1].0));
            let mut data = load_embedded_game_profile(VANILLA_PROFILE_ID).unwrap();
            data.attack_element_correct = entries;
            for id in rows
                .iter()
                .map(|row| row.split(',').next().unwrap().parse::<usize>().unwrap())
            {
                assert!(data.attack_element(id).is_some(), "{id}");
            }
            assert!(data.attack_element(usize::MAX).is_none());
        }

        #[test]
        fn duplicate_empty_headers_and_short_rows_fail_before_loading() {
            for content in ["weapon_id,weapon_id\n", "weapon_id,name\n1\n"] {
                assert!(CsvTable::from_content("weapons.csv".into(), content).is_err());
            }
        }

        #[test]
        fn descriptive_weapon_columns_remain_explicitly_optional() {
            let optional = [
                "native_skill_name",
                "one_hand_light_poise",
                "one_hand_heavy_poise",
                "one_hand_charged_heavy_poise",
                "one_hand_jumping_light_poise",
                "one_hand_jumping_heavy_poise",
                "two_hand_light_poise",
                "two_hand_heavy_poise",
                "two_hand_charged_heavy_poise",
                "two_hand_jumping_light_poise",
                "two_hand_jumping_heavy_poise",
            ];
            for embedded in [embedded_vanilla_csv, embedded_convergence_csv] {
                let content = embedded("weapons.csv").unwrap();
                let table = CsvTable::from_content("weapons.csv".into(), content).unwrap();
                let mut expected = load_weapons(table).unwrap();
                for weapon in &mut expected {
                    weapon.native_skill_name = None;
                    weapon.one_handed_poise = DisplayPoiseDamage::default();
                    weapon.two_handed_poise = DisplayPoiseDamage::default();
                }
                let content = without_columns(content, &optional, false);
                let actual =
                    load_weapons(CsvTable::from_content("weapons.csv".into(), &content).unwrap())
                        .unwrap();
                assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            }
        }

        #[test]
        #[ignore = "explicit cold-process loader measurement; run without competing work"]
        fn cold_process_loader_probe() {
            let profile = std::env::var("ER_LOADER_PROFILE").unwrap_or_else(|_| "vanilla".into());
            let relative = match profile.as_str() {
                "vanilla" => "../../data/phase1",
                "convergence" => "../../data/profiles/convergence",
                _ => panic!("unknown measurement profile"),
            };
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
            let start = std::time::Instant::now();
            let data = load_game_data(path).unwrap();
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let mut hash = Sha256::new();
            for part in [
                format!(
                    "{:?}",
                    (
                        &data.dataset_version,
                        &data.model_version,
                        &data.profile_id,
                        &data.profile_display_name,
                        &data.capabilities,
                        &data.rules
                    )
                ),
                format!(
                    "{:?}",
                    (
                        &data.weapons,
                        &data.reinforce,
                        &data.calc_correct,
                        &data.attack_element_correct,
                        &data.aows
                    )
                ),
                format!(
                    "{:?}",
                    data.attack_element_correct_ext
                        .iter()
                        .collect::<BTreeMap<_, _>>()
                ),
                format!(
                    "{:?}",
                    data.aow_attack_rows.iter().collect::<BTreeMap<_, _>>()
                ),
                format!(
                    "{:?}",
                    data.native_skill_attack_rows
                        .iter()
                        .collect::<BTreeMap<_, _>>()
                ),
                format!(
                    "{:?}",
                    data.aow_route_assignments
                        .iter()
                        .collect::<BTreeMap<_, _>>()
                ),
                format!("{:?}", data.aow_effects.iter().collect::<BTreeMap<_, _>>()),
                format!(
                    "{:?}",
                    data.weapon_passives.iter().collect::<BTreeMap<_, _>>()
                ),
                format!(
                    "{:?}",
                    data.weapon_passive_overlays
                        .iter()
                        .collect::<BTreeMap<_, _>>()
                ),
            ] {
                hash.update(part.as_bytes());
            }
            println!(
                "LOADER {}",
                serde_json::json!({"profile":profile,"loadMs":elapsed,"fingerprint":format!("{:x}", hash.finalize())})
            );
        }
    }

    use super::{
        CONVERGENCE_PROFILE_ID, CsvTable, VANILLA_PROFILE_ID, load_attack_element_correct,
        load_attack_element_correct_ext, load_calc_correct, load_embedded_game_profile,
        load_game_data, load_reinforce, load_weapon_passive_overlays, load_weapon_passives,
        parse_f32, parse_status_effect_source,
    };
    use crate::model::AowEffectRole;

    #[test]
    fn route_counts_are_required_positive_integers() {
        let header = "aow_id,sheet_row,route_id,route_label,route_priority,action_id,action_order,hit_order,hit_count\n";
        for value in ["", "0", "-1", "1.5", "65536"] {
            let csv = format!("{header}200,1224,full,Full,0,cast,1,0,{value}\n");
            let table = CsvTable::from_content("test".into(), &csv).unwrap();
            assert!(super::load_aow_route_assignments(table).is_err(), "{value}");
        }
        let csv = format!("{header}200,1224,full,Full,0,cast,1,0,4\n");
        let table = CsvTable::from_content("test".into(), &csv).unwrap();
        assert_eq!(
            super::load_aow_route_assignments(table).unwrap()[&(200, 1224)][0].hit_count,
            4
        );
    }

    fn duplicate_first_data_row(content: &str) -> String {
        let mut lines = content.lines();
        let header = lines.next().expect("CSV header");
        let row = lines.next().expect("CSV data row");
        format!("{header}\n{row}\n{row}\n")
    }

    fn assert_duplicate_row_rejected<T>(
        name: &str,
        content: &str,
        load: impl FnOnce(CsvTable) -> Result<T, String>,
        expected_error: &str,
    ) {
        let path = std::env::temp_dir().join(format!(
            "tarnisheds-arsenal-duplicate-{name}-{}.csv",
            std::process::id()
        ));
        fs::write(&path, duplicate_first_data_row(content)).unwrap();
        let table = CsvTable::from_bytes(name.to_string(), &fs::read(&path).unwrap()).unwrap();
        let error = match load(table) {
            Ok(_) => panic!("duplicate {name} row must fail"),
            Err(error) => error,
        };
        fs::remove_file(path).unwrap();
        assert!(error.contains(expected_error), "unexpected error: {error}");
    }

    #[test]
    fn external_snapshot_parses_verified_bytes_after_files_are_replaced() {
        let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/phase1");
        let manifest = crate::snapshot::validate_external_snapshot(&original)
            .unwrap()
            .manifest;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("verified-snapshot-{}-{unique}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let names = std::iter::once("manifest.json")
            .chain(manifest.runtime_files.iter().map(|file| file.path.as_str()))
            .chain(
                manifest
                    .diagnostic_files
                    .iter()
                    .map(|file| file.path.as_str()),
            )
            .chain(
                manifest
                    .sources
                    .iter()
                    .filter(|source| source.bundled)
                    .map(|source| source.path.as_str()),
            )
            .collect::<Vec<_>>();
        for name in &names {
            fs::copy(original.join(name), directory.join(name)).unwrap();
        }
        let verified = crate::snapshot::validate_external_snapshot(&directory).unwrap();
        for record in &verified.manifest.runtime_files {
            fs::write(directory.join(&record.path), b"replaced,invalid\n1,2\n").unwrap();
        }
        fs::write(directory.join("manifest.json"), b"replaced manifest").unwrap();
        let result = super::load_validated_game_data(verified.manifest, |name| {
            verified.runtime_files.get(name).map(Vec::as_slice)
        });
        for name in &names {
            fs::remove_file(directory.join(name)).unwrap();
        }
        fs::remove_dir(&directory).unwrap();
        let (data, loaded_manifest) = result.expect("parse only the originally verified bytes");
        assert_eq!(loaded_manifest.id, manifest.id);
        assert_eq!(data.profile_id, super::VANILLA_PROFILE_ID);
        assert!(data.weapons.len() > 3000);
        assert!(data.aows.len() > 100);
        assert!(!data.aow_effects.is_empty());
    }

    #[test]
    fn explicit_embedded_snapshot_loads_all_runtime_tables() {
        let data = load_embedded_game_profile(VANILLA_PROFILE_ID).expect("embedded snapshot loads");
        assert!(data.weapons.len() > 3000);
        assert!(
            data.weapons
                .iter()
                .any(|weapon| weapon.name == "Reverse-Bladed Sword")
        );
        let dagger = data
            .weapons
            .iter()
            .find(|weapon| weapon.weapon_id == 1_000_000)
            .expect("standard Dagger");
        assert_eq!(dagger.one_handed_poise.light, "3.0");
        assert_eq!(dagger.one_handed_poise.jumping_light, "4.5");
        assert_eq!(dagger.base_poise, 3.0);
        assert!(data.aows.len() > 100);
        assert!(!data.aow_effects.is_empty());
    }

    #[test]
    fn calc_correct_holes_are_missing_and_duplicates_fail() {
        let path = std::env::temp_dir().join(format!(
            "tarnisheds-arsenal-calc-correct-{}.csv",
            std::process::id()
        ));
        fs::write(&path, "curve_id,stat_value,multiplier\n1,0,0\n1,2,0.5\n").unwrap();
        let curves = load_calc_correct(
            CsvTable::from_bytes("calc_correct.csv".into(), &fs::read(&path).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(curves[0].is_none());
        assert!(curves[1].as_ref().unwrap()[1].is_none());

        fs::write(&path, "curve_id,stat_value,multiplier\n1,0,0\n1,0,0.5\n").unwrap();
        let error = load_calc_correct(
            CsvTable::from_bytes("calc_correct.csv".into(), &fs::read(&path).unwrap()).unwrap(),
        )
        .unwrap_err();
        fs::remove_file(path).unwrap();
        assert!(error.contains("duplicate calc-correct entry"));
    }

    #[test]
    fn duplicate_trimmed_csv_headers_fail_closed() {
        let error = match CsvTable::from_content("test.csv".to_string(), "id, id \n1,2\n") {
            Ok(_) => panic!("duplicate trimmed headers must fail"),
            Err(error) => error,
        };
        assert!(error.contains("duplicate csv column header: id"));
    }

    #[test]
    fn keyed_csv_loaders_reject_duplicate_rows() {
        assert_duplicate_row_rejected(
            "reinforce",
            super::embedded_vanilla_csv("reinforce.csv").unwrap(),
            load_reinforce,
            "duplicate reinforce entry",
        );
        assert_duplicate_row_rejected(
            "attack-element-correct",
            super::embedded_vanilla_csv("attack_element_correct.csv").unwrap(),
            load_attack_element_correct,
            "duplicate attack-element-correct entry",
        );
        assert_duplicate_row_rejected(
            "attack-element-correct-ext",
            super::embedded_vanilla_csv("attack_element_correct_ext.csv").unwrap(),
            load_attack_element_correct_ext,
            "duplicate attack-element-correct-ext entry",
        );
        assert_duplicate_row_rejected(
            "weapon-passives",
            super::embedded_vanilla_csv("weapon_passives.csv").unwrap(),
            load_weapon_passives,
            "duplicate weapon passive entry",
        );
        assert_duplicate_row_rejected(
            "weapon-passive-overlays",
            super::embedded_vanilla_csv("weapon_passive_overlays.csv").unwrap(),
            load_weapon_passive_overlays,
            "duplicate weapon passive overlay entry",
        );
    }

    #[test]
    fn convergence_snapshot_is_isolated_and_declares_partial_aow_coverage() {
        let data = load_embedded_game_profile(CONVERGENCE_PROFILE_ID)
            .expect("Convergence embedded snapshot loads");
        assert_eq!(data.profile_id, CONVERGENCE_PROFILE_ID);
        assert_eq!(data.dataset_version, "convergence-3.0.3.0");
        assert_eq!(data.rules.standard_max_upgrade, 15);
        assert_eq!(data.rules.somber_max_upgrade, 15);
        assert!(!data.rules.separate_upgrade_caps);
        assert!(!data.capabilities.weapon_ar_for_ammunition);
        assert!(data.capabilities.class_budget);
        assert!(!data.rules.scadutree_scaling);
        assert!(!data.rules.zero_attack_element_uses_weapon_scaling);
        // The regulation's own classes, never Vanilla's, plus the exact Custom stats entry.
        assert_eq!(data.starting_classes.len(), 29);
        let retainer = data.class_by_name("Retainer").expect("Convergence class");
        assert_eq!((retainer.base_level, retainer.base_total), (9, 88));
        assert!(data.class_by_name("Samurai").is_none());
        assert!(data.offers_custom_stats());
        // Every player weapon reads its AttackElementCorrect row; row 0 stays raw.
        assert!(
            data.weapons
                .iter()
                .all(|weapon| matches!(weapon.attack_element_correct_id, 10_000 | 16_000))
        );
        assert!(data.rules.extended_scaling_grades);
        assert!(!data.rules.status_buildup_scales);
        assert_eq!(data.weapons.len(), 3305);
        assert!(data.weapons.iter().any(|weapon| weapon.affinity == "Glint"));
        assert!(data.weapons.iter().any(|weapon| {
            weapon.weapon_id == 10_200_000 && weapon.name == "Galvanic Culling Blade [Twinblade]"
        }));
        let galvanic = data
            .weapons
            .iter()
            .find(|weapon| weapon.weapon_id == 10_200_000)
            .expect("Galvanic twinblade");
        let inseparable = data
            .weapons
            .iter()
            .find(|weapon| weapon.weapon_id == 2_090_000)
            .expect("Inseparable Sword");
        let three_finger = data
            .weapons
            .iter()
            .find(|weapon| weapon.weapon_id == 8_090_000)
            .expect("Three Finger Blade");
        assert!(inseparable.is_somber);
        assert!(three_finger.is_somber);
        assert_eq!(data.rules.somber_max_upgrade, 15);
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
                .any(|aow| aow.aow_id == 121 && aow.name == "Storm Stomp")
        );
        assert!(data.weapons.iter().any(|weapon| {
            weapon.native_skill_id == Some(815)
                && weapon.native_skill_name.as_deref() == Some("Ancient Thunderclap")
        }));
        assert!(!data.capabilities.aow_damage);
        assert!(!data.capabilities.aow_routes);
        let universal = data
            .attack_element(16_000)
            .expect("Convergence universal scaling row");
        assert!(universal.scales.iter().flatten().all(|enabled| *enabled));
        let raw_zero = data.attack_element(0).expect("raw row 0");
        assert!(raw_zero.scales.iter().flatten().all(|enabled| !*enabled));
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
    fn status_buildup_normalizes_only_the_known_missing_sentinel() {
        let table = CsvTable::from_content(
            "test.csv".to_string(),
            "bleed,frost,poison,scarlet_rot,sleep,madness,death,bleed_uses_status_correction,frost_uses_status_correction,poison_uses_status_correction,scarlet_rot_uses_status_correction,sleep_uses_status_correction,madness_uses_status_correction,death_uses_status_correction\n-99999,0,0,0,0,0,0,,,,,,,\n-0.5,0,0,0,0,0,0,,,,,,,\n",
        )
        .expect("status CSV parses");
        let source = parse_status_effect_source(
            &super::status_effect_columns(&table).unwrap(),
            &table.rows[0],
        )
        .expect("known missing-value sentinel must load");
        assert_eq!(source.buildup.bleed, 0.0);

        let error = parse_status_effect_source(
            &super::status_effect_columns(&table).unwrap(),
            &table.rows[1],
        )
        .expect_err("negative buildup must fail closed");
        assert!(error.contains("bleed"));
        assert!(error.contains("finite non-negative"));
    }

    #[test]
    fn numeric_csv_fields_reject_non_finite_values() {
        for value in ["NaN", "inf", "-inf"] {
            let error = parse_f32(value, "test_value").expect_err("non-finite value must fail");
            assert!(error.contains("test_value must be finite"));
        }
    }

    #[test]
    fn persistent_status_effect_roles_remain_separate() {
        let data = load_embedded_game_profile(VANILLA_PROFILE_ID).expect("embedded snapshot loads");
        let chilling_mist = data
            .aows
            .iter()
            .find(|aow| aow.aow_id == 227)
            .expect("Chilling Mist");
        assert_eq!(chilling_mist.scaling_status_add.frost, 0.0);
        for (role, expected) in [
            (AowEffectRole::PersistentWeaponBuff, 30.0),
            (AowEffectRole::PersistentOnHit, 60.0),
        ] {
            let effect = data
                .aow_effects(227, 0)
                .iter()
                .find(|effect| effect.role == role && effect.is_canonical == Some(true))
                .unwrap();
            assert_eq!(effect.status_buildup.frost, expected);
            assert!(!effect.is_supported);
            assert!(effect.reason.contains("overlap"));
        }
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
        let data = load_embedded_game_profile(VANILLA_PROFILE_ID).expect("embedded snapshot loads");
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
        let data = load_embedded_game_profile(VANILLA_PROFILE_ID).expect("embedded snapshot loads");
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
