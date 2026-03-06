use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{
    AttackElementCorrect, AttackElementCorrectExt, Aow, AowAttackRow, GameData, ReinforceLevel,
    StatusBuildup, Weapon,
    COMBAT_STAT_COUNT, DAMAGE_TYPE_COUNT,
};

struct CsvTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl CsvTable {
    fn from_path(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|err| format!("failed reading {}: {err}", path.display()))?;
        let mut lines = content.lines();
        let header_line = lines
            .next()
            .ok_or_else(|| format!("{} is empty", path.display()))?;
        let headers = split_csv_line(header_line);
        if headers.is_empty() {
            return Err(format!("{} has no headers", path.display()));
        }

        let mut rows = Vec::new();
        for (line_idx, line) in lines.enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let mut row = split_csv_line(line);
            if row.len() < headers.len() {
                row.resize(headers.len(), String::new());
            }
            if row.len() != headers.len() {
                return Err(format!(
                    "{} line {} has {} columns, expected {}",
                    path.display(),
                    line_idx + 2,
                    row.len(),
                    headers.len()
                ));
            }
            rows.push(row);
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

fn split_csv_line(line: &str) -> Vec<String> {
    line.split(',').map(|part| part.trim().to_string()).collect()
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

pub fn load_game_data(data_dir: impl AsRef<Path>) -> Result<GameData, String> {
    let data_dir = data_dir.as_ref();
    let weapons = load_weapons(data_dir.join("weapons.csv"))?;
    let reinforce = load_reinforce(data_dir.join("reinforce.csv"))?;
    let calc_correct = load_calc_correct(data_dir.join("calc_correct.csv"))?;
    let attack_element_correct =
        load_attack_element_correct(data_dir.join("attack_element_correct.csv"))?;
    let attack_element_correct_ext =
        load_attack_element_correct_ext_optional(data_dir.join("attack_element_correct_ext.csv"))?;
    let aows = load_aows(data_dir.join("aow.csv"))?;
    let aow_attack_rows = load_aow_attack_rows_optional(data_dir.join("aow_attack_data.csv"))?;
    let weapon_passives = load_weapon_passives_optional(data_dir.join("weapon_passives.csv"))?;
    let exact_aow_compat = load_exact_aow_compat_optional(data_dir.join("aow_weapon_compat.csv"))?;

    Ok(GameData {
        weapons,
        reinforce,
        calc_correct,
        attack_element_correct,
        attack_element_correct_ext,
        aows,
        aow_attack_rows,
        weapon_passives,
        exact_aow_compat,
    })
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
            weapon_type_id: parse_u16(table.get(row, "weapon_type_id")?, "weapon_type_id")?,
            weapon_type_name: table.get(row, "weapon_type_name")?.to_string(),
            weapon_type_keys: table.get(row, "weapon_type_keys")?.to_string(),
            base,
            scaling,
            requirements,
            reinforce_type: parse_u16(table.get(row, "reinforce_type")?, "reinforce_type")?,
            attack_element_correct_id: parse_usize(
                table.get(row, "attack_element_correct_id")?,
                "attack_element_correct_id",
            )?,
            damage_curve_ids,
            is_somber: parse_bool_u8(table.get(row, "is_somber")?, "is_somber")?,
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
            parse_f32(table.get(row, "physical_damage_mult")?, "physical_damage_mult")?,
            parse_f32(table.get(row, "magic_damage_mult")?, "magic_damage_mult")?,
            parse_f32(table.get(row, "fire_damage_mult")?, "fire_damage_mult")?,
            parse_f32(table.get(row, "lightning_damage_mult")?, "lightning_damage_mult")?,
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
            },
        ));
    }

    let mut reinforce = vec![Vec::<Option<ReinforceLevel>>::new(); max_type + 1];
    for (reinforce_type, max_level) in &max_level_by_type {
        reinforce[*reinforce_type] = vec![None; *max_level + 1];
    }
    for (reinforce_type, level, value) in entries {
        if let Some(levels) = reinforce.get_mut(reinforce_type) {
            if level < levels.len() {
                levels[level] = Some(value);
            }
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
        if stat_value > 99 {
            return Err(format!("calc_correct stat_value out of range: {stat_value}"));
        }
        let multiplier = parse_f32(table.get(row, "multiplier")?, "multiplier")?;
        max_curve_id = max_curve_id.max(curve_id);
        entries.push((curve_id, stat_value, multiplier));
    }

    let mut out = vec![vec![0.0_f32; 100]; max_curve_id + 1];
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
        let row_id =
            parse_usize(table.get(row, "attack_element_correct_id")?, "attack_element_correct_id")?;
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

fn load_aows(path: PathBuf) -> Result<Vec<Aow>, String> {
    let table = CsvTable::from_path(&path)?;
    let mut out = Vec::with_capacity(table.rows.len());

    for row in &table.rows {
        out.push(Aow {
            aow_id: parse_u16(table.get(row, "aow_id")?, "aow_id")?,
            name: table.get(row, "name")?.to_string(),
            bleed_buildup_add: parse_f32(table.get(row, "bleed_buildup_add")?, "bleed_buildup_add")?,
            frost_buildup_add: parse_f32(table.get(row, "frost_buildup_add")?, "frost_buildup_add")?,
            poison_buildup_add: parse_f32(
                table.get(row, "poison_buildup_add")?,
                "poison_buildup_add",
            )?,
            valid_weapon_types: table.get(row, "valid_weapon_types")?.to_string(),
        });
    }
    Ok(out)
}

fn load_attack_element_correct_ext_optional(
    path: PathBuf,
) -> Result<HashMap<usize, AttackElementCorrectExt>, String> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let table = CsvTable::from_path(&path)?;
    let mut out = HashMap::with_capacity(table.rows.len());
    for row in &table.rows {
        let row_id =
            parse_usize(table.get(row, "attack_element_correct_id")?, "attack_element_correct_id")?;
        let mut scales = [[false; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        let mut overwrite = [[None; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        let mut influence = [[100.0_f32; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT];
        for (stat_idx, stat_key) in ["str", "dex", "int", "fai", "arc"].iter().enumerate() {
            for (damage_idx, damage_key) in
                ["physical", "magic", "fire", "lightning", "holy"].iter().enumerate()
            {
                let scale_field = format!("{stat_key}_scales_{damage_key}");
                let overwrite_field = format!("{stat_key}_overwrite_{damage_key}");
                let influence_field = format!("{stat_key}_influence_{damage_key}");
                scales[stat_idx][damage_idx] =
                    parse_bool_u8(table.get(row, &scale_field)?, &scale_field)?;
                let overwrite_value = parse_f32(table.get(row, &overwrite_field)?, &overwrite_field)?;
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
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let table = CsvTable::from_path(&path)?;
    let mut out: HashMap<u16, Vec<AowAttackRow>> = HashMap::new();
    for row in &table.rows {
        let aow_id = parse_u16(table.get(row, "aow_id")?, "aow_id")?;
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
        out.entry(aow_id).or_default().push(AowAttackRow {
            sheet_row: parse_u16(table.get(row, "sheet_row")?, "sheet_row")?,
            aow_id,
            aow_name: table.get(row, "aow_name")?.to_string(),
            raw_name: table.get(row, "raw_name")?.to_string(),
            variant_weapon_type: table.get(row, "variant_weapon_type")?.to_string(),
            is_lacking_fp: parse_bool_u8(table.get(row, "is_lacking_fp")?, "is_lacking_fp")?,
            atk_id: parse_u32(table.get(row, "atk_id")?, "atk_id")?,
            overwrite_attack_element_correct_id: (overwrite_raw > 0).then_some(overwrite_raw as usize),
            is_disable_both_hands_bonus: parse_bool_u8(
                table.get(row, "is_disable_both_hands_bonus")?,
                "is_disable_both_hands_bonus",
            )?,
            is_add_base_atk: parse_bool_u8(table.get(row, "is_add_base_atk")?, "is_add_base_atk")?,
            motion_values: [
                parse_f32(table.get(row, "physical_mv")?, "physical_mv")?,
                parse_f32(table.get(row, "magic_mv")?, "magic_mv")?,
                parse_f32(table.get(row, "fire_mv")?, "fire_mv")?,
                parse_f32(table.get(row, "lightning_mv")?, "lightning_mv")?,
                parse_f32(table.get(row, "holy_mv")?, "holy_mv")?,
            ],
            attack_base: [
                parse_f32(table.get(row, "attack_base_physical")?, "attack_base_physical")?,
                parse_f32(table.get(row, "attack_base_magic")?, "attack_base_magic")?,
                parse_f32(table.get(row, "attack_base_fire")?, "attack_base_fire")?,
                parse_f32(table.get(row, "attack_base_lightning")?, "attack_base_lightning")?,
                parse_f32(table.get(row, "attack_base_holy")?, "attack_base_holy")?,
            ],
            status_mv: parse_f32(table.get(row, "status_mv")?, "status_mv")?,
            weapon_buff_mv: parse_f32(table.get(row, "weapon_buff_mv")?, "weapon_buff_mv")?,
            stamina_cost: parse_f32(table.get(row, "stamina_cost")?, "stamina_cost")?,
        });
    }

    for rows in out.values_mut() {
        rows.sort_by_key(|row| row.sheet_row);
    }
    Ok(out)
}

fn load_weapon_passives_optional(path: PathBuf) -> Result<HashMap<u32, StatusBuildup>, String> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let table = CsvTable::from_path(&path)?;
    let mut out = HashMap::with_capacity(table.rows.len());
    for row in &table.rows {
        let weapon_id = parse_u32(table.get(row, "weapon_id")?, "weapon_id")?;
        out.insert(
            weapon_id,
            StatusBuildup {
                bleed: parse_f32(table.get(row, "bleed")?, "bleed")?,
                frost: parse_f32(table.get(row, "frost")?, "frost")?,
                poison: parse_f32(table.get(row, "poison")?, "poison")?,
                sleep: parse_f32(table.get(row, "sleep")?, "sleep")?,
                madness: parse_f32(table.get(row, "madness")?, "madness")?,
                death: parse_f32(table.get(row, "death")?, "death")?,
            },
        );
    }
    Ok(out)
}

fn load_exact_aow_compat_optional(path: PathBuf) -> Result<HashSet<(u16, u32)>, String> {
    if !path.exists() {
        return Ok(HashSet::new());
    }

    let table = CsvTable::from_path(&path)?;
    let mut out = HashSet::with_capacity(table.rows.len());
    for row in &table.rows {
        let aow_id = parse_u16(table.get(row, "aow_id")?, "aow_id")?;
        let weapon_id = parse_u32(table.get(row, "weapon_id")?, "weapon_id")?;
        out.insert((aow_id, weapon_id));
    }
    Ok(out)
}
