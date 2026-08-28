from __future__ import annotations

import csv
from collections import defaultdict
from pathlib import Path
from typing import Iterable

from tools.phase1.param_binary import ParamValue, load_param_table


DAMAGE_FIELDS = (
    ("physical_attack_power", "physicsAttackPower"),
    ("magic_attack_power", "magicAttackPower"),
    ("fire_attack_power", "fireAttackPower"),
    ("lightning_attack_power", "thunderAttackPower"),
    ("holy_attack_power", "darkAttackPower"),
)
STATUS_FIELDS = (
    ("bleed_buildup", "bloodAttackPower"),
    ("frost_buildup", "freezeAttackPower"),
    ("poison_buildup", "poizonAttackPower"),
    ("scarlet_rot_buildup", "diseaseAttackPower"),
    ("sleep_buildup", "sleepAttackPower"),
    ("madness_buildup", "madnessAttackPower"),
    ("death_buildup", "curseAttackPower"),
)
LINK_FIELDS = (
    ("replace", "replaceSpEffectId"),
    ("cycle", "cycleOccurrenceSpEffectId"),
    ("attack_occurrence", "atkOccurrenceSpEffectId"),
)
ATTACK_EFFECT_FIELDS = tuple(f"spEffectId{index}" for index in range(5))
BULLET_FIELDS = (
    "atkId_Bullet",
    "spEffectIDForShooter",
    *ATTACK_EFFECT_FIELDS,
    "HitBulletID",
    "intervalCreateBulletId",
)
SP_EFFECT_FIELDS = (
    "effectEndurance",
    "motionInterval",
    *[source for _, source in DAMAGE_FIELDS],
    *[source for _, source in STATUS_FIELDS],
    *[source for _, source in LINK_FIELDS],
    "wepParamChange",
    "stateInfo",
    "effectTargetSelf",
    "effectTargetEnemy",
    "effectTargetAttacker",
    "isUseStatusAilmentAtkPowerCorrect",
    "isUseAtkParamAtkPowerCorrect",
)
OUTPUT_FIELDS = [
    "record_id",
    "aow_id",
    "aow_name",
    "sheet_row",
    "source_kind",
    "source_param_ids",
    "effect_id",
    "effect_name",
    "parent_effect_id",
    "link_kind",
    "role",
    "activation_action_id",
    "activation_timing",
    "hand_variant",
    "is_canonical",
    "is_supported",
    "reason",
    "duration_seconds",
    *[output for output, _ in DAMAGE_FIELDS],
    *[output for output, _ in STATUS_FIELDS],
    "uses_status_correction",
    "uses_attack_correction",
]


def _read_csv(path: Path) -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def _write_csv(path: Path, fieldnames: list[str], rows: Iterable[dict[str, str]]) -> None:
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def _positive_ids(values: Iterable[ParamValue | str]) -> tuple[int, ...]:
    ids: list[int] = []
    for value in values:
        parsed = int(float(value))
        if parsed > 0:
            ids.append(parsed)
    return tuple(ids)


def _has_payload(effect: dict[str, ParamValue], fields: tuple[tuple[str, str], ...]) -> bool:
    return any(float(effect[source]) > 0.0 for _, source in fields)


def _classify_direct_effect(
    effect: dict[str, ParamValue],
    source_kind: str,
) -> tuple[str, bool, str]:
    if source_kind == "bullet_shooter":
        return "self_buff", True, "applies to the projectile shooter rather than the target hit"
    if int(effect["replaceSpEffectId"]) > 0:
        return (
            "replacement_or_chained",
            False,
            "conditional replacement semantics are not immediate per-hit buildup",
        )
    has_status = _has_payload(effect, STATUS_FIELDS)
    has_damage = _has_payload(effect, DAMAGE_FIELDS)
    targets_enemy = bool(effect["effectTargetEnemy"])
    if has_status and targets_enemy:
        return "per_hit_status", True, "target status payload is attached to this hit by numeric ID"
    if has_damage and targets_enemy:
        return (
            "per_hit_attack_power",
            False,
            "direct SpEffect attack-power payload semantics require separate damage modeling",
        )
    if bool(effect["effectTargetAttacker"]) or not targets_enemy:
        return "self_mechanic", True, "effect targets the attacker/self and is not target buildup"
    if any(int(effect[source]) > 0 for _, source in LINK_FIELDS):
        return (
            "replacement_or_chained",
            False,
            "effect delegates through a conditional SpEffect chain",
        )
    return "visual_or_non_gameplay", True, "effect has no modeled attack-power or status payload"


def _effect_values(effect: dict[str, ParamValue]) -> dict[str, str]:
    values = {
        output: str(float(effect[source])) for output, source in DAMAGE_FIELDS + STATUS_FIELDS
    }
    values.update(
        {
            "duration_seconds": str(float(effect["effectEndurance"])),
            "uses_status_correction": "1"
            if bool(effect["isUseStatusAilmentAtkPowerCorrect"])
            else "0",
            "uses_attack_correction": "1"
            if bool(effect["isUseAtkParamAtkPowerCorrect"])
            else "0",
        }
    )
    return values


def _record(
    *,
    aow_id: int,
    aow_name: str,
    sheet_row: int,
    source_kind: str,
    source_param_ids: Iterable[int],
    effect_id: int,
    effect_name: str,
    effect: dict[str, ParamValue],
    parent_effect_id: int = 0,
    link_kind: str = "direct",
    role: str,
    activation_action_id: str = "",
    activation_timing: str = "on_hit",
    hand_variant: str = "",
    is_canonical: str = "",
    is_supported: bool,
    reason: str,
) -> dict[str, str]:
    row = {
        "record_id": "",
        "aow_id": str(aow_id),
        "aow_name": aow_name,
        "sheet_row": str(sheet_row),
        "source_kind": source_kind,
        "source_param_ids": "|".join(str(value) for value in sorted(set(source_param_ids))),
        "effect_id": str(effect_id),
        "effect_name": effect_name,
        "parent_effect_id": str(parent_effect_id),
        "link_kind": link_kind,
        "role": role,
        "activation_action_id": activation_action_id,
        "activation_timing": activation_timing,
        "hand_variant": hand_variant,
        "is_canonical": is_canonical,
        "is_supported": "1" if is_supported else "0",
        "reason": reason,
    }
    row.update(_effect_values(effect))
    return row


def _effect_signature(effect: dict[str, ParamValue]) -> tuple[float | bool, ...]:
    return tuple(
        [float(effect[source]) for _, source in DAMAGE_FIELDS + STATUS_FIELDS]
        + [
            bool(effect["isUseStatusAilmentAtkPowerCorrect"]),
            bool(effect["isUseAtkParamAtkPowerCorrect"]),
        ]
    )


def build_aow_effect_graph(
    *,
    project_root: Path,
    phase1_dir: Path,
    regulation_bin_dir: Path,
    paramdex_defs_dir: Path,
    effect_names: dict[int, str],
) -> None:
    attack_rows_by_key: dict[tuple[int, int], dict[str, str]] = {}
    for filename in ("aow_attack_data.csv", "native_skill_attack_data.csv"):
        for row in _read_csv(phase1_dir / filename):
            key = (int(row["aow_id"]), int(row["sheet_row"]))
            previous = attack_rows_by_key.get(key)
            if previous is not None:
                compared = ("atk_id", *[f"sp_effect_id{index}" for index in range(5)])
                if any(previous[field] != row[field] for field in compared):
                    raise ValueError(f"conflicting duplicate attack effect source for {key}")
                continue
            attack_rows_by_key[key] = row

    aow_names = {int(row["aow_id"]): row["name"] for row in _read_csv(phase1_dir / "aow.csv")}
    atk = load_param_table(
        regulation_bin_dir / "AtkParam_Pc.param",
        paramdex_defs_dir / "AtkParam.xml",
        ATTACK_EFFECT_FIELDS,
    )
    bullets = load_param_table(
        regulation_bin_dir / "Bullet.param",
        paramdex_defs_dir / "BulletParam.xml",
        BULLET_FIELDS,
    )
    effects = load_param_table(
        regulation_bin_dir / "SpEffectParam.param",
        paramdex_defs_dir / "SpEffect.xml",
        SP_EFFECT_FIELDS,
    )

    bullets_by_attack: dict[int, list[tuple[int, dict[str, ParamValue]]]] = defaultdict(list)
    for bullet_id, bullet in bullets.rows.items():
        attack_id = int(bullet["atkId_Bullet"])
        if attack_id > 0:
            bullets_by_attack[attack_id].append((bullet_id, bullet))

    records: list[dict[str, str]] = []
    coverage: list[dict[str, str]] = []
    for (aow_id, sheet_row), attack in sorted(attack_rows_by_key.items()):
        aow_name = aow_names.get(aow_id, attack["aow_name"])
        attack_id = int(attack["atk_id"])
        workbook_effect_ids = _positive_ids(
            attack[f"sp_effect_id{index}"] for index in range(5)
        )
        if attack_id <= 0:
            if workbook_effect_ids:
                raise ValueError(
                    f"attack row {(aow_id, sheet_row)} has SpEffect IDs but no AtkParam ID"
                )
            coverage.append(
                {
                    "aow_id": str(aow_id),
                    "sheet_row": str(sheet_row),
                    "atk_id": str(attack_id),
                    "atk_effect_count": "0",
                    "bullet_source_count": "0",
                    "effect_record_count": "0",
                    "supported_target_effect_count": "0",
                    "status": "no_attack_param",
                    "reason": "workbook row has no AtkParam reference",
                }
            )
            continue
        if attack_id not in atk.rows:
            raise ValueError(f"workbook AtkParam ID {attack_id} is absent from regulation")
        regulation_effect_ids = _positive_ids(atk.rows[attack_id][field] for field in ATTACK_EFFECT_FIELDS)
        if workbook_effect_ids != regulation_effect_ids:
            raise ValueError(
                f"SpEffect mismatch for attack row {(aow_id, sheet_row)} / AtkParam {attack_id}: "
                f"workbook={workbook_effect_ids}, regulation={regulation_effect_ids}"
            )

        grouped_sources: dict[tuple[str, int], set[int]] = defaultdict(set)
        for effect_id in regulation_effect_ids:
            grouped_sources[("attack_param", effect_id)].add(attack_id)
        bullet_sources = bullets_by_attack.get(attack_id, [])
        for bullet_id, bullet in bullet_sources:
            for effect_id in _positive_ids(bullet[field] for field in ATTACK_EFFECT_FIELDS):
                grouped_sources[("bullet_target", effect_id)].add(bullet_id)
            shooter_effect_id = int(bullet["spEffectIDForShooter"])
            if shooter_effect_id > 0:
                grouped_sources[("bullet_shooter", shooter_effect_id)].add(bullet_id)

        before_count = len(records)
        supported_target_count = 0
        for (source_kind, effect_id), source_ids in sorted(grouped_sources.items()):
            effect = effects.rows.get(effect_id)
            if effect is None:
                raise ValueError(
                    f"referenced SpEffect {effect_id} is absent for attack row {(aow_id, sheet_row)}"
                )
            role, supported, reason = _classify_direct_effect(effect, source_kind)
            if supported and role == "per_hit_status":
                supported_target_count += 1
            records.append(
                _record(
                    aow_id=aow_id,
                    aow_name=aow_name,
                    sheet_row=sheet_row,
                    source_kind=source_kind,
                    source_param_ids=source_ids,
                    effect_id=effect_id,
                    effect_name=effect_names.get(effect_id, ""),
                    effect=effect,
                    role=role,
                    is_supported=supported,
                    reason=reason,
                )
            )
            if role == "replacement_or_chained":
                for link_kind, link_field in LINK_FIELDS:
                    child_id = int(effect[link_field])
                    if child_id <= 0:
                        continue
                    child = effects.rows.get(child_id)
                    if child is None:
                        raise ValueError(
                            f"SpEffect {effect_id} references absent {link_kind} effect {child_id}"
                        )
                    records.append(
                        _record(
                            aow_id=aow_id,
                            aow_name=aow_name,
                            sheet_row=sheet_row,
                            source_kind="sp_effect_chain",
                            source_param_ids=(effect_id,),
                            effect_id=child_id,
                            effect_name=effect_names.get(child_id, ""),
                            effect=child,
                            parent_effect_id=effect_id,
                            link_kind=link_kind,
                            role="replacement_or_chained",
                            activation_timing="conditional",
                            is_supported=False,
                            reason="conditional child effect is recorded but not treated as immediate hit buildup",
                        )
                    )

        effect_record_count = len(records) - before_count
        coverage.append(
            {
                "aow_id": str(aow_id),
                "sheet_row": str(sheet_row),
                "atk_id": str(attack_id),
                "atk_effect_count": str(len(regulation_effect_ids)),
                "bullet_source_count": str(len(bullet_sources)),
                "effect_record_count": str(effect_record_count),
                "supported_target_effect_count": str(supported_target_count),
                "status": "resolved" if effect_record_count else "no_effects",
                "reason": "numeric AtkParam/Bullet/SpEffect graph resolved"
                if effect_record_count
                else "no target or shooter SpEffect is attached",
            }
        )

    persistent_rows = _read_csv(project_root / "tools/phase1/aow_persistent_effect_roots.csv")
    signatures_by_aow: dict[int, list[tuple[float | bool, ...]]] = defaultdict(list)
    for mapping in persistent_rows:
        aow_id = int(mapping["aow_id"])
        expected_name = aow_names.get(aow_id)
        if expected_name != mapping["aow_name"]:
            raise ValueError(
                f"persistent effect mapping name mismatch for {aow_id}: "
                f"dataset={expected_name!r}, mapping={mapping['aow_name']!r}"
            )
        root_id = int(mapping["effect_root_id"])
        root_effect = effects.rows.get(root_id)
        if root_effect is None:
            raise ValueError(f"persistent SpEffect root {root_id} is absent for AoW {aow_id}")
        active_id = int(root_effect["cycleOccurrenceSpEffectId"])
        if active_id <= 0 or active_id not in effects.rows:
            raise ValueError(f"persistent SpEffect root {root_id} has no resolvable cycle child")
        active_effect = effects.rows[active_id]
        common = {
            "aow_id": aow_id,
            "aow_name": expected_name,
            "sheet_row": 0,
            "activation_action_id": mapping["activation_action_id"],
            "hand_variant": mapping["hand_variant"],
            "is_canonical": mapping["is_canonical"],
        }
        records.append(
            _record(
                **common,
                source_kind="verified_script_setup",
                source_param_ids=(root_id,),
                effect_id=root_id,
                effect_name=effect_names.get(root_id, ""),
                effect=root_effect,
                role="persistent_setup",
                activation_timing="after_action",
                is_supported=True,
                reason=mapping["provenance"],
            )
        )
        records.append(
            _record(
                **common,
                source_kind="sp_effect_cycle",
                source_param_ids=(root_id,),
                effect_id=active_id,
                effect_name=effect_names.get(active_id, ""),
                effect=active_effect,
                parent_effect_id=root_id,
                link_kind="cycle",
                role="persistent_weapon_buff",
                activation_timing="after_action",
                is_supported=True,
                reason="active persistent buff is the root effect's numeric cycle child",
            )
        )
        signature = list(_effect_signature(active_effect))
        occurrence_id = int(active_effect["atkOccurrenceSpEffectId"])
        if occurrence_id > 0:
            occurrence = effects.rows.get(occurrence_id)
            if occurrence is None:
                raise ValueError(
                    f"persistent effect {active_id} references absent attack occurrence {occurrence_id}"
                )
            records.append(
                _record(
                    **common,
                    source_kind="sp_effect_attack_occurrence",
                    source_param_ids=(active_id,),
                    effect_id=occurrence_id,
                    effect_name=effect_names.get(occurrence_id, ""),
                    effect=occurrence,
                    parent_effect_id=active_id,
                    link_kind="attack_occurrence",
                    role="persistent_on_hit",
                    activation_timing="while_buff_active",
                    is_supported=True,
                    reason="on-hit payload is linked by atkOccurrenceSpEffectId and remains distinct",
                )
            )
            signature.extend(_effect_signature(occurrence))
        signatures_by_aow[aow_id].append(tuple(signature))

    for aow_id, signatures in signatures_by_aow.items():
        if len(signatures) != 2 or signatures[0] != signatures[1]:
            raise ValueError(
                f"persistent right/left effect variants do not have equivalent payloads for AoW {aow_id}"
            )

    records.sort(
        key=lambda row: (
            int(row["aow_id"]),
            int(row["sheet_row"]),
            row["source_kind"],
            int(row["effect_id"]),
            int(row["parent_effect_id"]),
            row["hand_variant"],
        )
    )
    for index, row in enumerate(records, start=1):
        row["record_id"] = str(index)

    exclusions = [
        {
            "aow_id": row["aow_id"],
            "sheet_row": row["sheet_row"],
            "effect_id": row["effect_id"],
            "role": row["role"],
            "reason": row["reason"],
        }
        for row in records
        if row["is_supported"] == "0"
    ]
    _write_csv(phase1_dir / "aow_effect_data.csv", OUTPUT_FIELDS, records)
    _write_csv(
        phase1_dir / "aow_effect_coverage.csv",
        [
            "aow_id",
            "sheet_row",
            "atk_id",
            "atk_effect_count",
            "bullet_source_count",
            "effect_record_count",
            "supported_target_effect_count",
            "status",
            "reason",
        ],
        coverage,
    )
    _write_csv(
        phase1_dir / "aow_effect_exclusions.csv",
        ["aow_id", "sheet_row", "effect_id", "role", "reason"],
        exclusions,
    )
    print(f"Wrote {len(records)} ID-linked AoW effect records")
    print(f"Wrote {len(coverage)} AoW effect coverage rows")
    print(f"Wrote {len(exclusions)} explicit AoW effect exclusions")
