#!/usr/bin/env python3

from __future__ import annotations

import argparse
import csv
import math
import re
import shutil
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Iterable, Iterator

ROW_MARKER = '<row id="'
ATTR_RE = re.compile(r'(\w+)="([^"]*)"')

WEAPON_PARAM = "EquipParamWeapon.param"
REINFORCE_PARAM = "ReinforceParamWeapon.param"
CALC_CORRECT_PARAM = "CalcCorrectGraph.param"
ATTACK_ELEMENT_PARAM = "AttackElementCorrectParam.param"
AOW_PARAM = "EquipParamGem.param"
SPEFFECT_PARAM = "SpEffectParam.param"

DAMAGE_INFOS = (
    ("physical", "Physics", "attackBasePhysics", "correctType_Physics"),
    ("magic", "Magic", "attackBaseMagic", "correctType_Magic"),
    ("fire", "Fire", "attackBaseFire", "correctType_Fire"),
    ("lightning", "Thunder", "attackBaseThunder", "correctType_Thunder"),
    ("holy", "Dark", "attackBaseDark", "correctType_Dark"),
)

STAT_AEC_PREFIX = {
    "str": "Strength",
    "dex": "Dexterity",
    "int": "Magic",
    "fai": "Faith",
    "arc": "Luck",
}

AFFINITY_PREFIXES = (
    "Flame Art",
    "Lightning",
    "Quality",
    "Occult",
    "Sacred",
    "Poison",
    "Heavy",
    "Magic",
    "Blood",
    "Keen",
    "Cold",
    "Fire",
)

WEP_TYPE_TO_AOW_KEYS: dict[int, tuple[str, ...]] = {
    1: ("Dagger",),
    3: ("SwordNormal",),
    5: ("SwordLarge",),
    7: ("SwordGigantic",),
    9: ("SaberNormal",),
    11: ("SaberLarge",),
    13: ("katana",),
    14: ("SwordDoubleEdge",),
    15: ("SwordPierce",),
    16: ("RapierHeavy",),
    17: ("AxeNormal",),
    19: ("AxeLarge",),
    21: ("HammerNormal",),
    23: ("HammerLarge",),
    24: ("Flail",),
    25: ("SpearNormal",),
    28: ("SpearHeavy",),
    29: ("SpearAxe",),
    31: ("Sickle",),
    35: ("Knuckle",),
    37: ("Claw",),
    39: ("Whip",),
    41: ("AxhammerLarge",),
    50: ("BowSmall",),
    51: ("BowNormal",),
    53: ("BowLarge",),
    55: ("ClossBow",),
    56: ("Ballista",),
    57: ("Staff", "Sorcery"),
    61: ("Talisman",),
    65: ("ShieldSmall",),
    67: ("ShieldNormal",),
    69: ("ShieldLarge",),
    87: ("Torch",),
    88: ("HandToHand",),
    89: ("PerfumeBottle",),
    90: ("ThrustingShield",),
    91: ("ThrowingWeapon",),
    92: ("ReverseHandSword",),
    93: ("LightGreatsword",),
    94: ("GreatKatana",),
    95: ("BeastClaw",),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Dump Phase 1 CSV data from regulation.bin.")
    parser.add_argument(
        "--regulation",
        type=Path,
        default=Path("data") / "raw" / "regulation.bin",
        help="Path to regulation.bin",
    )
    parser.add_argument(
        "--witchybnd",
        type=Path,
        default=Path("tools") / "phase1" / "_external" / "WitchyBND.exe",
        help="Path to WitchyBND.exe (not bundled in this repo)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("data") / "phase1",
        help="Output directory for CSV files",
    )
    parser.add_argument(
        "--workdir",
        type=Path,
        default=Path("data") / "_work_phase1",
        help="Temporary working directory",
    )
    parser.add_argument(
        "--keep-workdir",
        action="store_true",
        help="Keep working files after completion",
    )
    return parser.parse_args()


def ps_quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def run_witchybnd(witchybnd_path: Path, target_path: Path) -> None:
    script = (
        "$ErrorActionPreference='Stop'; "
        f"$p = Start-Process -FilePath {ps_quote(str(witchybnd_path))} "
        f"-ArgumentList '-s',{ps_quote(str(target_path))} -PassThru -Wait; "
        "exit $p.ExitCode"
    )
    result = subprocess.run(
        ["powershell", "-NoProfile", "-Command", script],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        stderr = result.stderr.strip() if result.stderr else "(no stderr)"
        raise RuntimeError(f"WitchyBND failed for {target_path}: {stderr}")


def unpack_regulation(regulation_path: Path, witchybnd_path: Path, workdir: Path) -> Path:
    if workdir.exists():
        shutil.rmtree(workdir)
    workdir.mkdir(parents=True, exist_ok=True)

    regulation_copy = workdir / "regulation.bin"
    shutil.copy2(regulation_path, regulation_copy)
    run_witchybnd(witchybnd_path, regulation_copy)

    unpacked = next(
        (item for item in workdir.iterdir() if item.is_dir() and "regulation" in item.name.lower()),
        None,
    )
    if unpacked is None:
        raise RuntimeError("Could not find unpacked regulation folder.")
    return unpacked


def serialize_param(unpacked_dir: Path, witchybnd_path: Path, param_name: str) -> Path:
    param_path = unpacked_dir / param_name
    if not param_path.exists():
        raise FileNotFoundError(f"Missing param: {param_name}")
    run_witchybnd(witchybnd_path, param_path)
    xml_path = Path(f"{param_path}.xml")
    if not xml_path.exists():
        raise FileNotFoundError(f"Missing serialized XML: {xml_path}")
    return xml_path


def iter_param_rows(xml_path: Path) -> Iterator[dict[str, str]]:
    with xml_path.open("r", encoding="utf-8") as handle:
        buffer = ""
        capturing = False

        for raw_line in handle:
            line = raw_line.strip()

            if not capturing:
                if ROW_MARKER not in line:
                    continue
                buffer = line
                capturing = True
            else:
                buffer = f"{buffer} {line}"

            if "/>" not in line:
                continue

            attrs = dict(ATTR_RE.findall(buffer))
            if "id" in attrs:
                yield attrs
            buffer = ""
            capturing = False


def to_int(attrs: dict[str, str], key: str, default: int = 0) -> int:
    raw = attrs.get(key)
    if raw is None or raw == "":
        return default
    return int(float(raw))


def to_float(attrs: dict[str, str], key: str, default: float = 0.0) -> float:
    raw = attrs.get(key)
    if raw is None or raw == "":
        return default
    return float(raw)


def normalize_percent(value: float) -> float:
    return value / 100.0


def format_float(value: float) -> str:
    text = f"{value:.8f}".rstrip("0").rstrip(".")
    if text == "-0":
        return "0"
    return text or "0"


def write_csv(path: Path, fieldnames: list[str], rows: Iterable[dict[str, object]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            out: dict[str, str] = {}
            for field in fieldnames:
                value = row.get(field, "")
                if isinstance(value, float):
                    out[field] = format_float(value)
                else:
                    out[field] = str(value)
            writer.writerow(out)


def load_weapon_name_map(witchybnd_path: Path) -> dict[int, str]:
    names_path = witchybnd_path.parent / "Assets" / "Paramdex" / "ER" / "Names" / "EquipParamWeapon.txt"
    if not names_path.exists():
        return {}

    mapping: dict[int, str] = {}
    with names_path.open("r", encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if not line:
                continue
            parts = line.split(" ", 1)
            if len(parts) != 2:
                continue
            try:
                key = int(parts[0])
            except ValueError:
                continue
            mapping[key] = parts[1].strip()
    return mapping


def load_wep_type_name_map(witchybnd_path: Path) -> dict[int, str]:
    meta_path = witchybnd_path.parent / "Assets" / "Paramdex" / "ER" / "Meta" / "EquipParamWeapon.xml"
    if not meta_path.exists():
        return {}

    text = meta_path.read_text(encoding="utf-8")
    enum_match = re.search(r'<Enum Name="WEP_TYPE" type="u16">(.*?)</Enum>', text, re.DOTALL)
    if enum_match is None:
        return {}

    mapping: dict[int, str] = {}
    for value, name in re.findall(r'<Option Value="(\d+)" Name="([^"]+)"\s*/>', enum_match.group(1)):
        mapping[int(value)] = name
    return mapping


def detect_affinity_prefix(name: str) -> str | None:
    for prefix in AFFINITY_PREFIXES:
        if name.startswith(f"{prefix} "):
            return prefix
    return None


def build_reinforce_affinity_map(weapon_rows: list[dict[str, str]]) -> dict[int, str]:
    total_by_type: Counter[int] = Counter()
    prefix_counts: dict[int, Counter[str]] = defaultdict(Counter)

    for row in weapon_rows:
        weapon_id = to_int(row, "id")
        if weapon_id % 100 != 0:
            continue
        name = row.get("paramdexName", "").strip()
        if not name:
            continue
        reinforce_type = to_int(row, "reinforceTypeId")
        total_by_type[reinforce_type] += 1
        prefix = detect_affinity_prefix(name)
        if prefix is not None:
            prefix_counts[reinforce_type][prefix] += 1

    affinity_by_type: dict[int, str] = {}
    for reinforce_type, total in total_by_type.items():
        if total < 5 or reinforce_type not in prefix_counts:
            affinity_by_type[reinforce_type] = "Standard"
            continue
        prefix, count = prefix_counts[reinforce_type].most_common(1)[0]
        if count / total >= 0.60:
            affinity_by_type[reinforce_type] = prefix
        else:
            affinity_by_type[reinforce_type] = "Standard"
    return affinity_by_type


def expand_calc_correct_curve(curve: dict[str, str]) -> list[float]:
    stage_vals = [to_float(curve, f"stageMaxVal{i}") for i in range(5)]
    stage_grow_vals = [to_float(curve, f"stageMaxGrowVal{i}") for i in range(5)]
    exponents = [to_float(curve, f"adjPt_maxGrowVal{i}") for i in range(5)]

    multipliers = [0.0] * 100
    for x in range(1, 100):
        segment = None
        for idx in range(4):
            if stage_vals[idx] <= x <= stage_vals[idx + 1]:
                segment = idx
                break
        if segment is None:
            segment = 0 if x < stage_vals[0] else 3

        left_x = stage_vals[segment]
        right_x = stage_vals[segment + 1]
        left_g = stage_grow_vals[segment]
        right_g = stage_grow_vals[segment + 1]
        exponent = exponents[segment]

        if right_x == left_x:
            ratio = 0.0
        else:
            ratio = (x - left_x) / (right_x - left_x)

        ratio = max(0.0, min(1.0, ratio))

        try:
            if exponent > 0.0:
                ratio_curve = ratio ** exponent
            elif exponent < 0.0:
                ratio_curve = 1.0 - (1.0 - ratio) ** (-exponent)
            else:
                ratio_curve = ratio
        except ZeroDivisionError:
            ratio_curve = 0.0
        if not math.isfinite(ratio_curve):
            ratio_curve = 0.0

        growth = left_g + (right_g - left_g) * ratio_curve
        multipliers[x] = growth / 100.0

    return multipliers


def derive_damage_curve_ids(weapon: dict[str, str]) -> dict[str, int]:
    return {
        "physical": to_int(weapon, "correctType_Physics"),
        "magic": to_int(weapon, "correctType_Magic"),
        "fire": to_int(weapon, "correctType_Fire"),
        "lightning": to_int(weapon, "correctType_Thunder"),
        "holy": to_int(weapon, "correctType_Dark"),
    }


def build_reinforce_rows(
    reinforce_rows: list[dict[str, str]],
) -> tuple[list[dict[str, object]], dict[int, int]]:
    rows_out: list[dict[str, object]] = []
    max_level_by_type: dict[int, int] = {}

    for row in reinforce_rows:
        row_id = to_int(row, "id")
        level = row_id % 100
        reinforce_type = row_id - level

        max_level_by_type[reinforce_type] = max(max_level_by_type.get(reinforce_type, 0), level)
        rows_out.append(
            {
                "reinforce_type": reinforce_type,
                "level": level,
                "physical_damage_mult": to_float(row, "physicsAtkRate", 1.0),
                "magic_damage_mult": to_float(row, "magicAtkRate", 1.0),
                "fire_damage_mult": to_float(row, "fireAtkRate", 1.0),
                "lightning_damage_mult": to_float(row, "thunderAtkRate", 1.0),
                "holy_damage_mult": to_float(row, "darkAtkRate", 1.0),
                "str_scaling_mult": to_float(row, "correctStrengthRate", 1.0),
                "dex_scaling_mult": to_float(row, "correctAgilityRate", 1.0),
                "int_scaling_mult": to_float(row, "correctMagicRate", 1.0),
                "fai_scaling_mult": to_float(row, "correctFaithRate", 1.0),
                "arc_scaling_mult": to_float(row, "correctLuckRate", 1.0),
            }
        )

    rows_out.sort(key=lambda item: (int(item["reinforce_type"]), int(item["level"])))
    return rows_out, max_level_by_type


def build_attack_element_rows(
    attack_rows: list[dict[str, str]],
) -> tuple[list[dict[str, object]], dict[int, dict[str, str]]]:
    rows_out: list[dict[str, object]] = []
    attack_map: dict[int, dict[str, str]] = {}

    for row in attack_rows:
        row_id = to_int(row, "id")
        attack_map[row_id] = row
        out_row: dict[str, object] = {"attack_element_correct_id": row_id}
        for stat_key, aec_prefix in STAT_AEC_PREFIX.items():
            for damage_name, damage_suffix, _, _ in DAMAGE_INFOS:
                field = f"is{aec_prefix}Correct_by{damage_suffix}"
                out_row[f"{stat_key}_scales_{damage_name}"] = to_int(row, field, 0)
        rows_out.append(out_row)

    rows_out.sort(key=lambda item: int(item["attack_element_correct_id"]))
    return rows_out, attack_map


def build_weapon_rows(
    weapon_rows: list[dict[str, str]],
    name_map: dict[int, str],
    wep_type_name_map: dict[int, str],
    affinity_by_type: dict[int, str],
    attack_map: dict[int, dict[str, str]],
    max_level_by_type: dict[int, int],
) -> list[dict[str, object]]:
    rows_out: list[dict[str, object]] = []

    for row in weapon_rows:
        weapon_id = to_int(row, "id")
        if weapon_id % 100 != 0:
            continue
        if to_int(row, "originEquipWep", -1) < 0:
            continue

        raw_name = row.get("paramdexName", "").strip()
        if not raw_name:
            raw_name = name_map.get(weapon_id, "").strip()
        if not raw_name:
            continue

        reinforce_type = to_int(row, "reinforceTypeId")
        affinity = affinity_by_type.get(reinforce_type, "Standard")
        name = raw_name
        if affinity != "Standard" and raw_name.startswith(f"{affinity} "):
            name = raw_name[len(affinity) + 1 :]

        attack_element_correct_id = to_int(row, "attackElementCorrectId")
        aec = attack_map.get(attack_element_correct_id, {})
        damage_curve_ids = derive_damage_curve_ids(row)
        is_somber = 1 if max_level_by_type.get(reinforce_type, 25) <= 10 else 0
        weapon_type_id = to_int(row, "wepType", 0)
        weapon_type_name = wep_type_name_map.get(weapon_type_id, "Unknown")
        weapon_type_keys = WEP_TYPE_TO_AOW_KEYS.get(weapon_type_id, ())

        base_physical = to_int(row, "attackBasePhysics", 0)
        base_magic = to_int(row, "attackBaseMagic", 0)
        base_fire = to_int(row, "attackBaseFire", 0)
        base_lightning = to_int(row, "attackBaseThunder", 0)
        base_holy = to_int(row, "attackBaseDark", 0)
        if (base_physical + base_magic + base_fire + base_lightning + base_holy) == 0:
            continue

        rows_out.append(
            {
                "weapon_id": weapon_id,
                "name": name,
                "affinity": affinity,
                "weapon_type_id": weapon_type_id,
                "weapon_type_name": weapon_type_name,
                "weapon_type_keys": "|".join(weapon_type_keys),
                "base_physical": base_physical,
                "base_magic": base_magic,
                "base_fire": base_fire,
                "base_lightning": base_lightning,
                "base_holy": base_holy,
                "str_scaling": normalize_percent(to_float(row, "correctStrength", 0.0)),
                "dex_scaling": normalize_percent(to_float(row, "correctAgility", 0.0)),
                "int_scaling": normalize_percent(to_float(row, "correctMagic", 0.0)),
                "fai_scaling": normalize_percent(to_float(row, "correctFaith", 0.0)),
                "arc_scaling": normalize_percent(to_float(row, "correctLuck", 0.0)),
                "req_str": to_int(row, "properStrength", 0),
                "req_dex": to_int(row, "properAgility", 0),
                "req_int": to_int(row, "properMagic", 0),
                "req_fai": to_int(row, "properFaith", 0),
                "req_arc": to_int(row, "properLuck", 0),
                "reinforce_type": reinforce_type,
                "attack_element_correct_id": attack_element_correct_id,
                "curve_id_physical": damage_curve_ids["physical"],
                "curve_id_magic": damage_curve_ids["magic"],
                "curve_id_fire": damage_curve_ids["fire"],
                "curve_id_lightning": damage_curve_ids["lightning"],
                "curve_id_holy": damage_curve_ids["holy"],
                "is_somber": is_somber,
            }
        )

    rows_out.sort(key=lambda item: int(item["weapon_id"]))
    return rows_out


def build_calc_correct_rows(curve_rows: list[dict[str, str]]) -> list[dict[str, object]]:
    rows_out: list[dict[str, object]] = []
    for row in curve_rows:
        curve_id = to_int(row, "id")
        expanded = expand_calc_correct_curve(row)
        for stat_value, multiplier in enumerate(expanded):
            rows_out.append(
                {
                    "curve_id": curve_id,
                    "stat_value": stat_value,
                    "multiplier": multiplier,
                }
            )
    rows_out.sort(key=lambda item: (int(item["curve_id"]), int(item["stat_value"])))
    return rows_out


def build_speffect_map(sp_rows: list[dict[str, str]]) -> dict[int, tuple[float, float, float]]:
    effect_map: dict[int, tuple[float, float, float]] = {}
    for row in sp_rows:
        effect_id = to_int(row, "id")
        effect_map[effect_id] = (
            to_float(row, "bloodAttackPower", 0.0),
            to_float(row, "freezeAttackPower", 0.0),
            to_float(row, "diseaseAttackPower", 0.0),
        )
    return effect_map


def build_aow_rows(
    aow_rows: list[dict[str, str]],
    effect_map: dict[int, tuple[float, float, float]],
) -> list[dict[str, object]]:
    grouped_rows: dict[int, list[dict[str, str]]] = defaultdict(list)

    for row in aow_rows:
        raw_name = row.get("paramdexName", "").strip()
        if not raw_name.startswith("Ash of War:"):
            continue

        sword_art_id = to_int(row, "swordArtsParamId", -1)
        if sword_art_id < 0:
            continue
        grouped_rows[sword_art_id].append(row)

    rows_out: list[dict[str, object]] = []
    for sword_art_id, rows in grouped_rows.items():
        def score(item: dict[str, str]) -> tuple[int, int, int, int]:
            sort_real = 1 if item.get("sortId") not in (None, "", "999999") else 0
            icon_real = 1 if item.get("iconId") not in (None, "", "0") else 0
            special = 1 if to_int(item, "isSpecialSwordArt", 0) != 0 else 0
            return (sort_real, icon_real, special, to_int(item, "id", 0))

        canonical = max(rows, key=score)
        aow_name = canonical.get("paramdexName", "").replace("Ash of War:", "", 1).strip()
        if not aow_name:
            continue

        # Ignore attack-hit effects; keep only passive AoW effects for build scoring.
        effect_ids: set[int] = set()
        for field in ("spEffectId0", "spEffectId1"):
            effect_id = to_int(canonical, field, -1)
            if effect_id > 0:
                effect_ids.add(effect_id)

        bleed = 0.0
        frost = 0.0
        poison = 0.0
        for effect_id in effect_ids:
            effect_bleed, effect_frost, effect_poison = effect_map.get(effect_id, (0.0, 0.0, 0.0))
            bleed += effect_bleed
            frost += effect_frost
            poison += effect_poison

        valid_weapon_types: set[str] = set()
        for key, value in canonical.items():
            if key.startswith("canMountWep_") and to_int(canonical, key, 0) != 0:
                valid_weapon_types.add(key.replace("canMountWep_", ""))

        rows_out.append(
            {
                "aow_id": sword_art_id,
                "name": aow_name,
                "bleed_buildup_add": bleed,
                "frost_buildup_add": frost,
                "poison_buildup_add": poison,
                "valid_weapon_types": "|".join(sorted(valid_weapon_types)),
            }
        )

    rows_out.sort(key=lambda item: int(item["aow_id"]))
    return rows_out


def main() -> int:
    args = parse_args()
    regulation_path: Path = args.regulation.resolve()
    witchybnd_path: Path = args.witchybnd.resolve()
    output_dir: Path = args.output.resolve()
    workdir: Path = args.workdir.resolve()

    if not regulation_path.exists():
        raise FileNotFoundError(f"regulation.bin not found: {regulation_path}")
    if not witchybnd_path.exists():
        raise FileNotFoundError(
            f"WitchyBND.exe not found: {witchybnd_path} (pass --witchybnd <path>)"
        )

    unpacked_dir = unpack_regulation(regulation_path, witchybnd_path, workdir)
    xml_paths = {
        WEAPON_PARAM: serialize_param(unpacked_dir, witchybnd_path, WEAPON_PARAM),
        REINFORCE_PARAM: serialize_param(unpacked_dir, witchybnd_path, REINFORCE_PARAM),
        CALC_CORRECT_PARAM: serialize_param(unpacked_dir, witchybnd_path, CALC_CORRECT_PARAM),
        ATTACK_ELEMENT_PARAM: serialize_param(unpacked_dir, witchybnd_path, ATTACK_ELEMENT_PARAM),
        AOW_PARAM: serialize_param(unpacked_dir, witchybnd_path, AOW_PARAM),
        SPEFFECT_PARAM: serialize_param(unpacked_dir, witchybnd_path, SPEFFECT_PARAM),
    }

    weapon_rows = list(iter_param_rows(xml_paths[WEAPON_PARAM]))
    reinforce_rows = list(iter_param_rows(xml_paths[REINFORCE_PARAM]))
    curve_rows = list(iter_param_rows(xml_paths[CALC_CORRECT_PARAM]))
    attack_rows = list(iter_param_rows(xml_paths[ATTACK_ELEMENT_PARAM]))
    aow_rows = list(iter_param_rows(xml_paths[AOW_PARAM]))
    sp_rows = list(iter_param_rows(xml_paths[SPEFFECT_PARAM]))

    weapon_name_map = load_weapon_name_map(witchybnd_path)
    wep_type_name_map = load_wep_type_name_map(witchybnd_path)
    affinity_by_type = build_reinforce_affinity_map(weapon_rows)
    reinforce_csv_rows, max_level_by_type = build_reinforce_rows(reinforce_rows)
    attack_csv_rows, attack_map = build_attack_element_rows(attack_rows)
    weapon_csv_rows = build_weapon_rows(
        weapon_rows,
        weapon_name_map,
        wep_type_name_map,
        affinity_by_type,
        attack_map,
        max_level_by_type,
    )
    calc_correct_csv_rows = build_calc_correct_rows(curve_rows)
    sp_effect_map = build_speffect_map(sp_rows)
    aow_csv_rows = build_aow_rows(aow_rows, sp_effect_map)

    write_csv(
        output_dir / "weapons.csv",
        [
            "weapon_id",
            "name",
            "affinity",
            "weapon_type_id",
            "weapon_type_name",
            "weapon_type_keys",
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
            "reinforce_type",
            "attack_element_correct_id",
            "curve_id_physical",
            "curve_id_magic",
            "curve_id_fire",
            "curve_id_lightning",
            "curve_id_holy",
            "is_somber",
        ],
        weapon_csv_rows,
    )
    write_csv(
        output_dir / "reinforce.csv",
        [
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
        ],
        reinforce_csv_rows,
    )
    write_csv(
        output_dir / "calc_correct.csv",
        ["curve_id", "stat_value", "multiplier"],
        calc_correct_csv_rows,
    )
    write_csv(
        output_dir / "attack_element_correct.csv",
        [
            "attack_element_correct_id",
            "str_scales_physical",
            "str_scales_magic",
            "str_scales_fire",
            "str_scales_lightning",
            "str_scales_holy",
            "dex_scales_physical",
            "dex_scales_magic",
            "dex_scales_fire",
            "dex_scales_lightning",
            "dex_scales_holy",
            "int_scales_physical",
            "int_scales_magic",
            "int_scales_fire",
            "int_scales_lightning",
            "int_scales_holy",
            "fai_scales_physical",
            "fai_scales_magic",
            "fai_scales_fire",
            "fai_scales_lightning",
            "fai_scales_holy",
            "arc_scales_physical",
            "arc_scales_magic",
            "arc_scales_fire",
            "arc_scales_lightning",
            "arc_scales_holy",
        ],
        attack_csv_rows,
    )
    write_csv(
        output_dir / "aow.csv",
        [
            "aow_id",
            "name",
            "bleed_buildup_add",
            "frost_buildup_add",
            "poison_buildup_add",
            "valid_weapon_types",
        ],
        aow_csv_rows,
    )

    if not args.keep_workdir:
        shutil.rmtree(workdir, ignore_errors=True)

    print(f"Wrote CSVs to {output_dir}")
    print(f"  weapons.csv rows: {len(weapon_csv_rows)}")
    print(f"  reinforce.csv rows: {len(reinforce_csv_rows)}")
    print(f"  calc_correct.csv rows: {len(calc_correct_csv_rows)}")
    print(f"  attack_element_correct.csv rows: {len(attack_csv_rows)}")
    print(f"  aow.csv rows: {len(aow_csv_rows)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        raise SystemExit(1)
