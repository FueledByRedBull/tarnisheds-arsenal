from __future__ import annotations

import csv
import itertools
import re
import sys
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import TypedDict
from xml.etree import ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

MAIN_NS = '{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
REL_NS = '{http://schemas.openxmlformats.org/officeDocument/2006/relationships}'
WORKBOOK_NS = {
    'x': 'http://schemas.openxmlformats.org/spreadsheetml/2006/main',
    'r': 'http://schemas.openxmlformats.org/officeDocument/2006/relationships',
}
DAMAGE_TYPES = ('physical', 'magic', 'fire', 'lightning', 'holy')
ATTACK_TYPE_COLUMNS = {
    'physical': 'AtkPhys',
    'magic': 'AtkMag',
    'fire': 'AtkFire',
    'lightning': 'AtkLtng',
    'holy': 'AtkHoly',
}
MV_COLUMNS = {
    'physical': 'Phys MV',
    'magic': 'Magic MV',
    'fire': 'Fire MV',
    'lightning': 'Ltng MV',
    'holy': 'Holy MV',
}
VARIANT_ALIASES = {
    'backhandblade': 'Reverse-hand Blade',
    'greatspear': 'Heavy Spear',
    'reaper': 'Scythe',
}


class AowCoverage(TypedDict):
    aow_id: int
    standard_rows: int
    damaging_rows: int
    lacking_fp_rows: int
    variant_rows: int
    bullet_rows: int
    parry_rows: int
    unique_collision_rows: int
    status: str


@dataclass(frozen=True)
class WorkbookSheet:
    headers: list[str]
    rows: list[list[str]]


@dataclass(frozen=True)
class NativeSkillWeaponIndex:
    by_name: dict[str, list[dict[str, str]]]
    by_token: dict[str, list[dict[str, str]]]
    by_skill_name_token: dict[str, list[dict[str, str]]]


@dataclass(frozen=True)
class NativeSkillMatch:
    rows: list[dict[str, str]]
    status: str
    match_source: str
    inferred_skill_name: str


@dataclass(frozen=True)
class WeaponWorkbookData:
    weapon_id: int
    weapon_class: str
    name: str
    stamina_consumption_rate: float
    physical_attribute_primary: str
    physical_attribute_secondary: str


class WorkbookReader:
    def __init__(self, path: Path) -> None:
        self.path = path
        self.archive = zipfile.ZipFile(path)
        self.shared_strings = self._load_shared_strings()
        self.workbook = ET.fromstring(self.archive.read('xl/workbook.xml'))
        workbook_rels = ET.fromstring(self.archive.read('xl/_rels/workbook.xml.rels'))
        self.workbook_rel_map = {rel.attrib['Id']: rel.attrib['Target'] for rel in workbook_rels}

    def close(self) -> None:
        self.archive.close()

    def _load_shared_strings(self) -> list[str]:
        if 'xl/sharedStrings.xml' not in self.archive.namelist():
            return []
        sst = ET.fromstring(self.archive.read('xl/sharedStrings.xml'))
        out: list[str] = []
        for item in sst:
            out.append(''.join(node.text or '' for node in item.iter() if node.text))
        return out

    def read_sheet(self, name: str) -> WorkbookSheet:
        target: str | None = None
        sheets = self.workbook.find('x:sheets', WORKBOOK_NS)
        for sheet in ([] if sheets is None else sheets):
            if sheet.attrib['name'] == name:
                target = self.workbook_rel_map[sheet.attrib[f'{REL_NS}id']]
                break
        if target is None:
            raise ValueError(f'missing sheet: {name}')

        sheet_xml = ET.fromstring(self.archive.read(f'xl/{target}'))
        sheet_data = sheet_xml.find(f'{MAIN_NS}sheetData')
        if sheet_data is None:
            raise ValueError(f'missing sheetData for {name}')
        rows_xml = list(sheet_data)
        if not rows_xml:
            return WorkbookSheet([], [])

        width = 0
        parsed_rows: list[list[str]] = []
        for row in rows_xml:
            parsed: dict[int, str] = {}
            width = max(width, len(row))
            for cell in row:
                idx = self._column_index(cell.attrib['r'])
                parsed[idx] = self._cell_value(cell)
                width = max(width, idx + 1)
            parsed_rows.append([parsed.get(idx, '') for idx in range(width)])
        headers = parsed_rows[0]
        return WorkbookSheet(headers=headers, rows=parsed_rows[1:])

    def _column_index(self, cell_ref: str) -> int:
        letters = ''.join(ch for ch in cell_ref if ch.isalpha())
        value = 0
        for ch in letters:
            value = value * 26 + (ord(ch.upper()) - 64)
        return value - 1

    def _cell_value(self, cell: ET.Element) -> str:
        cell_type = cell.attrib.get('t')
        value = cell.find(f'{MAIN_NS}v')
        if cell_type == 's':
            if value is None:
                return ''
            return self.shared_strings[int(value.text or '0')]
        if cell_type == 'inlineStr':
            inline = cell.find(f'{MAIN_NS}is')
            if inline is None:
                return ''
            return ''.join(node.text or '' for node in inline.iter() if node.text)
        return value.text if value is not None and value.text is not None else ''


def norm_token(text: str) -> str:
    return re.sub(r'[^a-z0-9]+', '', text.lower())


def parse_float(value: str) -> float:
    if not value or value == '-':
        return 0.0
    return float(value)


def parse_int(value: str) -> int:
    if not value or value == '-':
        return 0
    return int(float(value))


def normalize_physical_attribute(value: str) -> str:
    stripped = value.strip()
    if stripped in {'252', '252.0'}:
        return 'adaptive_secondary'
    if stripped in {'253', '253.0'}:
        return 'adaptive_primary'
    normalized = stripped.lower()
    if normalized in {'standard', 'strike', 'slash', 'pierce'}:
        return normalized
    raise ValueError(f'unsupported physical attack attribute: {value!r}')


def load_weapon_workbook_data(workbook_path: Path) -> dict[int, WeaponWorkbookData]:
    reader = WorkbookReader(workbook_path)
    try:
        sheet = reader.read_sheet('WeaponData')
    finally:
        reader.close()

    header_idx = {header: idx for idx, header in enumerate(sheet.headers)}
    required_headers = {
        'Weapon Class',
        'Weapon',
        'ID',
        'staminaConsumptionRate',
        'atkAttribute',
        'atkAttribute2',
    }
    missing = sorted(required_headers.difference(header_idx))
    if missing:
        raise ValueError(f'WeaponData is missing columns: {", ".join(missing)}')

    out: dict[int, WeaponWorkbookData] = {}
    for values in sheet.rows:
        weapon_id = parse_int(values[header_idx['ID']])
        weapon_name = values[header_idx['Weapon']].strip()
        if weapon_id <= 0 or weapon_name == 'Unarmed':
            # WeaponData contains a non-rankable Unarmed sentinel row.
            continue
        if weapon_id in out:
            raise ValueError(f'duplicate WeaponData ID: {weapon_id}')
        out[weapon_id] = WeaponWorkbookData(
            weapon_id=weapon_id,
            weapon_class=values[header_idx['Weapon Class']].strip(),
            name=weapon_name,
            stamina_consumption_rate=parse_float(
                values[header_idx['staminaConsumptionRate']]
            ),
            physical_attribute_primary=normalize_physical_attribute(
                values[header_idx['atkAttribute']]
            ),
            physical_attribute_secondary=normalize_physical_attribute(
                values[header_idx['atkAttribute2']]
            ),
        )
    if not out:
        raise ValueError('WeaponData contains no player weapon rows')
    return out


def safe_parse_float(value: str) -> float:
    if not value or value in {'-', 'invalid'}:
        return 0.0
    return float(value)


def find_matching_aow(raw_name: str, aow_names: list[str]) -> str | None:
    simplified = re.sub(r'^\[[^\]]+\]\s*', '', raw_name).strip()
    simplified = re.sub(r'\s*\(Lacking FP\)$', '', simplified).strip()
    for candidate in aow_names:
        if simplified == candidate:
            return candidate
        for marker in (' ', ' -', ' (', ' #'):
            if simplified.startswith(candidate + marker):
                return candidate
    return None


def extract_variant(raw_name: str) -> str:
    match = re.match(r'^\[([^\]]+)\]\s*', raw_name)
    return match.group(1).strip() if match else ''


def base_name_without_variant(raw_name: str) -> str:
    name = re.sub(r'^\[[^\]]+\]\s*', '', raw_name).strip()
    name = re.sub(r'\s*\(Lacking FP\)$', '', name).strip()
    return name


def infer_skill_name_from_raw_name(raw_name: str) -> str:
    name = base_name_without_variant(raw_name)
    name = re.sub(r'\s*\[\d+\]$', '', name).strip()
    name = re.sub(r'\s+#\d+(?=(?:\s+-|\s+\[|$))', '', name).strip()
    name = re.sub(r'\s*[- ]R\d+$', '', name).strip()
    name = re.sub(r'\s*-\s*(?:Loop|Bullet)(?:\s*\([^)]*\))?$', '', name).strip()
    name = re.sub(r'\s*\?$', '', name).strip()
    return name


def parse_sequence_variant(raw_name: str, aow_name: str) -> str:
    simple = base_name_without_variant(raw_name)
    remainder = simple
    if simple == aow_name:
        return 'base'
    for marker in (f'{aow_name} - ', f'{aow_name} ', f'{aow_name}#', f'{aow_name}('):
        if remainder.startswith(marker):
            remainder = remainder[len(aow_name):].strip(' -')
            break
    remainder = remainder.strip()
    return remainder or 'base'


def parse_hit_kind(raw_name: str, sequence_variant: str) -> str:
    lowered = f'{raw_name} {sequence_variant}'.lower()
    if 'bullet' in lowered:
        return 'bullet'
    if 'parry' in lowered:
        return 'parry'
    if 'buff' in lowered or 'vow' in lowered or 'order' in lowered:
        return 'buff'
    if 'loop' in lowered:
        return 'loop'
    if 'follow' in lowered:
        return 'follow_up'
    if 'charged' in lowered or 'charge' in lowered:
        return 'charged'
    return 'direct'


def parse_hit_order(raw_name: str, sequence_variant: str) -> int:
    for pattern in (r'\[(\d+)\](?!.*\[\d+\])', r'#(\d+)(?!.*#\d+)'):
        match = re.search(pattern, raw_name)
        if match:
            return int(match.group(1))
    match = re.search(r'R(\d+)', sequence_variant)
    if match:
        return int(match.group(1))
    return 1


def load_standard_native_skill_weapons(
    weapons_csv: Path,
) -> NativeSkillWeaponIndex:
    by_name: dict[str, list[dict[str, str]]] = {}
    by_token: dict[str, list[dict[str, str]]] = {}
    by_skill_name_token: dict[str, list[dict[str, str]]] = {}
    with weapons_csv.open('r', encoding='utf-8', newline='') as handle:
        for row in csv.DictReader(handle):
            if row.get('affinity', '').strip() != 'Standard':
                continue
            native_skill_id = row.get('native_skill_id', '').strip()
            weapon_name = row.get('name', '').strip()
            native_skill_name = row.get('native_skill_name', '').strip()
            if not native_skill_id or not weapon_name:
                continue
            by_name.setdefault(weapon_name, []).append(row)
            by_token.setdefault(norm_token(weapon_name), []).append(row)
            if native_skill_name:
                by_skill_name_token.setdefault(norm_token(native_skill_name), []).append(row)
    return NativeSkillWeaponIndex(
        by_name=by_name,
        by_token=by_token,
        by_skill_name_token=by_skill_name_token,
    )


def expand_unique_skill_weapon_variants(raw_name: str) -> list[str]:
    raw_name = raw_name.strip()
    if not raw_name:
        return []
    if raw_name.startswith('(') and ')' in raw_name:
        close_idx = raw_name.find(')')
        prefix_block = raw_name[1:close_idx]
        suffix = raw_name[close_idx + 1 :].strip()
        if ' / ' in prefix_block:
            return [f'{part.strip()} {suffix}'.strip() for part in prefix_block.split('/') if part.strip()]
    if ' / ' in raw_name:
        return [part.strip() for part in raw_name.split('/') if part.strip()]
    return [raw_name]


def resolve_weapon_rows(
    raw_name: str,
    weapons_by_name: dict[str, list[dict[str, str]]],
    weapons_by_token: dict[str, list[dict[str, str]]],
) -> list[dict[str, str]]:
    matches: list[dict[str, str]] = []
    seen_weapon_ids: set[str] = set()

    def add(rows: list[dict[str, str]] | None) -> None:
        if not rows:
            return
        for row in rows:
            weapon_id = row.get('weapon_id', '')
            if weapon_id in seen_weapon_ids:
                continue
            seen_weapon_ids.add(weapon_id)
            matches.append(row)

    add(weapons_by_name.get(raw_name.strip()))
    if matches:
        return matches

    for variant in expand_unique_skill_weapon_variants(raw_name):
        add(weapons_by_name.get(variant))
    if matches:
        return matches

    add(weapons_by_token.get(norm_token(raw_name)))
    if matches:
        return matches

    for variant in expand_unique_skill_weapon_variants(raw_name):
        add(weapons_by_token.get(norm_token(variant)))
    return matches


def group_rows_by_skill_id(rows: list[dict[str, str]]) -> dict[str, list[dict[str, str]]]:
    grouped: dict[str, list[dict[str, str]]] = {}
    for row in rows:
        skill_id = row.get('native_skill_id', '').strip()
        if not skill_id:
            continue
        grouped.setdefault(skill_id, []).append(row)
    return grouped


def resolve_unique_skill_weapons(
    unique_skill_weapon: str,
    raw_name: str,
    index: NativeSkillWeaponIndex,
) -> NativeSkillMatch:
    inferred_skill_name = infer_skill_name_from_raw_name(raw_name).strip()
    skill_rows = index.by_skill_name_token.get(norm_token(inferred_skill_name), [])
    if skill_rows:
        skill_groups = group_rows_by_skill_id(skill_rows)
        matched_groups: list[list[dict[str, str]]] = []
        for group_rows in skill_groups.values():
            group_by_name: dict[str, list[dict[str, str]]] = {}
            group_by_token: dict[str, list[dict[str, str]]] = {}
            for row in group_rows:
                weapon_name = row.get('name', '').strip()
                if not weapon_name:
                    continue
                group_by_name.setdefault(weapon_name, []).append(row)
                group_by_token.setdefault(norm_token(weapon_name), []).append(row)
            matched_rows = resolve_weapon_rows(unique_skill_weapon, group_by_name, group_by_token)
            if matched_rows:
                matched_groups.append(matched_rows)
        if len(matched_groups) == 1:
            return NativeSkillMatch(matched_groups[0], 'matched', 'skill_id+weapon', inferred_skill_name)
        if len(matched_groups) > 1:
            return NativeSkillMatch([], 'ambiguous_skill_family', 'skill_id+weapon', inferred_skill_name)
        fallback_rows = resolve_weapon_rows(unique_skill_weapon, index.by_name, index.by_token)
        if fallback_rows:
            skill_ids = {row.get('native_skill_id', '').strip() for row in fallback_rows if row.get('native_skill_id', '').strip()}
            if len(skill_ids) == 1:
                return NativeSkillMatch(fallback_rows, 'matched', 'weapon_name_fallback', inferred_skill_name)
            return NativeSkillMatch([], 'ambiguous_weapon_name_fallback', 'weapon_name_fallback', inferred_skill_name)
        if len(skill_groups) == 1:
            return NativeSkillMatch([], 'unmatched_weapon_in_skill_family', 'skill_id', inferred_skill_name)
        return NativeSkillMatch([], 'ambiguous_skill_name', 'skill_name', inferred_skill_name)

    fallback_rows = resolve_weapon_rows(unique_skill_weapon, index.by_name, index.by_token)
    if fallback_rows:
        skill_ids = {row.get('native_skill_id', '').strip() for row in fallback_rows if row.get('native_skill_id', '').strip()}
        if len(skill_ids) == 1:
            return NativeSkillMatch(fallback_rows, 'matched', 'weapon_name_fallback', inferred_skill_name)
        return NativeSkillMatch([], 'ambiguous_weapon_name_fallback', 'weapon_name_fallback', inferred_skill_name)
    return NativeSkillMatch([], 'unmatched_weapon', 'none', inferred_skill_name)


def build_attack_row(
    header_idx: dict[str, int],
    values: list[str],
    row_idx: int,
    skill_id: int,
    skill_name: str,
    raw_name: str,
    known_attack_element_ext_ids: set[int] | None = None,
) -> tuple[dict[str, str], bool, str]:
    variant_weapon_type = extract_variant(raw_name)
    sequence_variant = parse_sequence_variant(raw_name, skill_name)
    hit_kind = parse_hit_kind(raw_name, sequence_variant)
    motion_values = {
        damage_type: parse_float(values[header_idx[MV_COLUMNS[damage_type]]])
        for damage_type in DAMAGE_TYPES
    }
    attack_bases = {
        damage_type: parse_float(values[header_idx[ATTACK_TYPE_COLUMNS[damage_type]]])
        for damage_type in DAMAGE_TYPES
    }
    is_add_base_atk = (values[header_idx['isAddBaseAtk']] or '0') != '0'
    is_arrow_attack = (values[header_idx['IsArrowAtk']] or '0') != '0'
    unique_skill_weapon = values[header_idx['Unique Skill Weapon']].strip()
    stamina_cost_mode = (
        'precalculated'
        if unique_skill_weapon and 'spinning chain' not in raw_name.lower()
        else 'weapon_scaled'
    )
    damaging = is_damaging_row(
        motion_values,
        attack_bases,
        is_add_base_atk,
        is_arrow_attack,
    )
    overwrite_id = parse_int(values[header_idx['overwriteAttackElementCorrectId']])
    if known_attack_element_ext_ids is not None and overwrite_id > 0 and overwrite_id not in known_attack_element_ext_ids:
        overwrite_id = 0
    row: dict[str, str] = {
        'sheet_row': str(row_idx),
        'aow_id': str(skill_id),
        'aow_name': skill_name,
        'raw_name': raw_name,
        'variant_weapon_type': variant_weapon_type,
        'skill_family': skill_name,
        'sequence_variant': sequence_variant,
        'hit_kind': hit_kind,
        'hit_order': str(parse_hit_order(raw_name, sequence_variant)),
        'is_lacking_fp': '1' if raw_name.endswith('(Lacking FP)') else '0',
        'is_damaging': '1' if damaging else '0',
        'atk_id': str(parse_int(values[header_idx['AtkId']])),
        **{
            f'sp_effect_id{index}': str(parse_int(values[header_idx[f'spEffectId{index}']]))
            for index in range(5)
        },
        'overwrite_attack_element_correct_id': str(overwrite_id),
        'is_disable_both_hands_bonus': values[header_idx['isDisableBothHandsAtkBonus']] or '0',
        'is_add_base_atk': '1' if is_add_base_atk else '0',
        'is_arrow_attack': '1' if is_arrow_attack else '0',
        'physical_attack_attribute': normalize_physical_attribute(
            values[header_idx['PhysAtkAttribute']]
        ),
        'status_mv': str(parse_float(values[header_idx['Status MV']])),
        'weapon_buff_mv': str(parse_float(values[header_idx['Weapon Buff MV']])),
        'stamina_cost': str(parse_float(values[header_idx['StaminaCost']])),
        'stamina_cost_mode': stamina_cost_mode,
    }
    for damage_type in DAMAGE_TYPES:
        row[f'{damage_type}_mv'] = str(motion_values[damage_type])
        row[f'attack_base_{damage_type}'] = str(attack_bases[damage_type])
    return row, damaging, hit_kind


def read_sp_effect_sheet(workbook_path: Path) -> list[dict[str, str]]:
    rel_ns = '{http://schemas.openxmlformats.org/officeDocument/2006/relationships}'

    def column_index(cell_ref: str) -> int:
        letters = ''.join(ch for ch in cell_ref if ch.isalpha())
        value = 0
        for ch in letters:
            value = value * 26 + (ord(ch.upper()) - 64)
        return value - 1

    with zipfile.ZipFile(workbook_path) as archive:
        shared_strings: list[str] = []
        if 'xl/sharedStrings.xml' in archive.namelist():
            sst = ET.fromstring(archive.read('xl/sharedStrings.xml'))
            for item in sst:
                shared_strings.append(''.join(node.text or '' for node in item.iter(f'{MAIN_NS}t')))

        workbook = ET.fromstring(archive.read('xl/workbook.xml'))
        workbook_rels = ET.fromstring(archive.read('xl/_rels/workbook.xml.rels'))
        rel_map = {rel.attrib['Id']: rel.attrib['Target'] for rel in workbook_rels}

        target: str | None = None
        sheets = workbook.find('x:sheets', WORKBOOK_NS)
        for sheet in ([] if sheets is None else sheets):
            if sheet.attrib['name'] == 'SpEffectParam':
                target = rel_map[sheet.attrib[f'{rel_ns}id']]
                break
        if target is None:
            raise ValueError('missing sheet: SpEffectParam')

        sheet_xml = ET.fromstring(archive.read(f'xl/{target}'))
        sheet_data = sheet_xml.find(f'{MAIN_NS}sheetData')
        if sheet_data is None:
            raise ValueError('missing sheetData for SpEffectParam')

        parsed_rows: list[list[str]] = []
        width = 0
        for row in sheet_data:
            parsed: dict[int, str] = {}
            for cell in row:
                idx = column_index(cell.attrib['r'])
                cell_type = cell.attrib.get('t')
                value = cell.find(f'{MAIN_NS}v')
                if cell_type == 's':
                    text = '' if value is None else shared_strings[int(value.text or '0')]
                else:
                    text = value.text if value is not None and value.text is not None else ''
                parsed[idx] = text
                width = max(width, idx + 1)
            parsed_rows.append([parsed.get(idx, '') for idx in range(width)])

    if len(parsed_rows) < 3:
        return []
    headers = parsed_rows[1]
    return [
        {headers[idx]: values[idx] if idx < len(values) else '' for idx in range(len(headers)) if headers[idx]}
        for values in parsed_rows[2:]
        if values and any(value for value in values)
    ]


def is_damaging_row(
    motion_values: dict[str, float],
    attack_bases: dict[str, float],
    is_add_base_atk: bool,
    is_arrow_attack: bool,
) -> bool:
    return (
        any(value > 0.0 for value in motion_values.values())
        or (
            (is_add_base_atk or is_arrow_attack)
            and any(value > 0.0 for value in attack_bases.values())
        )
    )


def build_aow_attack_data(project_root: Path, phase1_dir: Path | None = None) -> None:
    phase1_dir = project_root / 'data' / 'phase1' if phase1_dir is None else phase1_dir
    workbook_path = phase1_dir / 'ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx'
    if not workbook_path.exists():
        workbook_path = project_root / 'data' / 'phase1' / 'ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx'
    aow_csv = phase1_dir / 'aow.csv'
    out_path = phase1_dir / 'aow_attack_data.csv'
    coverage_path = phase1_dir / 'aow_damage_coverage.csv'

    aow_rows = list(csv.DictReader(aow_csv.open('r', encoding='utf-8', newline='')))
    aow_id_by_name = {row['name']: int(row['aow_id']) for row in aow_rows}
    ordered_names = sorted(aow_id_by_name, key=len, reverse=True)
    known_attack_element_ext_ids = load_attack_element_correct_ext_ids(workbook_path)
    coverage: dict[str, AowCoverage] = {
        row['name']: {
            'aow_id': int(row['aow_id']),
            'standard_rows': 0,
            'damaging_rows': 0,
            'lacking_fp_rows': 0,
            'variant_rows': 0,
            'bullet_rows': 0,
            'parry_rows': 0,
            'unique_collision_rows': 0,
            'status': 'missing',
        }
        for row in aow_rows
    }

    reader = WorkbookReader(workbook_path)
    try:
        sheet = reader.read_sheet('Ashes of War Attack Data')
        header_idx = {header: idx for idx, header in enumerate(sheet.headers)}
        rows_out: list[dict[str, str]] = []
        for row_idx, values in enumerate(sheet.rows, start=2):
            unique_skill_weapon = values[header_idx['Unique Skill Weapon']].strip()
            raw_name = values[header_idx['Name']].strip()
            if not raw_name:
                continue
            matched = find_matching_aow(raw_name, ordered_names)
            if matched is None:
                continue
            if unique_skill_weapon:
                coverage[matched]['unique_collision_rows'] += 1
            row, damaging, hit_kind = build_attack_row(
                header_idx,
                values,
                row_idx,
                aow_id_by_name[matched],
                matched,
                raw_name,
                known_attack_element_ext_ids,
            )
            coverage[matched]['standard_rows'] += 1
            coverage[matched]['lacking_fp_rows'] += int(raw_name.endswith('(Lacking FP)'))
            coverage[matched]['variant_rows'] += int(bool(row['variant_weapon_type']))
            coverage[matched]['bullet_rows'] += int(hit_kind == 'bullet')
            coverage[matched]['parry_rows'] += int(hit_kind == 'parry')
            coverage[matched]['damaging_rows'] += int(damaging)
            rows_out.append(row)
    finally:
        reader.close()

    for name, entry in coverage.items():
        standard_rows = int(entry['standard_rows'])
        damaging_rows = int(entry['damaging_rows'])
        unique_collision_rows = int(entry['unique_collision_rows'])
        if damaging_rows > 0:
            entry['status'] = 'direct_damage'
        elif standard_rows > 0:
            entry['status'] = 'utility_only'
        elif unique_collision_rows > 0:
            entry['status'] = 'unique_skill_collision_only'
        else:
            entry['status'] = 'missing'

    rows_out.sort(key=lambda row: (int(row['aow_id']), int(row['sheet_row'])))
    fieldnames = [
        'sheet_row',
        'aow_id',
        'aow_name',
        'raw_name',
        'variant_weapon_type',
        'skill_family',
        'sequence_variant',
        'hit_kind',
        'hit_order',
        'is_lacking_fp',
        'is_damaging',
        'atk_id',
        'sp_effect_id0',
        'sp_effect_id1',
        'sp_effect_id2',
        'sp_effect_id3',
        'sp_effect_id4',
        'overwrite_attack_element_correct_id',
        'is_disable_both_hands_bonus',
        'is_add_base_atk',
        'is_arrow_attack',
        'physical_attack_attribute',
        'physical_mv',
        'magic_mv',
        'fire_mv',
        'lightning_mv',
        'holy_mv',
        'attack_base_physical',
        'attack_base_magic',
        'attack_base_fire',
        'attack_base_lightning',
        'attack_base_holy',
        'status_mv',
        'weapon_buff_mv',
        'stamina_cost',
        'stamina_cost_mode',
    ]
    with out_path.open('w', encoding='utf-8', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows_out)

    coverage_fields = [
        'aow_id',
        'aow_name',
        'status',
        'standard_rows',
        'damaging_rows',
        'lacking_fp_rows',
        'variant_rows',
        'bullet_rows',
        'parry_rows',
        'unique_collision_rows',
    ]
    with coverage_path.open('w', encoding='utf-8', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=coverage_fields)
        writer.writeheader()
        for row in aow_rows:
            entry = coverage[row['name']]
            writer.writerow(
                {
                    'aow_id': entry['aow_id'],
                    'aow_name': row['name'],
                    'status': entry['status'],
                    'standard_rows': entry['standard_rows'],
                    'damaging_rows': entry['damaging_rows'],
                    'lacking_fp_rows': entry['lacking_fp_rows'],
                    'variant_rows': entry['variant_rows'],
                    'bullet_rows': entry['bullet_rows'],
                    'parry_rows': entry['parry_rows'],
                    'unique_collision_rows': entry['unique_collision_rows'],
                }
            )
    print(f'Wrote {len(rows_out)} AoW attack rows to {out_path}')
    print(f'Wrote {len(aow_rows)} AoW coverage rows to {coverage_path}')


def build_native_skill_attack_data(project_root: Path, phase1_dir: Path | None = None) -> None:
    phase1_dir = project_root / 'data' / 'phase1' if phase1_dir is None else phase1_dir
    workbook_path = phase1_dir / 'ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx'
    if not workbook_path.exists():
        workbook_path = project_root / 'data' / 'phase1' / 'ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx'
    weapons_csv = phase1_dir / 'weapons.csv'
    aow_csv = phase1_dir / 'aow.csv'
    out_path = phase1_dir / 'native_skill_attack_data.csv'
    coverage_path = phase1_dir / 'native_skill_damage_coverage.csv'
    weapon_index = load_standard_native_skill_weapons(weapons_csv)
    generic_aow_names: list[str] = []
    if aow_csv.exists():
        generic_aow_names = sorted(
            {
            row['name'].strip()
            for row in csv.DictReader(aow_csv.open('r', encoding='utf-8', newline=''))
            if row.get('name', '').strip()
            },
            key=len,
            reverse=True,
        )
    known_attack_element_ext_ids = load_attack_element_correct_ext_ids(workbook_path)

    reader = WorkbookReader(workbook_path)
    try:
        sheet = reader.read_sheet('Ashes of War Attack Data')
        header_idx = {header: idx for idx, header in enumerate(sheet.headers)}
        rows_out: list[dict[str, str]] = []
        coverage_rows: list[dict[str, str]] = []
        for row_idx, values in enumerate(sheet.rows, start=2):
            unique_skill_weapon = values[header_idx['Unique Skill Weapon']].strip()
            raw_name = values[header_idx['Name']].strip()
            if not unique_skill_weapon or not raw_name:
                continue
            match = resolve_unique_skill_weapons(
                unique_skill_weapon,
                raw_name,
                weapon_index,
            )
            status = match.status
            match_source = match.match_source
            generic_aow_name = (
                find_matching_aow(raw_name, generic_aow_names) if not match.rows else None
            )
            if generic_aow_name is not None:
                status = 'generic_aow'
                match_source = 'aow_name'
            coverage_rows.append(
                {
                    'sheet_row': str(row_idx),
                    'unique_skill_weapon': unique_skill_weapon,
                    'raw_name': raw_name,
                    'inferred_skill_name': match.inferred_skill_name,
                    'status': status,
                    'match_source': match_source,
                    'matched_skill_ids': '|'.join(
                        sorted(
                            {
                                row['native_skill_id']
                                for row in match.rows
                                if row.get('native_skill_id', '').strip()
                            }
                        )
                    ),
                    'matched_weapon_ids': '|'.join(
                        sorted({row['weapon_id'] for row in match.rows if row.get('weapon_id', '').strip()})
                    ),
                    'matched_weapon_names': '|'.join(
                        sorted({row['name'] for row in match.rows if row.get('name', '').strip()})
                    ),
                }
            )
            if not match.rows:
                continue
            for weapon in match.rows:
                skill_id = int(weapon['native_skill_id'])
                skill_name = weapon['native_skill_name'].strip() or infer_skill_name_from_raw_name(raw_name)
                row, _damaging, _hit_kind = build_attack_row(
                    header_idx,
                    values,
                    row_idx,
                    skill_id,
                    skill_name,
                    raw_name,
                    known_attack_element_ext_ids,
                )
                row['weapon_id'] = weapon['weapon_id']
                row['weapon_name'] = weapon['name']
                row['unique_skill_weapon'] = unique_skill_weapon
                rows_out.append(row)
    finally:
        reader.close()

    rows_out.sort(key=lambda row: (int(row['weapon_id']), int(row['sheet_row'])))
    fieldnames = [
        'weapon_id',
        'weapon_name',
        'unique_skill_weapon',
        'sheet_row',
        'aow_id',
        'aow_name',
        'raw_name',
        'variant_weapon_type',
        'skill_family',
        'sequence_variant',
        'hit_kind',
        'hit_order',
        'is_lacking_fp',
        'is_damaging',
        'atk_id',
        'sp_effect_id0',
        'sp_effect_id1',
        'sp_effect_id2',
        'sp_effect_id3',
        'sp_effect_id4',
        'overwrite_attack_element_correct_id',
        'is_disable_both_hands_bonus',
        'is_add_base_atk',
        'is_arrow_attack',
        'physical_attack_attribute',
        'physical_mv',
        'magic_mv',
        'fire_mv',
        'lightning_mv',
        'holy_mv',
        'attack_base_physical',
        'attack_base_magic',
        'attack_base_fire',
        'attack_base_lightning',
        'attack_base_holy',
        'status_mv',
        'weapon_buff_mv',
        'stamina_cost',
        'stamina_cost_mode',
    ]
    with out_path.open('w', encoding='utf-8', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows_out)

    coverage_fields = [
        'sheet_row',
        'unique_skill_weapon',
        'raw_name',
        'inferred_skill_name',
        'status',
        'match_source',
        'matched_skill_ids',
        'matched_weapon_ids',
        'matched_weapon_names',
    ]
    with coverage_path.open('w', encoding='utf-8', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=coverage_fields)
        writer.writeheader()
        writer.writerows(coverage_rows)

    unresolved = [row for row in coverage_rows if row['status'] != 'matched']
    if unresolved:
        unresolved_list = ', '.join(sorted({row['unique_skill_weapon'] for row in unresolved}))
        print(f'Warning: unresolved native skill workbook rows: {unresolved_list}')
    print(f'Wrote {len(rows_out)} native skill attack rows to {out_path}')
    print(f'Wrote {len(coverage_rows)} native skill coverage rows to {coverage_path}')


def build_attack_element_correct_ext(project_root: Path, phase1_dir: Path | None = None) -> None:
    phase1_dir = project_root / 'data' / 'phase1' if phase1_dir is None else phase1_dir
    workbook_path = phase1_dir / 'ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx'
    if not workbook_path.exists():
        workbook_path = project_root / 'data' / 'phase1' / 'ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx'
    out_path = phase1_dir / 'attack_element_correct_ext.csv'
    reader = WorkbookReader(workbook_path)
    try:
        sheet = reader.read_sheet('AttackElementCorrectParam')
        header_idx = {header: idx for idx, header in enumerate(sheet.headers)}
        rows_out: list[dict[str, str]] = []
        for values in sheet.rows:
            row_id = parse_int(values[header_idx['ID']])
            if row_id <= 0:
                continue
            row: dict[str, str] = {'attack_element_correct_id': str(row_id)}
            for stat_key, raw_stat in (('str', 'Strength'), ('dex', 'Dexterity'), ('int', 'Magic'), ('fai', 'Faith'), ('arc', 'Luck')):
                for damage_type, raw_damage in (
                    ('physical', 'Physics'),
                    ('magic', 'Magic'),
                    ('fire', 'Fire'),
                    ('lightning', 'Thunder'),
                    ('holy', 'Dark'),
                ):
                    scale_field = f'is{raw_stat}Correct_by{raw_damage}'
                    overwrite_field = f'overwrite{raw_stat}CorrectRate_by{raw_damage}'
                    influence_field = f'Influence{raw_stat}CorrectRate_by{raw_damage}'
                    row[f'{stat_key}_scales_{damage_type}'] = values[header_idx[scale_field]] or '0'
                    row[f'{stat_key}_overwrite_{damage_type}'] = str(parse_float(values[header_idx[overwrite_field]]))
                    row[f'{stat_key}_influence_{damage_type}'] = str(parse_float(values[header_idx[influence_field]]))
            rows_out.append(row)
    finally:
        reader.close()

    rows_out.sort(key=lambda row: int(row['attack_element_correct_id']))
    fieldnames = ['attack_element_correct_id']
    for stat_key in ('str', 'dex', 'int', 'fai', 'arc'):
        for damage_type in DAMAGE_TYPES:
            fieldnames.append(f'{stat_key}_scales_{damage_type}')
            fieldnames.append(f'{stat_key}_overwrite_{damage_type}')
            fieldnames.append(f'{stat_key}_influence_{damage_type}')
    with out_path.open('w', encoding='utf-8', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows_out)
    print(f'Wrote {len(rows_out)} AttackElementCorrect override rows to {out_path}')


def _route_dimension_values(rows: list[dict[str, str]]) -> list[tuple[str, list[str]]]:
    texts = [f"{row['sequence_variant']} {row['raw_name']}".lower() for row in rows]
    dimensions: list[tuple[str, list[str]]] = []
    if any('1h' in text for text in texts) and any('2h' in text for text in texts):
        dimensions.append(('handedness', ['1h', '2h']))
    if any(re.search(r'\br1\b', text) for text in texts) and any(
        re.search(r'\br2\b', text) for text in texts
    ):
        dimensions.append(('button', ['r1', 'r2']))
    if any('charged' in text for text in texts) and any('charged' not in text for text in texts):
        dimensions.append(('charge', ['uncharged', 'charged']))
    if any('early release' in text for text in texts) and any(
        'early release' not in text for text in texts
    ):
        dimensions.append(('release', ['full', 'early_release']))
    if any('(far)' in text for text in texts) and any(
        'bullet' in text and '(far)' not in text for text in texts
    ):
        dimensions.append(('distance', ['near', 'far']))
    return dimensions


def _row_matches_route(
    row: dict[str, str],
    route: dict[str, str],
) -> bool:
    text = f"{row['sequence_variant']} {row['raw_name']}".lower()
    handedness = '1h' if '1h' in text else '2h' if '2h' in text else None
    if 'handedness' in route and handedness is not None and route['handedness'] != handedness:
        return False
    button_match = re.search(r'\b(r1|r2)\b', text)
    if 'button' in route and button_match is not None and route['button'] != button_match.group(1):
        return False
    if 'charge' in route:
        charge = 'charged' if 'charged' in text else 'uncharged'
        if route['charge'] != charge:
            return False
    if 'release' in route:
        release = 'early_release' if 'early release' in text else 'full'
        if route['release'] != release:
            return False
    if 'distance' in route and 'bullet' in text:
        distance = 'far' if '(far)' in text else 'near'
        if route['distance'] != distance:
            return False
    return True


def _route_action_id(row: dict[str, str]) -> str:
    variant = row['sequence_variant'].lower()
    if 'loop' in variant:
        return f"loop_{row['hit_order']}"
    stage = re.search(r'#(\d+)', variant)
    if stage is not None:
        return f"stage_{stage.group(1)}"
    button = re.search(r'\b(r1|r2)\b', variant)
    if button is not None:
        return button.group(1)
    if 'early release' in variant:
        return 'early_release'
    return 'activation'


def _route_slug(parts: list[str]) -> str:
    if not parts:
        return 'full'
    return '_'.join(re.sub(r'[^a-z0-9]+', '_', part.lower()).strip('_') for part in parts)


def build_aow_route_data(project_root: Path, phase1_dir: Path | None = None) -> None:
    phase1_dir = project_root / 'data' / 'phase1' if phase1_dir is None else phase1_dir
    assignment_path = phase1_dir / 'aow_route_assignments.csv'
    exclusion_path = phase1_dir / 'aow_route_exclusions.csv'
    source_rows: dict[tuple[int, int], dict[str, str]] = {}
    for file_name in ('aow_attack_data.csv', 'native_skill_attack_data.csv'):
        with (phase1_dir / file_name).open('r', encoding='utf-8', newline='') as handle:
            for row in csv.DictReader(handle):
                key = (int(row['aow_id']), int(row['sheet_row']))
                source_rows.setdefault(key, row)

    by_skill: dict[tuple[int, str], list[dict[str, str]]] = {}
    exclusions: list[dict[str, str]] = []
    for row in source_rows.values():
        if row['is_lacking_fp'] == '1':
            exclusions.append(
                {
                    'aow_id': row['aow_id'],
                    'sheet_row': row['sheet_row'],
                    'raw_name': row['raw_name'],
                    'reason': 'lacking_fp_variant',
                }
            )
            continue
        by_skill.setdefault((int(row['aow_id']), row['aow_name']), []).append(row)

    assignments: list[dict[str, str | int]] = []
    for (aow_id, aow_name), rows in sorted(by_skill.items()):
        rows.sort(key=lambda row: int(row['sheet_row']))
        dimensions = _route_dimension_values(rows)
        combinations = list(itertools.product(*(values for _, values in dimensions))) or [()]
        route_index = 0
        for combination in combinations:
            route = {
                dimension: value
                for (dimension, _), value in zip(dimensions, combination)
            }
            route_rows = [row for row in rows if _row_matches_route(row, route)]
            if not route_rows or not any(row['is_damaging'] == '1' for row in route_rows):
                continue
            label_parts = list(combination)
            route_id = _route_slug(label_parts)
            route_label = ' / '.join(part.replace('_', ' ').title() for part in label_parts) or 'Full sequence'
            action_first_rows: dict[str, int] = {}
            for row in route_rows:
                action_id = _route_action_id(row)
                action_first_rows[action_id] = min(
                    action_first_rows.get(action_id, int(row['sheet_row'])),
                    int(row['sheet_row']),
                )
            action_order = {
                action_id: index + 1
                for index, (action_id, _) in enumerate(
                    sorted(action_first_rows.items(), key=lambda item: (item[1], item[0]))
                )
            }
            for row in route_rows:
                action_id = _route_action_id(row)
                assignments.append(
                    {
                        'aow_id': aow_id,
                        'aow_name': aow_name,
                        'sheet_row': int(row['sheet_row']),
                        'route_id': route_id,
                        'route_label': route_label,
                        'route_priority': route_index,
                        'action_id': action_id,
                        'action_order': action_order[action_id],
                        'hit_order': int(row['hit_order']),
                    }
                )
            route_index += 1

    assignments.sort(
        key=lambda row: (
            int(row['aow_id']),
            int(row['route_priority']),
            int(row['action_order']),
            int(row['hit_order']),
            int(row['sheet_row']),
        )
    )
    with assignment_path.open('w', encoding='utf-8', newline='') as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                'aow_id',
                'aow_name',
                'sheet_row',
                'route_id',
                'route_label',
                'route_priority',
                'action_id',
                'action_order',
                'hit_order',
            ],
        )
        writer.writeheader()
        writer.writerows(assignments)
    exclusions.sort(key=lambda row: (int(row['aow_id']), int(row['sheet_row'])))
    with exclusion_path.open('w', encoding='utf-8', newline='') as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=['aow_id', 'sheet_row', 'raw_name', 'reason'],
        )
        writer.writeheader()
        writer.writerows(exclusions)
    print(f'Wrote {len(assignments)} AoW route assignments to {assignment_path}')
    print(f'Wrote {len(exclusions)} AoW route exclusions to {exclusion_path}')


def load_attack_element_correct_ext_ids(workbook_path: Path) -> set[int]:
    reader = WorkbookReader(workbook_path)
    try:
        sheet = reader.read_sheet('AttackElementCorrectParam')
        header_idx = {header: idx for idx, header in enumerate(sheet.headers)}
        return {
            row_id
            for values in sheet.rows
            if (row_id := parse_int(values[header_idx['ID']])) > 0
        }
    finally:
        reader.close()


def run_workbook_exports(
    project_root: Path,
    phase1_dir: Path | None = None,
    regulation_bin_dir: Path | None = None,
    paramdex_defs_dir: Path | None = None,
) -> None:
    phase1_dir = project_root / 'data' / 'phase1' if phase1_dir is None else phase1_dir
    build_aow_attack_data(project_root, phase1_dir)
    build_native_skill_attack_data(project_root, phase1_dir)
    build_aow_route_data(project_root, phase1_dir)
    build_attack_element_correct_ext(project_root, phase1_dir)
    if regulation_bin_dir is None:
        regulation_bin_dir = project_root / 'data' / '_work_phase1_reparse' / 'regulation-bin'
    if paramdex_defs_dir is None:
        candidates = sorted((project_root / 'data' / 'raw').glob('WitchyBND-*/Assets/Paramdex/ER/Defs'))
        if len(candidates) != 1:
            raise ValueError(
                'AoW effect extraction requires one explicit Paramdex ER Defs directory; '
                f'found {len(candidates)} candidates'
            )
        paramdex_defs_dir = candidates[0]
    workbook_path = phase1_dir / 'ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx'
    if not workbook_path.exists():
        workbook_path = project_root / 'data' / 'phase1' / workbook_path.name
    effect_names = {
        parse_int(row.get('ID', '')): row.get('Name', '')
        for row in read_sp_effect_sheet(workbook_path)
        if parse_int(row.get('ID', '')) > 0
    }
    from tools.phase1.aow_effect_graph import build_aow_effect_graph

    build_aow_effect_graph(
        project_root=project_root,
        phase1_dir=phase1_dir,
        regulation_bin_dir=regulation_bin_dir,
        paramdex_defs_dir=paramdex_defs_dir,
        effect_names=effect_names,
    )


def main() -> None:
    project_root = Path(__file__).resolve().parents[2]
    run_workbook_exports(project_root)


if __name__ == '__main__':
    main()
