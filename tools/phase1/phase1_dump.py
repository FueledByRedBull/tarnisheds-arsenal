#!/usr/bin/env python3

from __future__ import annotations

import argparse
import csv
import json
import math
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from collections import defaultdict
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any, Iterable, Iterator, Mapping, Sequence

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.profiles import discover_witchybnd, profile_definition  # noqa: E402

MAX_DISPLAYED_STAT = 99
MAX_EFFECTIVE_STRENGTH = MAX_DISPLAYED_STAT * 3 // 2

WEAPON_PARAM = "EquipParamWeapon.param"
REINFORCE_PARAM = "ReinforceParamWeapon.param"
CALC_CORRECT_PARAM = "CalcCorrectGraph.param"
ATTACK_ELEMENT_PARAM = "AttackElementCorrectParam.param"
AOW_PARAM = "EquipParamGem.param"
SPEFFECT_PARAM = "SpEffectParam.param"
WITCHYBND_TIMEOUT_SECONDS = 600
# Paramdex definitions WitchyBND uses to serialize the params above.
PARAM_DEFINITIONS = (
    "EquipParamWeapon",
    "ReinforceParamWeapon",
    "CalcCorrectGraph",
    "AttackElementCorrectParam",
    "EquipParamGem",
    "SpEffect",
)
STATUS_ATTACK_POWER_FIELDS = {
    "bleed": "bloodAttackPower",
    "frost": "freezeAttackPower",
    "poison": "poizonAttackPower",
    "scarlet_rot": "diseaseAttackPower",
    "sleep": "sleepAttackPower",
    "madness": "madnessAttackPower",
    "death": "curseAttackPower",
}
PHYSICAL_ATTACK_FLAGS = (
    ("isBlowAttackType", "strike"),
    ("isSlashAttackType", "slash"),
    ("isThrustAttackType", "pierce"),
    ("isNormalAttackType", "standard"),
)
DAMAGE_CURVE_FIELDS = {
    "physical": "correctType_Physics",
    "magic": "correctType_Magic",
    "fire": "correctType_Fire",
    "lightning": "correctType_Thunder",
    "holy": "correctType_Dark",
    "poison": "correctType_Poison",
    "blood": "correctType_Blood",
    "sleep": "correctType_Sleep",
    "madness": "correctType_Madness",
}
# Weapon model fields fail extraction when absent; their declared XML defaults are
# materialized so omitted-at-default attributes stay valid.
WEAPON_MODEL_FIELDS = (
    "originEquipWep", "reinforceTypeId", "disableGemAttr", "gemMountType", "wepType",
    "attackElementCorrectId", "swordArtsParamId", "weight", "throwAtkRate", "isDualBlade",
    "staminaConsumptionRate",
    "attackBasePhysics", "attackBaseMagic", "attackBaseFire", "attackBaseThunder", "attackBaseDark",
    "correctStrength", "correctAgility", "correctMagic", "correctFaith", "correctLuck",
    "properStrength", "properAgility", "properMagic", "properFaith", "properLuck",
    *DAMAGE_CURVE_FIELDS.values(),
    *(field for field, _ in PHYSICAL_ATTACK_FLAGS),
)
WORKBOOK_POISE_FIELDS = (
    "base_poise",
    "move_count",
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
)

DAMAGE_INFOS = (
    ("physical", "Physics"),
    ("magic", "Magic"),
    ("fire", "Fire"),
    ("lightning", "Thunder"),
    ("holy", "Dark"),
)

STAT_AEC_PREFIX = {
    "str": "Strength",
    "dex": "Dexterity",
    "int": "Magic",
    "fai": "Faith",
    "arc": "Luck",
}

WEP_TYPE_KEY_ALIASES = {
    "straightsword": ("SwordNormal",),
    "greatsword": ("SwordLarge",),
    "colossalsword": ("SwordGigantic",),
    "curvedsword": ("SaberNormal",),
    "curvedgreatsword": ("SaberLarge",),
    "katana": ("katana",),
    "twinblade": ("SwordDoubleEdge",),
    "thrustingsword": ("SwordPierce",),
    "heavythrustingsword": ("RapierHeavy",),
    "axe": ("AxeNormal",),
    "greataxe": ("AxeLarge",),
    "hammer": ("HammerNormal",),
    "greathammer": ("HammerLarge",),
    "flail": ("Flail",),
    "spear": ("SpearNormal",),
    "heavyspear": ("SpearHeavy",),
    "halberd": ("SpearAxe",),
    "scythe": ("Sickle",),
    "fist": ("Knuckle",),
    "claw": ("Claw",),
    "whip": ("Whip",),
    "colossalweapon": ("AxhammerLarge",),
    "lightbow": ("BowSmall",),
    "bow": ("BowNormal",),
    "greatbow": ("BowLarge",),
    "crossbow": ("ClossBow",),
    "ballista": ("Ballista",),
    "staff": ("Staff", "Sorcery"),
    "seal": ("Talisman",),
    "smallshield": ("ShieldSmall",),
    "mediumshield": ("ShieldNormal",),
    "greatshield": ("ShieldLarge",),
    "torch": ("Torch",),
    "handtohand": ("HandToHand",),
    "perfumebottle": ("PerfumeBottle",),
    "thrustingshield": ("ThrustingShield",),
    "throwingblade": ("ThrowingWeapon",),
    "reversehandblade": ("ReverseHandSword",),
    "lightgreatsword": ("LightGreatsword",),
    "greatkatana": ("GreatKatana",),
    "beastclaw": ("BeastClaw",),
}


@dataclass(frozen=True)
class RegulationContext:
    workdir: Path
    weapon_rows: list[dict[str, str]]
    reinforce_rows: list[dict[str, str]]
    curve_rows: list[dict[str, str]]
    attack_rows: list[dict[str, str]]
    aow_rows: list[dict[str, str]]
    sp_rows: list[dict[str, str]]
    weapon_name_map: dict[int, str]
    sword_art_name_map: dict[int, str]
    wep_type_name_map: dict[int, str]
    weapon_type_keys_by_id: dict[int, tuple[str, ...]]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Dump Phase 1 CSV data from regulation.bin.")
    parser.add_argument(
        "--profile",
        default="vanilla",
        help="Game profile to extract (vanilla, convergence, or conv)",
    )
    parser.add_argument(
        "--regulation",
        type=Path,
        help="Path to regulation.bin (defaults to the selected profile source folder)",
    )
    parser.add_argument(
        "--witchybnd",
        type=Path,
        help="Path to WitchyBND.exe (auto-discovered under data/raw when omitted)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Output directory for the selected profile snapshot",
    )
    parser.add_argument(
        "--workdir",
        type=Path,
        help="Temporary working directory (profile-specific when omitted)",
    )
    parser.add_argument(
        "--weapon-name-xml",
        dest="weapon_name_xmls",
        action="append",
        type=Path,
        help="Unpacked WeaponName FMG XML override; repeat for base/DLC tables",
    )
    parser.add_argument(
        "--arts-name-xml",
        dest="arts_name_xmls",
        action="append",
        type=Path,
        help="Unpacked ArtsName FMG XML override; repeat for base/DLC tables",
    )
    parser.add_argument(
        "--profile-version-file",
        type=Path,
        help="Optional profile version provenance file override",
    )
    parser.add_argument(
        "--keep-workdir",
        action="store_true",
        help="Keep working files after completion",
    )
    return parser.parse_args()


def run_witchybnd(witchybnd_path: Path, target_path: Path) -> None:
    result = subprocess.run(
        [str(witchybnd_path), "-s", str(target_path)],
        capture_output=True,
        text=True,
        check=False,
        timeout=WITCHYBND_TIMEOUT_SECONDS,
        creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0,
    )
    if result.returncode != 0:
        stderr = result.stderr.strip() if result.stderr else "(no stderr)"
        raise RuntimeError(f"WitchyBND failed for {target_path}: {stderr}")


def unpack_regulation(regulation_path: Path, witchybnd_path: Path, workdir: Path) -> Path:
    workdir.mkdir(parents=True, exist_ok=True)
    workdir = Path(tempfile.mkdtemp(prefix="regulation-", dir=workdir))

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


def iter_param_rows(
    xml_path: Path, *, apply_defaults: bool = False, default_fields: tuple[str, ...] = ()
) -> Iterator[dict[str, str]]:
    defaults: dict[str, str] = {}
    for _event, element in ET.iterparse(xml_path, events=("end",)):
        tag = element.tag.rsplit("}", 1)[-1]
        if (
            tag == "field"
            and "defaultValue" in element.attrib
            and (apply_defaults or element.get("name") in default_fields)
        ):
            defaults[element.attrib["name"]] = element.attrib["defaultValue"]
        if tag == "row":
            to_int(element.attrib, "id", None)
            yield defaults | element.attrib
        element.clear()


def to_int(attrs: dict[str, str], key: str, default: int | None = 0) -> int:
    raw = attrs.get(key)
    if raw is None and default is not None:
        return default
    if raw is None:
        raise ValueError(f"row {attrs.get('id', 'unknown')}: missing {key}")
    try:
        value = Decimal(raw)
    except InvalidOperation as exc:
        raise ValueError(f"invalid integer {key}={raw!r}") from exc
    if not value.is_finite() or value != value.to_integral_value():
        raise ValueError(f"invalid integer {key}={raw!r}")
    return int(value)


def to_float(attrs: dict[str, str], key: str, default: float | None = 0.0) -> float:
    raw = attrs.get(key)
    if raw is None and default is not None:
        return default
    if raw is None:
        raise ValueError(f"row {attrs.get('id', 'unknown')}: missing {key}")
    value = float(raw)
    if not math.isfinite(value):
        raise ValueError(f"non-finite {key}={raw!r}")
    return value


def param_row_name(attrs: Mapping[str, str]) -> str:
    return (attrs.get("paramdexName") or attrs.get("name") or "").strip()


def object_to_int(value: object) -> int:
    if isinstance(value, (int, float, str)):
        return int(value)
    raise TypeError(f"expected an integer-compatible value, got {type(value).__name__}")


def normalize_percent(value: float) -> float:
    return value / 100.0


def format_float(value: float) -> str:
    text = f"{value:.8f}".rstrip("0").rstrip(".")
    if text == "-0":
        return "0"
    return text or "0"


def write_csv(path: Path, fieldnames: list[str], rows: Iterable[Mapping[str, object]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames, lineterminator="\n")
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


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def paramdex_path(witchybnd_path: Path, *parts: str) -> Path:
    return witchybnd_path.parent.joinpath("Assets", "Paramdex", "ER", *parts)


def load_param_name_map(witchybnd_path: Path, param_name: str) -> dict[int, str]:
    names_path = paramdex_path(witchybnd_path, "Names", f"{param_name}.txt")
    if not names_path.is_file():
        raise FileNotFoundError(f"Paramdex names file not found: {names_path}")

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
            value = parts[1].strip()
            if not value or value in {"%null%", "[ERROR]"}:
                continue
            mapping[key] = value
    if not mapping:
        raise ValueError(f"Paramdex names file contains no usable entries: {names_path}")
    return mapping


def load_weapon_name_map(witchybnd_path: Path) -> dict[int, str]:
    return load_param_name_map(witchybnd_path, "EquipParamWeapon")


def load_fmg_name_map(xml_path: Path) -> dict[int, str]:
    if not xml_path.is_file():
        raise FileNotFoundError(f"FMG name source not found: {xml_path}")
    root = ET.parse(xml_path).getroot()
    mapping: dict[int, str] = {}
    for element in root.findall("./entries/text"):
        raw_id = element.get("id")
        value = (element.text or "").strip()
        if raw_id is None or not value or value in {"%null%", "[ERROR]"}:
            continue
        try:
            entry_id = int(raw_id)
        except ValueError:
            continue
        mapping[entry_id] = value
    if not mapping:
        raise ValueError(f"FMG name source contains no usable entries: {xml_path}")
    return mapping


def load_merged_fmg_name_map(xml_paths: Sequence[Path], label: str) -> dict[int, str]:
    mapping: dict[int, str] = {}
    source_by_id: dict[int, Path] = {}
    for xml_path in xml_paths:
        for entry_id, value in load_fmg_name_map(xml_path).items():
            previous = mapping.get(entry_id)
            if previous is not None and previous != value:
                raise ValueError(
                    f"conflicting {label} FMG name for id={entry_id}: "
                    f"{previous!r} from {source_by_id[entry_id].name}, "
                    f"{value!r} from {xml_path.name}"
                )
            mapping[entry_id] = value
            source_by_id[entry_id] = xml_path
    if xml_paths and not mapping:
        raise ValueError(f"merged {label} FMG sources contain no usable names")
    return mapping


def load_wep_type_name_map(witchybnd_path: Path) -> dict[int, str]:
    meta_path = paramdex_path(witchybnd_path, "Meta", "EquipParamWeapon.xml")
    if not meta_path.is_file():
        raise FileNotFoundError(f"Paramdex weapon meta file not found: {meta_path}")
    mapping = {
        to_int(option.attrib, "Value", None): option.attrib["Name"]
        for enum in ET.parse(meta_path).getroot().iter("Enum")
        if enum.get("Name") == "WEP_TYPE"
        for option in enum.iter("Option")
    }
    if not mapping:
        raise ValueError(f"Paramdex weapon meta file has no WEP_TYPE options: {meta_path}")
    return mapping


def normalize_lookup_token(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "", value.lower())


def discover_gem_mount_fields(xml_path: Path) -> tuple[str, ...]:
    prefix = "canMountWep_"
    names = (
        element.get("name", "")
        for element in ET.parse(xml_path).getroot().iter()
        if element.tag.rsplit("}", 1)[-1] == "field"
    )
    fields = tuple(dict.fromkeys(name[len(prefix):] for name in names if name.startswith(prefix)))
    if not fields:
        raise ValueError(f"gem param XML declares no {prefix} fields: {xml_path}")
    return fields


def build_weapon_type_key_map(
    wep_type_name_map: dict[int, str],
    gem_mount_fields: tuple[str, ...],
) -> dict[int, tuple[str, ...]]:
    mount_field_set = set(gem_mount_fields)
    direct_by_norm: dict[str, list[str]] = defaultdict(list)
    for field_name in gem_mount_fields:
        direct_by_norm[normalize_lookup_token(field_name)].append(field_name)

    out: dict[int, tuple[str, ...]] = {}
    for weapon_type_id, weapon_type_name in wep_type_name_map.items():
        normalized_name = normalize_lookup_token(weapon_type_name)
        candidates = direct_by_norm.get(normalized_name, [])
        if not candidates:
            candidates = [
                field_name
                for field_name in WEP_TYPE_KEY_ALIASES.get(normalized_name, ())
                if field_name in mount_field_set
            ]
        out[weapon_type_id] = tuple(dict.fromkeys(candidates))
    return out


def weapon_series_id(weapon_id: int) -> int:
    return weapon_id - (weapon_id % 10000)


def weapon_affinity_slot(weapon_id: int) -> int:
    return (weapon_id % 10000) // 100


def player_weapon_param_name(row: dict[str, str]) -> str:
    name = param_row_name(row).strip()
    lowered = name.casefold()
    if not name or to_int(row, "sortId", 9_999_999) == 9_999_999:
        return ""
    if any(marker in lowered for marker in ("[npc]", "(npc)", "dummy", "test weapon")):
        return ""
    return re.sub(r"^\[conv\]\s*", "", name, flags=re.IGNORECASE)


def build_standard_name_map(
    weapon_rows: list[dict[str, str]],
    name_map: dict[int, str],
    name_overrides: Mapping[int, str],
) -> dict[int, str]:
    out: dict[int, str] = {}
    for row in weapon_rows:
        weapon_id = to_int(row, "id", None)
        if weapon_id % 10000 != 0:
            continue
        safe_param_name = player_weapon_param_name(row)
        override_name = name_overrides.get(weapon_id, "").strip()
        if not safe_param_name and not override_name:
            continue
        raw_name = override_name or name_map.get(weapon_id, "").strip() or safe_param_name
        if raw_name:
            out[weapon_series_id(weapon_id)] = raw_name
    return out


def physical_attack_attributes(row: dict[str, str]) -> tuple[str, str]:
    attributes: list[str] = []
    for field, label in PHYSICAL_ATTACK_FLAGS:
        if to_int(row, field, None) != 0 and label not in attributes:
            attributes.append(label)
    if not attributes:
        attributes.append("standard")
    secondary = attributes[1] if len(attributes) > 1 else attributes[0]
    return attributes[0], secondary


def expand_calc_correct_curve(curve: dict[str, str]) -> list[float]:
    curve_id = curve.get("id", "unknown")

    def required_finite(key: str) -> float:
        raw = curve.get(key)
        if raw is None or raw == "":
            raise ValueError(f"curve {curve_id}: missing {key}")
        value = float(raw)
        if not math.isfinite(value):
            raise ValueError(f"curve {curve_id}: non-finite {key}={raw}")
        return value

    stage_vals = [required_finite(f"stageMaxVal{i}") for i in range(5)]
    stage_grow_vals = [required_finite(f"stageMaxGrowVal{i}") for i in range(5)]
    exponents = [required_finite(f"adjPt_maxGrowVal{i}") for i in range(5)]
    if stage_vals != sorted(stage_vals):
        raise ValueError(f"curve {curve_id}: stage bounds are not nondecreasing: {stage_vals}")
    segments = [idx for idx in range(4) if stage_vals[idx] < stage_vals[idx + 1]]
    max_stat_value = max(MAX_EFFECTIVE_STRENGTH, math.ceil(max(stage_vals)))
    if not segments:
        if len(set(stage_grow_vals)) != 1:
            raise ValueError(
                f"curve {curve_id}: empty stage ranges have conflicting growth values"
            )
        return [0.0] + [stage_grow_vals[0] / 100.0] * max_stat_value

    multipliers = [0.0] * (max_stat_value + 1)
    for x in range(1, max_stat_value + 1):
        segment = next(
            (idx for idx in segments if stage_vals[idx] <= x <= stage_vals[idx + 1]),
            None,
        )
        if segment is None:
            segment = segments[0] if x < stage_vals[segments[0]] else segments[-1]

        left_x = stage_vals[segment]
        right_x = stage_vals[segment + 1]
        left_g = stage_grow_vals[segment]
        right_g = stage_grow_vals[segment + 1]
        exponent = exponents[segment]

        ratio = (x - left_x) / (right_x - left_x)
        ratio = max(0.0, min(1.0, ratio))
        if exponent > 0.0:
            ratio_curve = ratio**exponent
        elif exponent < 0.0:
            ratio_curve = 1.0 - (1.0 - ratio) ** (-exponent)
        else:
            ratio_curve = ratio
        if not math.isfinite(ratio_curve):
            raise ValueError(
                f"curve {curve_id}: non-finite interpolation at stat {x}, segment {segment}"
            )

        growth = left_g + (right_g - left_g) * ratio_curve
        if not math.isfinite(growth):
            raise ValueError(f"curve {curve_id}: non-finite growth at stat {x}")
        multipliers[x] = growth / 100.0

    return multipliers


def derive_damage_curve_ids(weapon: dict[str, str]) -> dict[str, int]:
    return {name: to_int(weapon, field, None) for name, field in DAMAGE_CURVE_FIELDS.items()}


def build_reinforce_rows(
    reinforce_rows: list[dict[str, str]],
) -> tuple[list[dict[str, object]], dict[int, int]]:
    rows_out: list[dict[str, object]] = []
    max_level_by_type: dict[int, int] = {}

    for row in reinforce_rows:
        row_id = to_int(row, "id", None)
        level = row_id % 100
        reinforce_type = row_id - level

        max_level_by_type[reinforce_type] = max(max_level_by_type.get(reinforce_type, 0), level)
        rows_out.append(
            {
                "reinforce_type": reinforce_type,
                "level": level,
                "physical_damage_mult": to_float(row, "physicsAtkRate", None),
                "magic_damage_mult": to_float(row, "magicAtkRate", None),
                "fire_damage_mult": to_float(row, "fireAtkRate", None),
                "lightning_damage_mult": to_float(row, "thunderAtkRate", None),
                "holy_damage_mult": to_float(row, "darkAtkRate", None),
                "str_scaling_mult": to_float(row, "correctStrengthRate", None),
                "dex_scaling_mult": to_float(row, "correctAgilityRate", None),
                "int_scaling_mult": to_float(row, "correctMagicRate", None),
                "fai_scaling_mult": to_float(row, "correctFaithRate", None),
                "arc_scaling_mult": to_float(row, "correctLuckRate", None),
                "base_attack_mult": to_float(row, "baseAtkRate", None),
            }
        )

    rows_out.sort(
        key=lambda item: (object_to_int(item["reinforce_type"]), object_to_int(item["level"]))
    )
    return rows_out, max_level_by_type


def build_attack_element_rows(attack_rows: list[dict[str, str]]) -> list[dict[str, object]]:
    rows_out: list[dict[str, object]] = []

    for row in attack_rows:
        row_id = to_int(row, "id", None)
        out_row: dict[str, object] = {"attack_element_correct_id": row_id}
        for stat_key, aec_prefix in STAT_AEC_PREFIX.items():
            for damage_name, damage_suffix in DAMAGE_INFOS:
                field = f"is{aec_prefix}Correct_by{damage_suffix}"
                out_row[f"{stat_key}_scales_{damage_name}"] = to_int(row, field, None)
        rows_out.append(out_row)

    rows_out.sort(key=lambda item: object_to_int(item["attack_element_correct_id"]))
    return rows_out


def apply_profile_attack_element_rules(
    rows: list[dict[str, object]],
    *,
    zero_attack_element_uses_weapon_scaling: bool,
) -> None:
    if not zero_attack_element_uses_weapon_scaling:
        return
    zero_row = next(
        (row for row in rows if object_to_int(row["attack_element_correct_id"]) == 0),
        None,
    )
    if zero_row is None:
        raise ValueError("profile requires attack-element fallback but row 0 is missing")
    for stat_key in STAT_AEC_PREFIX:
        for damage_name, _ in DAMAGE_INFOS:
            zero_row[f"{stat_key}_scales_{damage_name}"] = 1


def build_attack_element_correct_ext_rows(
    attack_rows: list[dict[str, str]],
) -> tuple[list[str], list[dict[str, object]]]:
    fieldnames = ["attack_element_correct_id"]
    for stat_key in ("str", "dex", "int", "fai", "arc"):
        for damage_type, _ in DAMAGE_INFOS:
            fieldnames.extend(
                (
                    f"{stat_key}_scales_{damage_type}",
                    f"{stat_key}_overwrite_{damage_type}",
                    f"{stat_key}_influence_{damage_type}",
                )
            )

    rows_out: list[dict[str, object]] = []
    for source in attack_rows:
        row_id = to_int(source, "id", None)
        if row_id <= 0:
            continue
        row: dict[str, object] = {"attack_element_correct_id": row_id}
        for stat_key, raw_stat in STAT_AEC_PREFIX.items():
            for damage_type, raw_damage in DAMAGE_INFOS:
                row[f"{stat_key}_scales_{damage_type}"] = to_int(
                    source, f"is{raw_stat}Correct_by{raw_damage}", None
                )
                row[f"{stat_key}_overwrite_{damage_type}"] = to_float(
                    source, f"overwrite{raw_stat}CorrectRate_by{raw_damage}", None
                )
                row[f"{stat_key}_influence_{damage_type}"] = to_float(
                    source, f"Influence{raw_stat}CorrectRate_by{raw_damage}", None
                )
        rows_out.append(row)
    rows_out.sort(key=lambda item: object_to_int(item["attack_element_correct_id"]))
    return fieldnames, rows_out


def initialize_unsupported_aow_runtime_files(output_dir: Path) -> None:
    from tools.phase1.aow_effect_graph import OUTPUT_FIELDS
    from tools.phase1.extract_motion_workbook import (
        AOW_ATTACK_FIELDS,
        NATIVE_SKILL_ATTACK_FIELDS,
        ROUTE_ASSIGNMENT_FIELDS,
    )

    for file_name, fieldnames in (
        ("aow_attack_data.csv", AOW_ATTACK_FIELDS),
        ("aow_effect_data.csv", OUTPUT_FIELDS),
        ("aow_route_assignments.csv", ROUTE_ASSIGNMENT_FIELDS),
        ("native_skill_attack_data.csv", NATIVE_SKILL_ATTACK_FIELDS),
    ):
        write_csv(output_dir / file_name, list(fieldnames), ())


def build_weapon_rows(
    weapon_rows: list[dict[str, str]],
    name_map: dict[int, str],
    sword_art_name_map: dict[int, str],
    wep_type_name_map: dict[int, str],
    weapon_type_keys_by_id: dict[int, tuple[str, ...]],
    affinity_by_slot: Mapping[int, str],
    max_level_by_type: dict[int, int],
    player_weapon_data: Mapping[int, Any],
    *,
    use_workbook_weapon_metadata: bool,
    weapon_name_overrides: Mapping[int, str],
    somber_reinforce_types: frozenset[int],
    weapon_affinity_by_id: Mapping[int, str] | None = None,
) -> list[dict[str, object]]:
    rows_out: list[dict[str, object]] = []
    standard_name_by_series = build_standard_name_map(weapon_rows, name_map, weapon_name_overrides)

    for row in weapon_rows:
        weapon_id = to_int(row, "id", None)
        if weapon_id % 100 != 0:
            continue
        if weapon_affinity_by_id is not None and weapon_id not in weapon_affinity_by_id:
            continue
        if to_int(row, "originEquipWep", None) < 0:
            continue
        safe_param_name = player_weapon_param_name(row)

        standard_weapon_id = weapon_series_id(weapon_id)
        override_name = weapon_name_overrides.get(standard_weapon_id, "").strip()
        if not safe_param_name and not override_name:
            continue
        workbook_weapon = player_weapon_data.get(standard_weapon_id)

        raw_name = override_name or name_map.get(weapon_id, "").strip() or safe_param_name

        reinforce_type = to_int(row, "reinforceTypeId", None)
        if reinforce_type not in max_level_by_type:
            raise ValueError(f"weapon_id={weapon_id} references unknown reinforce type {reinforce_type}")
        affinity_slot = weapon_affinity_slot(weapon_id)
        disable_gem_attr = to_int(row, "disableGemAttr", None)
        if weapon_affinity_by_id is not None:
            affinity = weapon_affinity_by_id[weapon_id]
            if affinity == "Standard" and disable_gem_attr != 0:
                raw_name = safe_param_name
        else:
            affinity = affinity_by_slot.get(affinity_slot)
            if affinity_slot in affinity_by_slot and affinity_slot != 0 and disable_gem_attr != 0:
                continue
            if affinity is None:
                if disable_gem_attr != 0:
                    affinity = "Standard"
                    raw_name = safe_param_name
                else:
                    raise ValueError(
                        f"unsupported ashable affinity slot {affinity_slot} for weapon_id={weapon_id}"
                    )
        name = raw_name
        if affinity != "Standard":
            name = standard_name_by_series.get(weapon_series_id(weapon_id), raw_name)

        attack_element_correct_id = to_int(row, "attackElementCorrectId", None)
        damage_curve_ids = derive_damage_curve_ids(row)
        weapon_type_id = to_int(row, "wepType", None)
        if weapon_type_id not in wep_type_name_map:
            raise ValueError(f"weapon_id={weapon_id} has unknown wepType {weapon_type_id}")
        native_skill_id = to_int(row, "swordArtsParamId", None)
        native_skill_name = (
            sword_art_name_map.get(native_skill_id, "").strip() if native_skill_id > 0 else ""
        )

        base_physical = to_int(row, "attackBasePhysics", None)
        base_magic = to_int(row, "attackBaseMagic", None)
        base_fire = to_int(row, "attackBaseFire", None)
        base_lightning = to_int(row, "attackBaseThunder", None)
        base_holy = to_int(row, "attackBaseDark", None)
        if (base_physical + base_magic + base_fire + base_lightning + base_holy) == 0:
            continue

        if use_workbook_weapon_metadata:
            if workbook_weapon is None:
                raise ValueError(
                    f"WeaponData has no row for {raw_name} (weapon_id={standard_weapon_id})"
                )
            stamina_consumption_rate = workbook_weapon.stamina_consumption_rate
            physical_attribute_primary = workbook_weapon.physical_attribute_primary
            physical_attribute_secondary = workbook_weapon.physical_attribute_secondary
        else:
            stamina_consumption_rate = to_float(row, "staminaConsumptionRate", None)
            physical_attribute_primary, physical_attribute_secondary = physical_attack_attributes(row)
        poise = (
            {field: getattr(workbook_weapon, field) for field in WORKBOOK_POISE_FIELDS}
            if workbook_weapon is not None
            # Profiles without workbook metadata carry no poise data.
            else dict.fromkeys(WORKBOOK_POISE_FIELDS, "") | {"base_poise": 0.0, "move_count": 0}
        )

        rows_out.append(
            {
                "weapon_id": weapon_id,
                "name": name,
                "affinity": affinity,
                "weapon_type_id": weapon_type_id,
                "weapon_type_name": wep_type_name_map[weapon_type_id],
                "weapon_type_keys": "|".join(weapon_type_keys_by_id[weapon_type_id]),
                "weight": to_float(row, "weight", None),
                "critical_damage_percent": 100 + to_int(row, "throwAtkRate", None),
                "stamina_consumption_rate": stamina_consumption_rate,
                **poise,
                "physical_attribute_primary": physical_attribute_primary,
                "physical_attribute_secondary": physical_attribute_secondary,
                "base_physical": base_physical,
                "base_magic": base_magic,
                "base_fire": base_fire,
                "base_lightning": base_lightning,
                "base_holy": base_holy,
                "str_scaling": normalize_percent(to_float(row, "correctStrength", None)),
                "dex_scaling": normalize_percent(to_float(row, "correctAgility", None)),
                "int_scaling": normalize_percent(to_float(row, "correctMagic", None)),
                "fai_scaling": normalize_percent(to_float(row, "correctFaith", None)),
                "arc_scaling": normalize_percent(to_float(row, "correctLuck", None)),
                "req_str": to_int(row, "properStrength", None),
                "req_dex": to_int(row, "properAgility", None),
                "req_int": to_int(row, "properMagic", None),
                "req_fai": to_int(row, "properFaith", None),
                "req_arc": to_int(row, "properLuck", None),
                "reinforce_type": reinforce_type,
                "attack_element_correct_id": attack_element_correct_id,
                "curve_id_physical": damage_curve_ids["physical"],
                "curve_id_magic": damage_curve_ids["magic"],
                "curve_id_fire": damage_curve_ids["fire"],
                "curve_id_lightning": damage_curve_ids["lightning"],
                "curve_id_holy": damage_curve_ids["holy"],
                "curve_id_poison": damage_curve_ids["poison"],
                "curve_id_blood": damage_curve_ids["blood"],
                "curve_id_sleep": damage_curve_ids["sleep"],
                "curve_id_madness": damage_curve_ids["madness"],
                "native_skill_id": native_skill_id if native_skill_id > 0 else "",
                "native_skill_name": native_skill_name,
                "disable_gem_attr": disable_gem_attr,
                "can_change_aow": int(to_int(row, "gemMountType", None) == 2),
                "disable_two_hand_bonus": to_int(row, "isDualBlade", None),
                "is_somber": int(reinforce_type in somber_reinforce_types),
            }
        )

    rows_out.sort(key=lambda item: object_to_int(item["weapon_id"]))
    return rows_out


def build_calc_correct_rows(
    curve_rows: list[dict[str, str]],
    used_curve_ids: set[int],
) -> list[dict[str, object]]:
    rows_out: list[dict[str, object]] = []
    curves = {to_int(row, "id", None): row for row in curve_rows}
    missing = sorted(used_curve_ids - curves.keys())
    if missing:
        raise ValueError(f"weapons reference missing CalcCorrectGraph curves: {missing}")
    for curve_id in sorted(used_curve_ids):
        row = curves[curve_id]
        expanded = expand_calc_correct_curve(row)
        for stat_value, multiplier in enumerate(expanded):
            rows_out.append(
                {
                    "curve_id": curve_id,
                    "stat_value": stat_value,
                    "multiplier": multiplier,
                }
            )
    rows_out.sort(key=lambda item: (object_to_int(item["curve_id"]), object_to_int(item["stat_value"])))
    return rows_out


def build_speffect_map(sp_rows: list[dict[str, str]]) -> dict[int, tuple[float, float, float, float]]:
    effect_map: dict[int, tuple[float, float, float, float]] = {}
    for row in sp_rows:
        effect_id = to_int(row, "id", None)
        bleed, frost, poison, scarlet_rot = (
            to_float(row, STATUS_ATTACK_POWER_FIELDS[status], 0.0)
            for status in ("bleed", "frost", "poison", "scarlet_rot")
        )
        effect_map[effect_id] = (bleed, frost, poison, scarlet_rot)
    return effect_map


def canonical_gem_rows(gem_rows: list[dict[str, str]]) -> dict[int, dict[str, str]]:
    grouped_rows: dict[int, list[dict[str, str]]] = {}
    for row in gem_rows:
        raw_name = param_row_name(row)
        if (
            not raw_name.startswith("Ash of War:")
            or to_int(row, "sortId", 999999) == 999999
            or to_int(row, "iconId", 0) == 0
        ):
            continue
        canonical_name = raw_name.replace("Ash of War:", "", 1).strip()
        if not canonical_name:
            continue
        sword_art_id = to_int(row, "swordArtsParamId", -1)
        if sword_art_id < 0:
            continue
        grouped_rows.setdefault(sword_art_id, []).append(row)

    out: dict[int, dict[str, str]] = {}
    for sword_art_id, rows in grouped_rows.items():
        out[sword_art_id] = max(
            rows,
            key=lambda item: (to_int(item, "isSpecialSwordArt", 0) != 0, to_int(item, "id", None)),
        )
    return out


def build_aow_rows(
    aow_rows: list[dict[str, str]],
    effect_map: dict[int, tuple[float, float, float, float]],
    sword_art_name_map: Mapping[int, str],
    affinity_by_slot: Mapping[int, str],
) -> list[dict[str, object]]:
    rows_out: list[dict[str, object]] = []
    for sword_art_id, canonical in canonical_gem_rows(aow_rows).items():
        aow_name = sword_art_name_map.get(sword_art_id, "").strip()
        if not aow_name:
            aow_name = param_row_name(canonical).replace("Ash of War:", "", 1).strip()
        if not aow_name or aow_name in {"%null%", "[ERROR]"}:
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
        scarlet_rot = 0.0
        for effect_id in effect_ids:
            effect_bleed, effect_frost, effect_poison, effect_scarlet_rot = effect_map.get(
                effect_id,
                (0.0, 0.0, 0.0, 0.0),
            )
            bleed += effect_bleed
            frost += effect_frost
            poison += effect_poison
            scarlet_rot += effect_scarlet_rot

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
                "scarlet_rot_buildup_add": scarlet_rot,
                "valid_weapon_types": "|".join(sorted(valid_weapon_types)),
                "valid_affinities": "|".join(
                    affinity
                    for slot, affinity in affinity_by_slot.items()
                    if to_int(canonical, f"configurableWepAttr{slot:02d}", 0) != 0
                ),
            }
        )

    rows_out.sort(key=lambda item: object_to_int(item["aow_id"]))
    return rows_out


def load_regulation_context(
    regulation_path: Path,
    witchybnd_path: Path,
    workdir: Path,
    weapon_name_xmls: Sequence[Path] = (),
    arts_name_xmls: Sequence[Path] = (),
) -> RegulationContext:
    unpacked_dir = unpack_regulation(regulation_path, witchybnd_path, workdir)
    xml_paths = {
        param: serialize_param(unpacked_dir, witchybnd_path, param)
        for param in (WEAPON_PARAM, REINFORCE_PARAM, CALC_CORRECT_PARAM,
                      ATTACK_ELEMENT_PARAM, AOW_PARAM, SPEFFECT_PARAM)
    }
    weapon_rows = list(iter_param_rows(xml_paths[WEAPON_PARAM], default_fields=WEAPON_MODEL_FIELDS))
    reinforce_rows = list(iter_param_rows(xml_paths[REINFORCE_PARAM], apply_defaults=True))
    curve_rows = list(iter_param_rows(xml_paths[CALC_CORRECT_PARAM], apply_defaults=True))
    attack_rows = list(iter_param_rows(xml_paths[ATTACK_ELEMENT_PARAM], apply_defaults=True))
    aow_rows = list(iter_param_rows(xml_paths[AOW_PARAM], apply_defaults=True))
    sp_rows = list(iter_param_rows(xml_paths[SPEFFECT_PARAM]))
    weapon_name_map = (
        load_merged_fmg_name_map(weapon_name_xmls, "weapon")
        if weapon_name_xmls
        else load_weapon_name_map(witchybnd_path)
    )
    sword_art_name_map = (
        load_merged_fmg_name_map(arts_name_xmls, "skill")
        if arts_name_xmls
        else load_param_name_map(witchybnd_path, "SwordArtsParam")
    )
    wep_type_name_map = load_wep_type_name_map(witchybnd_path)
    gem_mount_fields = discover_gem_mount_fields(xml_paths[AOW_PARAM])
    weapon_type_keys_by_id = build_weapon_type_key_map(wep_type_name_map, gem_mount_fields)
    return RegulationContext(
        workdir=unpacked_dir.parent,
        weapon_rows=weapon_rows,
        reinforce_rows=reinforce_rows,
        curve_rows=curve_rows,
        attack_rows=attack_rows,
        aow_rows=aow_rows,
        sp_rows=sp_rows,
        weapon_name_map=weapon_name_map,
        sword_art_name_map=sword_art_name_map,
        wep_type_name_map=wep_type_name_map,
        weapon_type_keys_by_id=weapon_type_keys_by_id,
    )


def main() -> int:
    args = parse_args()
    profile = profile_definition(args.profile)
    regulation_path = (args.regulation or profile.regulation_path).resolve()
    witchybnd_path = (args.witchybnd or discover_witchybnd(ROOT)).resolve()
    destination_dir = (args.output or profile.output_dir).resolve()
    workdir = (args.workdir or Path("data") / f"_work_phase1_{profile.id}").resolve()
    configured_name_xmls = tuple(args.weapon_name_xmls or profile.weapon_name_xmls)
    weapon_name_xmls = tuple(path.resolve() for path in configured_name_xmls)
    configured_arts_xmls = tuple(args.arts_name_xmls or profile.arts_name_xmls)
    arts_name_xmls = tuple(path.resolve() for path in configured_arts_xmls)
    configured_version_file = args.profile_version_file or profile.version_file
    version_file = configured_version_file.resolve() if configured_version_file is not None else None
    from tools.phase1.snapshot_manifest import check_snapshot_destination, sha256_file

    check_snapshot_destination(destination_dir)
    weapon_affinity_by_id: dict[int, str] | None = None
    if profile.weapon_reference_path is not None:
        reference_path = profile.weapon_reference_path.resolve()
        if not reference_path.is_file():
            raise FileNotFoundError(
                f"weapon availability reference not found: {reference_path}; "
                "refresh it explicitly before extracting this profile"
            )
        reference = json.loads(reference_path.read_text(encoding="utf-8"))
        if reference.get("profile") != profile.id or reference.get("version") != profile.mod_version:
            raise ValueError(f"weapon availability reference does not match {profile.id} {profile.mod_version}")
        weapon_affinity_by_id = {
            int(entry["weaponId"]): str(entry["affinity"])
            for entry in reference["weapons"]
        }

    if not regulation_path.exists():
        raise FileNotFoundError(f"regulation.bin not found: {regulation_path}")
    regulation_sha256 = sha256_file(regulation_path)
    if regulation_sha256 != profile.regulation_sha256:
        raise ValueError(
            f"{regulation_path} has SHA-256 {regulation_sha256}; "
            f"{profile.dataset_version} expects {profile.regulation_sha256}"
        )
    if not witchybnd_path.exists():
        raise FileNotFoundError(
            f"WitchyBND.exe not found: {witchybnd_path} (pass --witchybnd <path>)"
        )
    for weapon_name_xml in weapon_name_xmls:
        if not weapon_name_xml.is_file():
            raise FileNotFoundError(f"weapon name FMG XML not found: {weapon_name_xml}")
    for arts_name_xml in arts_name_xmls:
        if not arts_name_xml.is_file():
            raise FileNotFoundError(f"skill name FMG XML not found: {arts_name_xml}")
    if version_file is not None:
        if not version_file.is_file():
            raise FileNotFoundError(f"profile version file not found: {version_file}")
        source_version = version_file.read_text(encoding="utf-8-sig").strip()
        if source_version != profile.mod_version:
            raise ValueError(
                f"profile version file says {source_version!r}; expected {profile.mod_version!r}"
            )

    from tools.phase1.extract_motion_workbook import MOTION_WORKBOOK_NAME, load_weapon_workbook_data

    workbook_path = destination_dir / MOTION_WORKBOOK_NAME
    if not workbook_path.exists():
        workbook_path = ROOT / "data" / "phase1" / MOTION_WORKBOOK_NAME

    destination_dir.parent.mkdir(parents=True, exist_ok=True)
    staging_owner = tempfile.TemporaryDirectory(
        prefix=f".{destination_dir.name}-snapshot-",
        dir=destination_dir.parent,
    )
    output_dir = Path(staging_owner.name)
    needs_workbook = (
        profile.use_workbook_weapon_metadata
        or profile.capabilities.aow_damage
        or profile.capabilities.aow_routes
    )
    if needs_workbook:
        staged_workbook = output_dir / workbook_path.name
        shutil.copy2(workbook_path, staged_workbook)
        player_weapon_data = load_weapon_workbook_data(staged_workbook)
    else:
        player_weapon_data = {}

    context = load_regulation_context(
        regulation_path,
        witchybnd_path,
        workdir,
        weapon_name_xmls,
        arts_name_xmls,
    )
    context.sword_art_name_map.update(profile.sword_art_name_overrides)
    reinforce_csv_rows, max_level_by_type = build_reinforce_rows(context.reinforce_rows)
    attack_csv_rows = build_attack_element_rows(context.attack_rows)
    apply_profile_attack_element_rules(
        attack_csv_rows,
        zero_attack_element_uses_weapon_scaling=(
            profile.rules.zero_attack_element_uses_weapon_scaling
        ),
    )
    weapon_csv_rows = build_weapon_rows(
        context.weapon_rows,
        context.weapon_name_map,
        context.sword_art_name_map,
        context.wep_type_name_map,
        context.weapon_type_keys_by_id,
        profile.affinity_by_slot,
        max_level_by_type,
        player_weapon_data,
        use_workbook_weapon_metadata=profile.use_workbook_weapon_metadata,
        weapon_name_overrides=profile.weapon_name_overrides,
        somber_reinforce_types=profile.somber_reinforce_types,
        weapon_affinity_by_id=weapon_affinity_by_id,
    )
    calc_correct_csv_rows = build_calc_correct_rows(
        context.curve_rows,
        {
            object_to_int(row[f"curve_id_{name}"])
            for row in weapon_csv_rows
            for name in DAMAGE_CURVE_FIELDS
        },
    )
    sp_effect_map = build_speffect_map(context.sp_rows)
    aow_csv_rows = build_aow_rows(
        context.aow_rows,
        sp_effect_map,
        context.sword_art_name_map,
        profile.affinity_by_slot,
    )

    write_csv(
        output_dir / "weapons.csv",
        [
            "weapon_id",
            "name",
            "affinity",
            "weapon_type_id",
            "weapon_type_name",
            "weapon_type_keys",
            "weight",
            "base_poise",
            "critical_damage_percent",
            "stamina_consumption_rate",
            "move_count",
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
            "physical_attribute_primary",
            "physical_attribute_secondary",
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
            "curve_id_poison",
            "curve_id_blood",
            "curve_id_sleep",
            "curve_id_madness",
            "native_skill_id",
            "native_skill_name",
            "disable_gem_attr",
            "can_change_aow",
            "disable_two_hand_bonus",
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
            "base_attack_mult",
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
    attack_ext_fields, attack_ext_rows = build_attack_element_correct_ext_rows(
        context.attack_rows
    )
    write_csv(
        output_dir / "attack_element_correct_ext.csv",
        attack_ext_fields,
        attack_ext_rows,
    )
    write_csv(
        output_dir / "aow.csv",
        [
            "aow_id",
            "name",
            "bleed_buildup_add",
            "frost_buildup_add",
            "poison_buildup_add",
            "scarlet_rot_buildup_add",
            "valid_weapon_types",
            "valid_affinities",
        ],
        aow_csv_rows,
    )

    from tools.phase1.derive_phase1_raw_extras import export_regulation_extras
    from tools.phase1.extract_motion_workbook import run_workbook_exports
    from tools.phase1.snapshot_manifest import (
        promote_snapshot,
        validate_snapshot_manifest,
        write_snapshot_manifest,
    )

    export_regulation_extras(
        weapon_csv_rows=[{key: str(value) for key, value in row.items()} for row in weapon_csv_rows],
        reinforce_csv_rows=[{key: str(value) for key, value in row.items()} for row in reinforce_csv_rows],
        weapon_param_rows={to_int(row, "id", None): row for row in context.weapon_rows},
        reinforce_param_rows={to_int(row, "id", None): row for row in context.reinforce_rows},
        sp_effect_rows={to_int(row, "id", None): row for row in context.sp_rows},
        output_dir=output_dir,
    )
    if profile.capabilities.aow_damage or profile.capabilities.aow_routes:
        run_workbook_exports(
            ROOT,
            output_dir,
            context.workdir / "regulation-bin",
            paramdex_path(witchybnd_path, "Defs"),
        )
    else:
        initialize_unsupported_aow_runtime_files(output_dir)
    source_paths: dict[str, Path] = {}
    for prefix, paths in (("weaponNames", weapon_name_xmls), ("artsNames", arts_name_xmls)):
        for path in paths:
            lowered = path.name.casefold()
            suffix = "Dlc01" if "_dlc01" in lowered else "Base"
            kind = f"{prefix}{suffix}"
            if kind in source_paths:
                raise ValueError(f"duplicate {kind} FMG source: {path}")
            source_paths[kind] = path
    if version_file is not None:
        source_paths["modVersion"] = version_file
    if profile.weapon_reference_path is not None:
        source_paths["weaponAvailability"] = profile.weapon_reference_path
    # Paramdex files decide names, weapon-type keys and field layouts.
    source_paths["witchybnd"] = witchybnd_path
    source_paths["paramdexMetaEquipParamWeapon"] = paramdex_path(
        witchybnd_path, "Meta", "EquipParamWeapon.xml"
    )
    if not weapon_name_xmls:
        source_paths["paramdexNamesEquipParamWeapon"] = paramdex_path(
            witchybnd_path, "Names", "EquipParamWeapon.txt"
        )
    if not arts_name_xmls:
        source_paths["paramdexNamesSwordArtsParam"] = paramdex_path(
            witchybnd_path, "Names", "SwordArtsParam.txt"
        )
    definitions = list(PARAM_DEFINITIONS)
    if profile.capabilities.aow_damage or profile.capabilities.aow_routes:
        definitions += ["AtkParam", "BulletParam"]
    for definition in definitions:
        source_paths[f"paramdexDefs{definition}"] = paramdex_path(
            witchybnd_path, "Defs", f"{definition}.xml"
        )
    write_snapshot_manifest(
        output_dir,
        context.workdir / "regulation.bin",
        profile=profile,
        source_paths=source_paths,
    )
    validate_snapshot_manifest(output_dir, profile)
    from tools.phase4.validate_phase4 import validate_profile_snapshot

    errors = [
        issue.message
        for issue in validate_profile_snapshot(output_dir, profile.id)
        if issue.level == "error"
    ]
    if errors:
        raise ValueError("snapshot validation failed: " + "; ".join(errors))
    promote_snapshot(output_dir, destination_dir)
    staging_owner.cleanup()

    if not args.keep_workdir:
        shutil.rmtree(context.workdir)

    print(f"Wrote CSVs to {destination_dir}")
    print(f"  weapons.csv rows: {len(weapon_csv_rows)}")
    print(f"  reinforce.csv rows: {len(reinforce_csv_rows)}")
    print(f"  calc_correct.csv rows: {len(calc_correct_csv_rows)}")
    print(f"  attack_element_correct.csv rows: {len(attack_csv_rows)}")
    print(f"  aow.csv rows: {len(aow_csv_rows)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
