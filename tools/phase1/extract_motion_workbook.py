from __future__ import annotations

import csv
import re
import zipfile
from dataclasses import dataclass
from pathlib import Path
from xml.etree import ElementTree as ET

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


@dataclass(frozen=True)
class WorkbookSheet:
    headers: list[str]
    rows: list[list[str]]


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


def is_damaging_row(
    motion_values: dict[str, float],
    attack_bases: dict[str, float],
    is_add_base_atk: bool,
) -> bool:
    return (
        any(value > 0.0 for value in motion_values.values())
        or any(value > 0.0 for value in attack_bases.values())
        or is_add_base_atk
    )


def build_aow_attack_data(project_root: Path) -> None:
    workbook_path = project_root / 'data' / 'phase1' / 'ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx'
    aow_csv = project_root / 'data' / 'phase1' / 'aow.csv'
    out_path = project_root / 'data' / 'phase1' / 'aow_attack_data.csv'
    coverage_path = project_root / 'data' / 'phase1' / 'aow_damage_coverage.csv'

    aow_rows = list(csv.DictReader(aow_csv.open('r', encoding='utf-8', newline='')))
    aow_id_by_name = {row['name']: int(row['aow_id']) for row in aow_rows}
    ordered_names = sorted(aow_id_by_name, key=len, reverse=True)
    coverage: dict[str, dict[str, int | str]] = {
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
                continue
            variant_weapon_type = extract_variant(raw_name)
            sequence_variant = parse_sequence_variant(raw_name, matched)
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
            damaging = is_damaging_row(motion_values, attack_bases, is_add_base_atk)
            coverage[matched]['standard_rows'] += 1
            coverage[matched]['lacking_fp_rows'] += int(raw_name.endswith('(Lacking FP)'))
            coverage[matched]['variant_rows'] += int(bool(variant_weapon_type))
            coverage[matched]['bullet_rows'] += int(hit_kind == 'bullet')
            coverage[matched]['parry_rows'] += int(hit_kind == 'parry')
            coverage[matched]['damaging_rows'] += int(damaging)
            row: dict[str, str] = {
                'sheet_row': str(row_idx),
                'aow_id': str(aow_id_by_name[matched]),
                'aow_name': matched,
                'raw_name': raw_name,
                'variant_weapon_type': variant_weapon_type,
                'skill_family': matched,
                'sequence_variant': sequence_variant,
                'hit_kind': hit_kind,
                'hit_order': str(parse_hit_order(raw_name, sequence_variant)),
                'is_lacking_fp': '1' if raw_name.endswith('(Lacking FP)') else '0',
                'is_damaging': '1' if damaging else '0',
                'atk_id': str(parse_int(values[header_idx['AtkId']])),
                'overwrite_attack_element_correct_id': str(parse_int(values[header_idx['overwriteAttackElementCorrectId']])),
                'is_disable_both_hands_bonus': values[header_idx['isDisableBothHandsAtkBonus']] or '0',
                'is_add_base_atk': '1' if is_add_base_atk else '0',
                'status_mv': str(parse_float(values[header_idx['Status MV']])),
                'weapon_buff_mv': str(parse_float(values[header_idx['Weapon Buff MV']])),
                'stamina_cost': str(parse_float(values[header_idx['StaminaCost']])),
            }
            for damage_type in DAMAGE_TYPES:
                row[f'{damage_type}_mv'] = str(motion_values[damage_type])
                row[f'attack_base_{damage_type}'] = str(attack_bases[damage_type])
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
        'overwrite_attack_element_correct_id',
        'is_disable_both_hands_bonus',
        'is_add_base_atk',
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


def build_attack_element_correct_ext(project_root: Path) -> None:
    workbook_path = project_root / 'data' / 'phase1' / 'ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx'
    out_path = project_root / 'data' / 'phase1' / 'attack_element_correct_ext.csv'
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


def main() -> None:
    project_root = Path(__file__).resolve().parents[2]
    build_aow_attack_data(project_root)
    build_attack_element_correct_ext(project_root)


if __name__ == '__main__':
    main()
