"""Ash of War attack and route rows from a profile's own animations, hks and regulation.

Profiles without a motion workbook keyed to their attack IDs (Convergence) derive skill
attacks from the chain the game itself follows:

    SwordArtsParam.swordArtsTypeNew -> TAE section a(600 + type)
    hks category for the weapon     -> animation group 40000 + 100 * group
    TAE AttackBehavior/BulletBehavior event judge ID
    -> BehaviorParam_PC (weapon variation, its base variation, then 0)
    -> AtkParam_Pc directly, or Bullet.atkId_Bullet along the HitBulletID chain.
"""

from __future__ import annotations

import argparse
import re
import shutil
import struct
import sys
import tempfile
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Collection, Mapping

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.extract_motion_workbook import (  # noqa: E402
    AOW_ATTACK_FIELDS,
    ATTACK_PARAM_FIELDS,
    NATIVE_SKILL_ATTACK_FIELDS,
    ROUTE_ASSIGNMENT_FIELDS,
    is_damaging_row,
    load_attack_element_correct_ext_ids,
    verify_attack_rows_against_regulation,
)
from tools.phase1.param_binary import ParamValue, load_param_table  # noqa: E402
from tools.phase1.phase1_dump import format_float, read_csv, run_witchybnd, write_csv  # noqa: E402
from tools.phase1.profiles import SkillAnimationSources  # noqa: E402

ATTACK_BEHAVIOR_EVENT = 1
BULLET_BEHAVIOR_EVENT = 2
SP_EFFECT_EVENT = 66
SKILL_TAE_OFFSET = 600
SKILL_ANIM_BASE = 40000
# Slots 5-9 of each ten are the Lacking FP versions of slots 0-4.
LACKING_FP_SLOT_OFFSET = 5
# GetSwordArtsRequestNew: these TAE SpEffects open the RIGHT_COMBO_1/2 requests, which
# play slots 10 and 20 of the same group.
COMBO_SP_EFFECT_SLOTS = {100052: 10, 100053: 20}
# Behavior graph selectors SwordArtsOneShot_<group>_CMSG, indexed by the hks
# SwordArtsOneShotCategory (arts_idx) set for a right-hand request.
ARTS_INDEX_ANIM_GROUPS = (0, 2, 3, 20, 24, 28, 42, 47, 48, 58, 59)
# Engine constants the hks reads but does not define; each equals the wepmotionCategory
# of the weapon types it names.
WEAPON_CATEGORY_CONSTANTS = {
    "WEAPON_CATEGORY_SHORT_SWORD": 20,
    "WEAPON_CATEGORY_CLAW": 22,
    "WEAPON_CATEGORY_TWINBLADE": 24,
    "WEAPON_CATEGORY_CURVEDSWORD": 28,
    "WEAPON_CATEGORY_LARGE_RAPIER": 39,
    "WEAPON_CATEGORY_FIST": 42,
    "WEAPON_CATEGORY_LARGE_ARROW": 45,
    "WEAPON_CATEGORY_LARGE_SHIELD": 47,
    "WEAPON_CATEGORY_SMALL_SHIELD": 48,
    "WEAPON_CATEGORY_THROW_DAGGER": 53,
    "WEAPON_CATEGORY_MARTIAL_ARTS": 55,
    "WEAPON_CATEGORY_PERFUME_BOTTLE": 56,
    "WEAPON_CATEGORY_DUELING_SHIELD": 57,
    "WEAPON_CATEGORY_BACKHAND_SWORD": 58,
    "WEAPON_CATEGORY_LIGHT_LARGE_SWORD": 60,
    "WEAPON_CATEGORY_LARGE_KATANA": 61,
    "WEAPON_CATEGORY_BEAST_CLAW": 62,
}
# GetSwordArtsDiffCategory branches ported below; extraction stops if the script drops one.
CATEGORY_SCRIPT_MARKERS = (
    "170 <= artID and artID < 255 or artID >= 300 or AowCat_blacklist[artID]",
    "if artID == 169 then",
    "if wepCat == WEAPON_CATEGORY_CLAW then",
    "elseif wep_cat1 == WEAPON_CATEGORY_LIGHT_LARGE_SWORD then",
    "if wepCat == WEAPON_CATEGORY_LARGE_RAPIER then",
    "(artID == 90 or artID == 98 or artID == 151 or artID == 152)",
    "if artID >= 90 and artID <= 99 and",
)
REQUEST_SCRIPT_MARKERS = (
    "elseif env(GetSpEffectID, 100052) == TRUE then request = SWORDARTS_REQUEST_RIGHT_COMBO_1",
    "elseif env(GetSpEffectID, 100053) == TRUE then request = SWORDARTS_REQUEST_RIGHT_COMBO_2",
)
PHYSICAL_ATTRIBUTES = {
    0: "slash",
    1: "strike",
    2: "pierce",
    3: "standard",
    252: "adaptive_secondary",
    253: "adaptive_primary",
}
BEHAVIOR_FIELDS = ("refType", "refId", "stamina")
BULLET_FIELDS = ("atkId_Bullet", "HitBulletID", "intervalCreateBulletId")
WEAPON_FIELDS = ("behaviorVariationId", "wepmotionCategory", "wepmotionOneHandId")
DAMAGE_TYPES = ("physical", "magic", "fire", "lightning", "holy")
# Effect roles that need a route even on a hit without damage.
ROUTE_EFFECT_ROLES = {"per_hit_status", "per_hit_attack_power", "replacement_or_chained"}
EXCLUSION_FIELDS = ("aow_id", "weapon_id", "tae_section", "anim_id", "judge_id", "reason")
AOW_COVERAGE_FIELDS = (
    "aow_id",
    "aow_name",
    "status",
    "standard_rows",
    "damaging_rows",
    "lacking_fp_rows",
    "variant_rows",
    "bullet_rows",
    "parry_rows",
    "unique_collision_rows",
)
NATIVE_COVERAGE_FIELDS = (
    "weapon_id",
    "weapon_name",
    "native_skill_id",
    "native_skill_name",
    "tae_section",
    "anim_group",
    "attack_rows",
    "damaging_rows",
    "status",
)


@dataclass(frozen=True)
class TaeEvent:
    event_type: int
    start: float
    values: tuple[int, ...]


@dataclass(frozen=True)
class CategoryScript:
    categories: Mapping[int, frozenset[int]]
    whitelist: frozenset[int]
    blacklist: frozenset[int]
    arts_index_by_category: Mapping[int, int]


@dataclass(frozen=True)
class SkillHit:
    anim_id: int
    hit_order: int
    judge_id: int
    behavior_id: int
    attack_id: int
    is_bullet: bool
    stamina: int
    # Position along the bullet chain; a chain can apply one AtkParam more than once.
    chain_index: int = 0

    @property
    def key(self) -> tuple[int, int, int, int, int]:
        return (self.anim_id, self.hit_order, self.chain_index, self.behavior_id, self.attack_id)


@dataclass(frozen=True)
class _Resolution:
    """A skill's hits for one animation group and behavior variation."""

    tae_section: int
    group: int
    hits: tuple[SkillHit, ...]
    problems: tuple[tuple[int, int, str], ...]
    missing: str

    @property
    def content(self) -> frozenset[tuple[int, int, int, int, int]]:
        return frozenset(hit.key for hit in self.hits)


@dataclass
class _Skill:
    """Rows of one skill ID, shared by the AoW and native tables so routes stay consistent."""

    name: str
    tae_section: int
    rows: dict[tuple[str, int, int, int, int, int], dict[str, str]]


def read_tae(data: bytes, first_anim_id: int = 0) -> dict[int, tuple[TaeEvent, ...]]:
    """Behavior and SpEffect events of a 64-bit little-endian TAE3 file's animations
    numbered from first_anim_id."""
    if data[:4] != b"TAE " or data[4] != 0 or data[7] != 0xFF:
        raise ValueError("not a 64-bit little-endian TAE file")
    try:
        anim_count = struct.unpack_from("<i", data, 0x54)[0]
        table_offset = struct.unpack_from("<q", data, 0x58)[0]
        anims: dict[int, tuple[TaeEvent, ...]] = {}
        for index in range(anim_count):
            anim_id, anim_offset = struct.unpack_from("<qq", data, table_offset + 16 * index)
            if anim_id < first_anim_id:
                continue
            header_offset = struct.unpack_from("<q", data, anim_offset)[0]
            event_count = struct.unpack_from("<i", data, anim_offset + 32)[0]
            events = []
            for event_index in range(event_count):
                start_offset, _end_offset, data_offset = struct.unpack_from(
                    "<qqq", data, header_offset + 24 * event_index
                )
                event_type = struct.unpack_from("<i", data, data_offset)[0]
                if event_type not in (ATTACK_BEHAVIOR_EVENT, BULLET_BEHAVIOR_EVENT, SP_EFFECT_EVENT):
                    continue
                params_offset = struct.unpack_from("<q", data, data_offset + 8)[0]
                count = 1 if event_type == SP_EFFECT_EVENT else 3
                events.append(
                    TaeEvent(
                        event_type,
                        struct.unpack_from("<f", data, start_offset)[0],
                        struct.unpack_from(f"<{count}i", data, params_offset),
                    )
                )
            if anim_id in anims:
                raise ValueError(f"duplicate TAE animation {anim_id}")
            anims[anim_id] = tuple(events)
    except struct.error as error:
        raise ValueError(f"TAE structure exceeds file bounds: {error}") from error
    return anims


def _lua_table(text: str, name: str) -> str:
    match = re.search(rf"{re.escape(name)}\s*=\s*\{{(.*?)\n\s*\}}", text, re.DOTALL)
    if match is None:
        raise ValueError(f"hks table {name} not found")
    return re.sub(r"--[^\n]*", "", match.group(1))


def parse_category_script(category_text: str, request_text: str) -> CategoryScript:
    missing = [
        marker
        for markers, text in ((CATEGORY_SCRIPT_MARKERS, category_text), (REQUEST_SCRIPT_MARKERS, request_text))
        for marker in markers
        if marker not in re.sub(r"\s+", " ", text)
    ]
    if missing:
        raise ValueError(f"skill category script changed; missing: {missing}")
    constants = dict(WEAPON_CATEGORY_CONSTANTS)
    for name, number in re.findall(r"^(SWORD_ART_DIFF_CAT_\w+)\s*=\s*(\d+)", category_text, re.M):
        constants[name] = int(number)

    def value(name: str) -> int:
        if name not in constants:
            raise ValueError(f"unknown hks category constant {name}")
        return constants[name]

    def ids(name: str) -> frozenset[int]:
        return frozenset(int(art_id) for art_id in re.findall(r"\[(\d+)\]", _lua_table(category_text, name)))

    categories = {
        int(art_id): frozenset(value(name.strip()) for name in body.split(",") if name.strip())
        for art_id, body in re.findall(
            r"\[(\d+)\]\s*=\s*\{([^}]*)\}", _lua_table(category_text, "SwordArtsCategory")
        )
    }
    # Category-only branches of the right-hand request choose the selector index;
    # branches keyed on a weapon ID or SpEffect are skipped.
    block = request_text.split("elseif request == SWORDARTS_REQUEST_RIGHT_NORMAL then", 1)[-1]
    block = re.sub(r"--[^\n]*", "", block.split('SetVariable("SwordArtsOneShotCategory"', 1)[0])
    arts_index: dict[int, int] = {}
    for condition, index in re.findall(
        r"\b(?:if|elseif)\s+((?:(?!\bthen\b).)*?)\s+then\s+arts_idx\s*=\s*(\d+)", block, re.DOTALL
    ):
        names = re.findall(r"arts_cat\s*==\s*(\w+)", condition)
        if names and not re.sub(r"arts_cat\s*==\s*\w+|\bor\b|\s", "", condition):
            for name in names:
                arts_index.setdefault(value(name), int(index))
    if not arts_index or max(arts_index.values()) >= len(ARTS_INDEX_ANIM_GROUPS):
        raise ValueError("right-hand Sword Arts category selection was not found")
    return CategoryScript(
        categories=categories,
        whitelist=ids("local AowCat_whitelist"),
        blacklist=ids("local AowCat_blacklist"),
        arts_index_by_category=arts_index,
    )


def sword_arts_diff_category(script: CategoryScript, art_id: int, idle_cat: int, wep_cat: int) -> int:
    """GetSwordArtsDiffCategory for a right-hand, one-handed request."""
    constant = WEAPON_CATEGORY_CONSTANTS
    if art_id not in script.whitelist and (
        170 <= art_id < 255 or art_id >= 300 or art_id in script.blacklist
    ):
        return 0
    if art_id == 169:
        return constant["WEAPON_CATEGORY_LARGE_ARROW"]
    cousin = wep_cat
    if wep_cat == constant["WEAPON_CATEGORY_CLAW"]:
        cousin = constant["WEAPON_CATEGORY_FIST"]
    elif wep_cat == constant["WEAPON_CATEGORY_LIGHT_LARGE_SWORD"]:
        return 0
    idle = idle_cat - 10 if idle_cat >= 10 else idle_cat
    if wep_cat == constant["WEAPON_CATEGORY_LARGE_RAPIER"]:
        idle = 0
    if art_id in (90, 98, 151, 152) and wep_cat == constant["WEAPON_CATEGORY_DUELING_SHIELD"]:
        return 0
    categories = script.categories.get(art_id)
    if categories is None:
        return 0
    if wep_cat in categories:
        return wep_cat
    if cousin in categories:
        return cousin
    if 90 <= art_id <= 99:
        return 0
    return idle if idle in categories else 0


def skill_anim_group(script: CategoryScript, art_id: int, idle_cat: int, wep_cat: int) -> int:
    category = sword_arts_diff_category(script, art_id, idle_cat, wep_cat)
    return ARTS_INDEX_ANIM_GROUPS[script.arts_index_by_category.get(category, 0)]


def variation_chain(variation_id: int) -> tuple[int, ...]:
    return tuple(dict.fromkeys((variation_id, variation_id // 100 * 100, 0)))


def behavior_param_id(judge_id: int, variation_id: int) -> int:
    # Judges below 1000 are weapon-attack behaviors, which live in the 1xxxxxxxx rows.
    category, rest = divmod(judge_id, 1000)
    return max(category, 1) * 100_000_000 + variation_id * 1000 + rest


def resolve_behavior(
    judge_id: int,
    variation_id: int,
    behaviors: Mapping[int, Mapping[str, ParamValue]],
) -> int | None:
    for variation in variation_chain(variation_id):
        behavior_id = behavior_param_id(judge_id, variation)
        if behavior_id in behaviors:
            return behavior_id
    return None


def behavior_attacks(
    behavior: Mapping[str, ParamValue],
    bullets: Mapping[int, Mapping[str, ParamValue]],
) -> tuple[list[tuple[int, bool]], list[str]]:
    """(AtkParam ID, is bullet) pairs a behavior applies, and why any hits stay unmodeled."""
    ref_id = int(behavior["refId"])
    ref_type = int(behavior["refType"])
    if ref_type == 0:
        return ([(ref_id, False)] if ref_id > 0 else []), []
    if ref_type != 1:
        return [], [f"unsupported_behavior_ref_type_{ref_type}"]
    attacks: list[tuple[int, bool]] = []
    problems: list[str] = []
    seen: set[int] = set()
    bullet_id = ref_id
    while bullet_id > 0 and bullet_id not in seen:
        seen.add(bullet_id)
        bullet = bullets.get(bullet_id)
        if bullet is None:
            problems.append("missing_bullet")
            break
        if int(bullet["atkId_Bullet"]) > 0:
            attacks.append((int(bullet["atkId_Bullet"]), True))
        if int(bullet["intervalCreateBulletId"]) > 0:
            problems.append("interval_bullet_not_modeled")
        bullet_id = int(bullet["HitBulletID"])
    return attacks, problems


def anim_slot(anim_id: int) -> int:
    return anim_id % 100


def is_lacking_fp_slot(slot: int) -> bool:
    return slot % 10 >= LACKING_FP_SLOT_OFFSET


def group_anims(
    section: Mapping[int, tuple[TaeEvent, ...]], group: int
) -> dict[int, tuple[TaeEvent, ...]]:
    first = SKILL_ANIM_BASE + 100 * group
    return {anim_id: events for anim_id, events in section.items() if first <= anim_id < first + 100}


def skill_hits(
    anims: Mapping[int, tuple[TaeEvent, ...]],
    variation_id: int,
    behaviors: Mapping[int, Mapping[str, ParamValue]],
    bullets: Mapping[int, Mapping[str, ParamValue]],
) -> tuple[list[SkillHit], list[tuple[int, int, str]]]:
    """Resolved hits in animation and time order, and (anim, judge, reason) problems."""
    hits: list[SkillHit] = []
    problems: list[tuple[int, int, str]] = []
    for anim_id, events in sorted(anims.items()):
        behavior_events = sorted(
            (event for event in events if event.event_type != SP_EFFECT_EVENT),
            key=lambda event: event.start,
        )
        for hit_order, event in enumerate(behavior_events, start=1):
            judge_id = event.values[2]
            if judge_id < 0:
                problems.append((anim_id, judge_id, "negative_judge"))
                continue
            behavior_id = resolve_behavior(judge_id, variation_id, behaviors)
            if behavior_id is None:
                problems.append((anim_id, judge_id, "unresolved_behavior"))
                continue
            behavior = behaviors[behavior_id]
            attacks, attack_problems = behavior_attacks(behavior, bullets)
            problems.extend((anim_id, judge_id, reason) for reason in attack_problems)
            if not attacks and not attack_problems:
                problems.append((anim_id, judge_id, "behavior_without_attack"))
            hits.extend(
                SkillHit(
                    anim_id=anim_id,
                    hit_order=hit_order,
                    judge_id=judge_id,
                    behavior_id=behavior_id,
                    attack_id=attack_id,
                    is_bullet=is_bullet,
                    stamina=int(behavior["stamina"]),
                    chain_index=chain_index,
                )
                for chain_index, (attack_id, is_bullet) in enumerate(attacks)
            )
    return hits, problems


def combo_routes(anims: Mapping[int, tuple[TaeEvent, ...]]) -> list[list[int]]:
    """Every full-FP action chain: a root animation, then each combo continuation."""
    successors: dict[int, list[int]] = {}
    for anim_id, events in anims.items():
        base = anim_id - anim_slot(anim_id)
        # A Lacking FP animation is never part of a route, so it continues nothing.
        successors[anim_id] = sorted(
            {
                base + slot
                for event in events
                if not is_lacking_fp_slot(anim_slot(anim_id))
                and event.event_type == SP_EFFECT_EVENT
                and (slot := COMBO_SP_EFFECT_SLOTS.get(event.values[0])) is not None
                and base + slot in anims
                and base + slot != anim_id
            }
        )
    continuations = {anim for targets in successors.values() for anim in targets}
    routes: list[list[int]] = []

    def extend(chain: list[int]) -> None:
        routes.append(chain)
        for successor in successors[chain[-1]]:
            if successor not in chain:
                extend([*chain, successor])

    for anim_id in sorted(anims):
        if not is_lacking_fp_slot(anim_slot(anim_id)) and anim_id not in continuations:
            extend([anim_id])
    return routes


def _slot_label(slot: int, is_follow_up: bool) -> str:
    if is_follow_up:
        return f"Follow-up {slot // 10}"
    return "Activation" if slot == 0 else f"Animation slot {slot:02d}"


def build_attack_row(
    hit: SkillHit,
    *,
    sheet_row: int,
    aow_id: int,
    aow_name: str,
    variant: str,
    attack: Mapping[str, ParamValue],
) -> dict[str, str]:
    lacking_fp = is_lacking_fp_slot(anim_slot(hit.anim_id))
    attribute = int(attack["atkAttribute"])
    if attribute not in PHYSICAL_ATTRIBUTES:
        raise ValueError(f"AtkParam {hit.attack_id} has unsupported atkAttribute {attribute}")
    sequence = f"a{hit.anim_id:06d}"
    raw_name = f"{aow_name} {sequence} [{hit.hit_order}]"
    if variant:
        raw_name = f"[{variant}] {raw_name}"
    if lacking_fp:
        raw_name += " (Lacking FP)"
    row = {
        "sheet_row": str(sheet_row),
        "aow_id": str(aow_id),
        "aow_name": aow_name,
        "raw_name": raw_name,
        "variant_weapon_type": variant,
        "sequence_variant": sequence,
        "hit_kind": "bullet" if hit.is_bullet else "direct",
        "hit_order": str(hit.hit_order),
        "is_lacking_fp": "1" if lacking_fp else "0",
        "atk_id": str(hit.attack_id),
        "is_bullet_attack": "1" if hit.is_bullet else "0",
        "is_throw_attack": "1" if int(attack["throwFlag"]) == 2 else "0",
        "physical_attack_attribute": PHYSICAL_ATTRIBUTES[attribute],
        "stamina_cost": format_float(hit.stamina),
        "stamina_cost_mode": "weapon_scaled",
    }
    for column, field in ATTACK_PARAM_FIELDS.items():
        value = attack[field]
        if column.startswith("is_"):
            row[column] = "1" if value else "0"
        elif column.startswith(("sp_effect_id", "overwrite_")):
            row[column] = str(int(value))
        else:
            row[column] = format_float(float(value))
    row["is_damaging"] = "1" if is_damaging_row(
        {damage: float(row[f"{damage}_mv"]) for damage in DAMAGE_TYPES},
        {damage: float(row[f"attack_base_{damage}"]) for damage in DAMAGE_TYPES},
        row["is_add_base_atk"] == "1",
        row["is_arrow_attack"] == "1",
        hit.is_bullet,
    ) else "0"
    return row


def route_assignments(
    aow_id: int,
    aow_name: str,
    anims: Mapping[int, tuple[TaeEvent, ...]],
    rows: list[dict[str, str]],
    effect_rows: Collection[int] = (),
) -> list[dict[str, str | int]]:
    """Assign one variant's rows to every combo chain of its animation group that deals
    damage or carries a supported per-hit effect (effect_rows holds those sheet rows)."""
    rows_by_anim: dict[int, list[dict[str, str]]] = defaultdict(list)
    for row in rows:
        rows_by_anim[int(row["sequence_variant"][1:])].append(row)

    def counts(row: dict[str, str]) -> bool:
        return row["is_damaging"] == "1" or int(row["sheet_row"]) in effect_rows

    chains = [
        chain
        for chain in combo_routes(anims)
        if any(counts(row) for anim_id in chain for row in rows_by_anim[anim_id])
    ]
    # Longest chains first, so a skill's full combo is its default route.
    chains.sort(key=lambda chain: (-len(chain), chain))
    assignments: list[dict[str, str | int]] = []
    for priority, chain in enumerate(chains):
        slots = [anim_slot(anim_id) for anim_id in chain]
        label = " + ".join(_slot_label(slot, index > 0) for index, slot in enumerate(slots))
        for action_order, anim_id in enumerate(chain, start=1):
            assignments.extend(
                {
                    "aow_id": aow_id,
                    "aow_name": aow_name,
                    "sheet_row": int(row["sheet_row"]),
                    "route_id": "slots_" + "_".join(f"{slot:02d}" for slot in slots),
                    "route_label": label,
                    "route_priority": priority,
                    "action_id": f"slot_{anim_slot(anim_id):02d}",
                    "action_order": action_order,
                    "hit_order": int(row["hit_order"]),
                    "hit_count": 1,
                }
                for row in rows_by_anim[anim_id]
            )
    return assignments


def run_skill_animation_exports(
    phase1_dir: Path,
    regulation_bin_dir: Path,
    paramdex_defs_dir: Path,
    sources: SkillAnimationSources,
    witchybnd_path: Path,
    workdir: Path,
) -> None:
    from tools.phase1.aow_effect_graph import build_aow_effect_graph

    script = parse_category_script(
        sources.category_script.read_text(encoding="utf-8-sig"),
        sources.request_script.read_text(encoding="utf-8-sig"),
    )
    tae_dir = unpack_anibnd(sources.anibnd, witchybnd_path, workdir)
    sections: dict[int, dict[int, tuple[TaeEvent, ...]] | None] = {}

    def section(number: int) -> dict[int, tuple[TaeEvent, ...]] | None:
        if number not in sections:
            path = tae_dir / f"a{number}.tae"
            sections[number] = read_tae(path.read_bytes(), SKILL_ANIM_BASE) if path.is_file() else None
        return sections[number]

    def table(name: str, definition: str, fields: Collection[str]) -> dict[int, dict[str, ParamValue]]:
        return load_param_table(regulation_bin_dir / name, paramdex_defs_dir / definition, fields).rows

    behaviors = table("BehaviorParam_PC.param", "BehaviorParam.xml", BEHAVIOR_FIELDS)
    bullets = table("Bullet.param", "BulletParam.xml", BULLET_FIELDS)
    sword_arts = table("SwordArtsParam.param", "SwordArtsParam.xml", ("swordArtsTypeNew",))
    weapon_params = table("EquipParamWeapon.param", "EquipParamWeapon.xml", WEAPON_FIELDS)
    attacks = table(
        "AtkParam_Pc.param", "AtkParam.xml", {"throwFlag", "atkAttribute", *ATTACK_PARAM_FIELDS.values()}
    )
    known_overrides = load_attack_element_correct_ext_ids(phase1_dir)
    aows = read_csv(phase1_dir / "aow.csv")
    weapons = read_csv(phase1_dir / "weapons.csv")
    skills: dict[int, _Skill] = {}
    exclusions: list[dict[str, str]] = []
    resolutions: dict[tuple[int, int, int], _Resolution] = {}

    def exclude(skill_id: int, weapon_id: object, tae: object, anim_id: object, judge_id: object, reason: str) -> None:
        values = (skill_id, weapon_id, tae, anim_id, judge_id, reason)
        exclusions.append(dict(zip(EXCLUSION_FIELDS, map(str, values))))

    def resolve(skill_id: int, weapon_id: int | None) -> _Resolution:
        """The skill's hits as the hks and behavior variation select them for a weapon."""
        art = sword_arts.get(skill_id)
        if art is None:
            return _Resolution(0, 0, (), (), "missing_sword_arts_param")
        art_type = int(art["swordArtsTypeNew"])
        group = variation = 0
        if weapon_id is not None:
            param = weapon_params[weapon_id]
            group = skill_anim_group(
                script, art_type, int(param["wepmotionOneHandId"]), int(param["wepmotionCategory"])
            )
            variation = int(param["behaviorVariationId"])
        cache_key = (skill_id, group, variation)
        if cache_key not in resolutions:
            tae_number = SKILL_TAE_OFFSET + art_type
            anims_in_section = section(tae_number)
            anims = group_anims(anims_in_section or {}, group)
            if anims_in_section is None:
                resolution = _Resolution(tae_number, group, (), (), "missing_tae_section")
            elif not anims:
                resolution = _Resolution(tae_number, group, (), (), "missing_group_animation")
            else:
                hits, problems = skill_hits(anims, variation, behaviors, bullets)
                missing = "" if hits or problems else "no_behavior_events"
                resolution = _Resolution(tae_number, group, tuple(hits), tuple(problems), missing)
            resolutions[cache_key] = resolution
        return resolutions[cache_key]

    def register(
        skill_id: int, name: str, variant: str, resolution: _Resolution, weapon_id: object
    ) -> list[dict[str, str]]:
        """Numbered rows for a resolution; identical hits share one sheet row per variant."""
        tae = resolution.tae_section or ""
        if resolution.missing:
            anim = SKILL_ANIM_BASE + 100 * resolution.group if resolution.tae_section else ""
            exclude(skill_id, weapon_id, tae, anim, "", resolution.missing)
        for anim_id, judge_id, reason in resolution.problems:
            exclude(skill_id, weapon_id, tae, anim_id, judge_id, reason)
        if not resolution.hits:
            return []
        skill = skills.setdefault(skill_id, _Skill(name, resolution.tae_section, {}))
        rows = []
        for hit in resolution.hits:
            attack = attacks.get(hit.attack_id)
            override = int(attack["overwriteAttackElementCorrectId"]) if attack else 0
            if attack is None or (override > 0 and override not in known_overrides):
                reason = (
                    f"missing_attack_param_{hit.attack_id}" if attack is None
                    else f"unknown_attack_element_override_{override}"
                )
                exclude(skill_id, weapon_id, tae, hit.anim_id, hit.judge_id, reason)
                continue
            key = (variant, *hit.key)
            if key not in skill.rows:
                skill.rows[key] = build_attack_row(
                    hit,
                    sheet_row=len(skill.rows) + 1,
                    aow_id=skill_id,
                    aow_name=name,
                    variant=variant,
                    attack=attack,
                )
            rows.append(skill.rows[key])
        return rows

    # Generic Ashes of War: the hits most mountable weapons get are the untyped rows; a
    # weapon type whose own majority differs gets typed rows. Weapons that differ from
    # their type's choice are counted, since AoW rows cannot be keyed by weapon.
    aow_rows: list[dict[str, str]] = []
    coverage: list[dict[str, object]] = []
    selected_content: dict[tuple[int, str], frozenset[tuple[int, int, int, int, int]]] = {}
    for ash in aows:
        aow_id = int(ash["aow_id"])
        valid_types = set(filter(None, ash["valid_weapon_types"].split("|")))
        by_type: dict[str, Counter[_Resolution]] = defaultdict(Counter)
        for weapon in weapons:
            if weapon["can_change_aow"] == "1" and valid_types & set(weapon["weapon_type_keys"].split("|")):
                by_type[weapon["weapon_type_name"]][resolve(aow_id, int(weapon["weapon_id"]))] += 1
        overall = sum(by_type.values(), Counter[_Resolution]())
        generic = (
            max(overall, key=lambda item: (overall[item], -item.group))
            if overall else resolve(aow_id, None)
        )
        selected = register(aow_id, ash["name"], "", generic, "")
        typed: list[dict[str, str]] = []
        unrepresented = 0
        for type_name, counts in sorted(by_type.items()):
            choice = max(counts, key=lambda item: (counts[item], item.content == generic.content, -item.group))
            unrepresented += sum(count for item, count in counts.items() if item.content != choice.content)
            selected_content[(aow_id, type_name)] = choice.content
            if choice.content != generic.content:
                typed += register(aow_id, ash["name"], type_name, choice, "")
        if unrepresented:
            exclude(aow_id, "", "", "", "", f"weapons_unlike_their_type_rows_{unrepresented}")
        selected_content[(aow_id, "")] = generic.content
        aow_rows += selected + typed
        rows = selected + typed
        damaging = sum(row["is_damaging"] == "1" for row in rows)
        coverage.append(
            {
                "aow_id": aow_id,
                "aow_name": ash["name"],
                "status": "direct_damage" if damaging else "utility_only" if rows else "missing",
                "standard_rows": len(rows),
                "damaging_rows": damaging,
                "lacking_fp_rows": sum(row["is_lacking_fp"] == "1" for row in rows),
                "variant_rows": len(typed),
                "bullet_rows": sum(row["is_bullet_attack"] == "1" for row in rows),
                "parry_rows": 0,
                "unique_collision_rows": 0,
            }
        )

    # Native skills get exact per-weapon rows wherever the AoW rows the core would select
    # for that weapon resolve differently (unique skills, variations, animation groups).
    native_rows: list[dict[str, str]] = []
    native_coverage: list[dict[str, object]] = []
    aow_names = {int(ash["aow_id"]): ash["name"] for ash in aows}
    for weapon in weapons:
        if not weapon["native_skill_id"]:
            continue
        weapon_id = int(weapon["weapon_id"])
        skill_id = int(weapon["native_skill_id"])
        name = weapon["native_skill_name"] or aow_names.get(skill_id, str(skill_id))
        resolution = resolve(skill_id, weapon_id)
        generic_content = selected_content.get(
            (skill_id, weapon["weapon_type_name"]), selected_content.get((skill_id, ""))
        )
        is_generic = generic_content == resolution.content
        rows = [] if is_generic else register(skill_id, name, "", resolution, weapon_id)
        native_rows += [
            {
                **row,
                "weapon_id": weapon["weapon_id"],
                "weapon_name": weapon["name"],
                "unique_skill_weapon": weapon["name"],
            }
            for row in rows
        ]
        damaging = sum(row["is_damaging"] == "1" for row in rows)
        native_coverage.append(
            {
                "weapon_id": weapon_id,
                "weapon_name": weapon["name"],
                "native_skill_id": skill_id,
                "native_skill_name": name,
                "tae_section": resolution.tae_section or "",
                "anim_group": resolution.group,
                "attack_rows": len(rows),
                "damaging_rows": damaging,
                "status": "generic_aow" if is_generic else
                "direct_damage" if damaging else "utility_only" if rows else "missing",
            }
        )

    aow_rows.sort(key=lambda row: (int(row["aow_id"]), int(row["sheet_row"])))
    native_rows.sort(key=lambda row: (int(row["weapon_id"]), int(row["sheet_row"])))
    write_csv(phase1_dir / "aow_attack_data.csv", list(AOW_ATTACK_FIELDS), aow_rows)
    write_csv(phase1_dir / "native_skill_attack_data.csv", list(NATIVE_SKILL_ATTACK_FIELDS), native_rows)
    for file_name in ("aow_attack_data.csv", "native_skill_attack_data.csv"):
        verify_attack_rows_against_regulation(read_csv(phase1_dir / file_name), attacks, file_name)
    write_csv(phase1_dir / "aow_damage_coverage.csv", list(AOW_COVERAGE_FIELDS), coverage)
    write_csv(phase1_dir / "native_skill_damage_coverage.csv", list(NATIVE_COVERAGE_FIELDS), native_coverage)

    build_aow_effect_graph(
        project_root=ROOT,
        phase1_dir=phase1_dir,
        regulation_bin_dir=regulation_bin_dir,
        paramdex_defs_dir=paramdex_defs_dir,
        effect_names={},
        persistent_roots=False,
        allow_absent_effects=True,
    )
    effect_rows: dict[int, set[int]] = defaultdict(set)
    for effect in read_csv(phase1_dir / "aow_effect_data.csv"):
        if effect["is_supported"] == "1" and effect["role"] in ROUTE_EFFECT_ROLES:
            effect_rows[int(effect["aow_id"])].add(int(effect["sheet_row"]))
    routes: list[dict[str, str | int]] = []
    lacking: list[dict[str, str]] = []
    for skill_id, skill in sorted(skills.items()):
        buckets: dict[tuple[str, int], list[dict[str, str]]] = defaultdict(list)
        for (variant, anim_id, *_), row in skill.rows.items():
            if row["is_lacking_fp"] == "1":
                lacking.append(
                    {
                        "aow_id": str(skill_id),
                        "sheet_row": row["sheet_row"],
                        "raw_name": row["raw_name"],
                        "reason": "lacking_fp_variant",
                    }
                )
            buckets[(variant, (anim_id - SKILL_ANIM_BASE) // 100)].append(row)
        anims_in_section = section(skill.tae_section) or {}
        for (_, group), rows in sorted(buckets.items()):
            anims = group_anims(anims_in_section, group)
            routes += route_assignments(skill_id, skill.name, anims, rows, effect_rows[skill_id])
    routes.sort(key=lambda row: (int(row["aow_id"]), int(row["sheet_row"]), int(row["route_priority"])))
    write_csv(phase1_dir / "aow_route_assignments.csv", list(ROUTE_ASSIGNMENT_FIELDS), routes)
    lacking.sort(key=lambda row: (int(row["aow_id"]), int(row["sheet_row"])))
    write_csv(phase1_dir / "aow_route_exclusions.csv", ["aow_id", "sheet_row", "raw_name", "reason"], lacking)
    unique_exclusions = sorted(
        {tuple(row.values()): row for row in exclusions}.values(), key=lambda row: tuple(row.values())
    )
    write_csv(phase1_dir / "skill_animation_exclusions.csv", list(EXCLUSION_FIELDS), unique_exclusions)
    print(f"Wrote {len(aow_rows)} AoW and {len(native_rows)} native skill attack rows")
    print(f"Wrote {len(routes)} route assignments and {len(unique_exclusions)} skill animation exclusions")


def unpack_anibnd(anibnd: Path, witchybnd_path: Path, workdir: Path) -> Path:
    workdir.mkdir(parents=True, exist_ok=True)
    target = Path(tempfile.mkdtemp(prefix="anibnd-", dir=workdir)) / anibnd.name
    shutil.copy2(anibnd, target)
    run_witchybnd(witchybnd_path, target)
    tae_dirs = {path.parent for path in target.parent.rglob("*.tae")}
    if len(tae_dirs) != 1:
        raise RuntimeError(f"expected one TAE folder in {anibnd.name}, found {len(tae_dirs)}")
    return tae_dirs.pop()


def main() -> int:
    from tools.phase1.phase1_dump import paramdex_path, unpack_regulation
    from tools.phase1.profiles import discover_witchybnd, profile_definition

    parser = argparse.ArgumentParser(
        description="Write AoW attack and route CSVs from a profile's animations into a copied snapshot folder."
    )
    parser.add_argument("snapshot_dir", type=Path, help="copy of a snapshot folder (weapons.csv, aow.csv, ...)")
    parser.add_argument("--profile", default="convergence")
    parser.add_argument("--raw-dir", type=Path, help="folder holding regulation.bin and the skill sources")
    parser.add_argument("--witchybnd", type=Path)
    args = parser.parse_args()
    profile = profile_definition(args.profile)
    configured = profile.skill_animation_sources
    if configured is None:
        raise ValueError(f"{profile.id} has no skill animation sources")
    raw_dir: Path = args.raw_dir or profile.raw_dir
    sources = SkillAnimationSources(
        *(raw_dir / path.name for path in (configured.anibnd, configured.request_script, configured.category_script))
    )
    witchybnd = (args.witchybnd or discover_witchybnd(ROOT)).resolve()
    with tempfile.TemporaryDirectory(prefix="skill-anim-") as temp:
        unpacked = unpack_regulation(raw_dir / "regulation.bin", witchybnd, Path(temp))
        run_skill_animation_exports(
            args.snapshot_dir, unpacked, paramdex_path(witchybnd, "Defs"), sources, witchybnd, Path(temp)
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
