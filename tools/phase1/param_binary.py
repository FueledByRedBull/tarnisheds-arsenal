from __future__ import annotations

import re
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Collection
from xml.etree import ElementTree as ET


ParamValue = bool | int | float | bytes

_FIELD_RE = re.compile(
    r"^(?P<kind>[A-Za-z0-9_]+)\s+"
    r"(?P<name>[^:\[\s=]+)"
    r"(?:\[(?P<array_length>\d+)\])?"
    r"(?::(?P<bit_count>\d+))?"
    r"(?:\s*=\s*.+)?$"
)
_KIND_FORMATS: dict[str, tuple[str, int]] = {
    "u8": ("B", 1),
    "s8": ("b", 1),
    "u16": ("H", 2),
    "s16": ("h", 2),
    "u32": ("I", 4),
    "s32": ("i", 4),
    "f32": ("f", 4),
    "dummy8": ("B", 1),
}


@dataclass(frozen=True)
class ParamFieldLayout:
    name: str
    kind: str
    offset: int
    array_length: int = 1
    bit_count: int | None = None
    bit_shift: int = 0


@dataclass(frozen=True)
class ParamDefinition:
    param_type: str
    data_version: int
    row_size: int
    fields: tuple[ParamFieldLayout, ...]


@dataclass(frozen=True)
class ParamTable:
    param_type: str
    data_version: int
    row_size: int
    rows: dict[int, dict[str, ParamValue]]


def _child_text(node: ET.Element, name: str, default: str = "") -> str:
    child = node.find(name)
    return default if child is None or child.text is None else child.text.strip()


def _is_removed(field: ET.Element) -> bool:
    raw = field.attrib.get("RemovedVersion", _child_text(field, "RemovedVersion", "-1"))
    return raw not in {"", "-1"}


def load_param_definition(path: Path) -> ParamDefinition:
    root = ET.parse(path).getroot()
    param_type = _child_text(root, "ParamType")
    if not param_type:
        raise ValueError(f"Paramdex definition has no ParamType: {path}")
    if _child_text(root, "BigEndian", "False").lower() == "true":
        raise ValueError(f"big-endian Paramdex definitions are unsupported: {path}")
    data_version = int(_child_text(root, "DataVersion", "0"))
    fields_node = root.find("Fields")
    if fields_node is None:
        raise ValueError(f"Paramdex definition has no Fields: {path}")

    layouts: list[ParamFieldLayout] = []
    offset = 0
    active_bits: tuple[int, int, int] | None = None
    for field in fields_node:
        if _is_removed(field):
            continue
        raw_definition = field.attrib.get("Def", "").strip()
        match = _FIELD_RE.fullmatch(raw_definition)
        if match is None:
            raise ValueError(f"unsupported Paramdex field definition {raw_definition!r} in {path}")
        kind = match.group("kind")
        if kind not in _KIND_FORMATS:
            raise ValueError(f"unsupported Paramdex field type {kind!r} in {path}")
        name = match.group("name")
        array_length = int(match.group("array_length") or "1")
        bit_count_raw = match.group("bit_count")
        bit_count = None if bit_count_raw is None else int(bit_count_raw)
        _format, kind_size = _KIND_FORMATS[kind]

        if bit_count is not None:
            if array_length != 1:
                raise ValueError(f"unsupported bit field {raw_definition!r} in {path}")
            storage_bits = kind_size * 8
            if not 0 < bit_count <= storage_bits:
                raise ValueError(f"invalid bit width in {raw_definition!r} from {path}")
            if (
                active_bits is None
                or active_bits[0] != kind_size
                or active_bits[2] + bit_count > storage_bits
            ):
                active_bits = (kind_size, offset, 0)
                offset += kind_size
            storage_size, storage_offset, bit_shift = active_bits
            layouts.append(
                ParamFieldLayout(
                    name=name,
                    kind=kind,
                    offset=storage_offset,
                    bit_count=bit_count,
                    bit_shift=bit_shift,
                )
            )
            next_shift = bit_shift + bit_count
            active_bits = (
                None
                if next_shift == storage_bits
                else (storage_size, storage_offset, next_shift)
            )
            continue

        active_bits = None
        layouts.append(
            ParamFieldLayout(
                name=name,
                kind=kind,
                offset=offset,
                array_length=array_length,
            )
        )
        offset += kind_size * array_length

    return ParamDefinition(
        param_type=param_type,
        data_version=data_version,
        row_size=offset,
        fields=tuple(layouts),
    )


def _unpack_field(
    data: bytes,
    row_offset: int,
    field: ParamFieldLayout,
    endian: str,
) -> ParamValue:
    field_offset = row_offset + field.offset
    format_code, kind_size = _KIND_FORMATS[field.kind]
    if field.bit_count is not None:
        storage = struct.unpack_from(f"{endian}{format_code}", data, field_offset)[0]
        value = (int(storage) >> field.bit_shift) & ((1 << field.bit_count) - 1)
        return bool(value) if field.bit_count == 1 else value
    if field.kind == "dummy8":
        return data[field_offset : field_offset + field.array_length]
    if field.array_length == 1:
        return struct.unpack_from(f"{endian}{format_code}", data, field_offset)[0]
    values = struct.unpack_from(
        f"{endian}{field.array_length}{format_code}",
        data,
        field_offset,
    )
    if kind_size == 1:
        return bytes(values)
    raise ValueError(f"non-byte PARAM arrays are unsupported: {field.name}")


def load_param_table(
    param_path: Path,
    definition_path: Path,
    selected_fields: Collection[str] | None = None,
) -> ParamTable:
    definition = load_param_definition(definition_path)
    data = param_path.read_bytes()
    if len(data) < 64:
        raise ValueError(f"PARAM file is too short: {param_path}")

    endian_marker = struct.unpack_from("b", data, 0x2C)[0]
    if endian_marker not in {-1, 0}:
        raise ValueError(f"invalid PARAM endian marker {endian_marker}: {param_path}")
    endian = ">" if endian_marker == -1 else "<"
    flags1 = data[0x2D]
    offset_param = bool(flags1 & 0x80)
    int_data_offset = bool(flags1 & 0x01 and flags1 & 0x02)
    long_data_offset = bool(flags1 & 0x04)

    data_version, row_count = struct.unpack_from(f"{endian}HH", data, 8)
    if offset_param:
        param_type_offset = struct.unpack_from(f"{endian}q", data, 16)[0]
        if not 0 <= param_type_offset < len(data):
            raise ValueError(f"invalid PARAM type offset {param_type_offset}: {param_path}")
        type_end = data.find(b"\0", param_type_offset)
        if type_end < 0:
            raise ValueError(f"unterminated PARAM type string: {param_path}")
        param_type = data[param_type_offset:type_end].decode("ascii")
    else:
        param_type = data[12:44].split(b"\0", 1)[0].decode("ascii")

    if param_type != definition.param_type:
        raise ValueError(
            f"PARAM type mismatch for {param_path}: binary={param_type!r}, "
            f"definition={definition.param_type!r}"
        )
    if data_version != definition.data_version:
        raise ValueError(
            f"PARAM data-version mismatch for {param_path}: binary={data_version}, "
            f"definition={definition.data_version}"
        )

    pointer_offset = 48
    if int_data_offset or long_data_offset:
        pointer_offset = 64
    pointer_size = 24 if long_data_offset else 12
    pointer_end = pointer_offset + pointer_size * row_count
    if pointer_end > len(data):
        raise ValueError(f"PARAM row-pointer table exceeds file bounds: {param_path}")

    pointers: list[tuple[int, int]] = []
    for index in range(row_count):
        offset = pointer_offset + index * pointer_size
        if long_data_offset:
            row_id, _unknown, row_data_offset, _name_offset = struct.unpack_from(
                f"{endian}iiqq", data, offset
            )
        else:
            row_id, row_data_offset, _name_offset = struct.unpack_from(
                f"{endian}iII", data, offset
            )
        if not pointer_end <= row_data_offset <= len(data) - definition.row_size:
            raise ValueError(
                f"PARAM row {row_id} has invalid data offset {row_data_offset}: {param_path}"
            )
        pointers.append((row_id, row_data_offset))

    for (row_id, row_offset), (_, next_offset) in zip(pointers, pointers[1:]):
        if next_offset - row_offset != definition.row_size:
            raise ValueError(
                f"PARAM row-size mismatch at row {row_id} in {param_path}: "
                f"binary={next_offset - row_offset}, definition={definition.row_size}"
            )

    fields_by_name = {field.name: field for field in definition.fields}
    if selected_fields is None:
        selected = tuple(definition.fields)
    else:
        missing = sorted(set(selected_fields) - fields_by_name.keys())
        if missing:
            raise ValueError(f"unknown fields for {param_type}: {', '.join(missing)}")
        selected = tuple(fields_by_name[name] for name in selected_fields)

    rows: dict[int, dict[str, ParamValue]] = {}
    for row_id, row_offset in pointers:
        if row_id in rows:
            raise ValueError(f"duplicate PARAM row ID {row_id}: {param_path}")
        rows[row_id] = {
            field.name: _unpack_field(data, row_offset, field, endian) for field in selected
        }

    return ParamTable(
        param_type=param_type,
        data_version=data_version,
        row_size=definition.row_size,
        rows=rows,
    )
