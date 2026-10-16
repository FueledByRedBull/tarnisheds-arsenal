from __future__ import annotations

import math
import struct
import tempfile
import unittest
from pathlib import Path

from tools.phase1.param_binary import load_param_definition, load_param_table

DEFINITION = """<?xml version="1.0" encoding="utf-8"?>
<PARAMDEF XmlVersion="2">
  <ParamType>TEST_PARAM_ST</ParamType>
  <DataVersion>3</DataVersion>
  <BigEndian>{big_endian}</BigEndian>
  <Fields>
    <Field Def="s32 atkId = -1" />
    <Field Def="f32 rate" />
    <Field Def="u8 flagA:1" />
    <Field Def="u8 flagB:3" />
    <Field Def="u16 removed" RemovedVersion="2" />
    <Field Def="dummy8 pad[2]" />
    <Field Def="{bytes_kind} raw[2]" />
  </Fields>
</PARAMDEF>
"""
ROW_SIZE = 13


def row_bytes(attack_id: int, rate: float, flag_a: int, flag_b: int) -> bytes:
    return struct.pack("<ifB", attack_id, rate, flag_a | flag_b << 1) + b"\0\0" + b"\x07\x09"


def param_bytes(
    rows: list[tuple[int, bytes]],
    *,
    long_offsets: bool = False,
    param_type: bytes = b"TEST_PARAM_ST",
    data_version: int = 3,
    row_count: int | None = None,
    spacing: int = ROW_SIZE,
    terminate_type: bool = True,
) -> bytes:
    pointer_offset, pointer_size = (64, 24) if long_offsets else (48, 12)
    data_start = pointer_offset + pointer_size * len(rows)
    header = bytearray(pointer_offset)
    struct.pack_into("<HH", header, 8, data_version, len(rows) if row_count is None else row_count)
    header[0x2D] = 0x84 if long_offsets else 0
    pointers = bytearray()
    data = bytearray()
    for index, (row_id, payload) in enumerate(rows):
        offset = data_start + index * spacing
        if long_offsets:
            pointers += struct.pack("<iiqq", row_id, 0, offset, 0)
        else:
            pointers += struct.pack("<iII", row_id, offset, 0)
        data += payload.ljust(spacing, b"\0")
    type_string = param_type + (b"\0" if terminate_type else b"")
    if long_offsets:
        struct.pack_into("<q", header, 16, data_start + len(data))
        data += type_string
    else:
        header[12 : 12 + len(type_string)] = type_string
    return bytes(header + pointers + data)


class ParamBinaryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.definition = self.write_definition()

    def tearDown(self) -> None:
        self.directory.cleanup()

    def write_definition(self, *, big_endian: str = "False", bytes_kind: str = "u8") -> Path:
        path = self.root / "TestParam.xml"
        path.write_text(DEFINITION.format(big_endian=big_endian, bytes_kind=bytes_kind), encoding="utf-8")
        return path

    def load(self, data: bytes, fields: tuple[str, ...] | None = None):
        path = self.root / "Test.param"
        path.write_bytes(data)
        return load_param_table(path, self.definition, fields)

    def test_definition_layout_packs_bit_fields_and_skips_removed_fields(self) -> None:
        definition = load_param_definition(self.definition)
        layout = {field.name: (field.offset, field.bit_shift, field.bit_count) for field in definition.fields}
        self.assertEqual(definition.row_size, ROW_SIZE)
        self.assertEqual(layout["flagA"], (8, 0, 1))
        self.assertEqual(layout["flagB"], (8, 1, 3))
        self.assertEqual(layout["pad"], (9, 0, None))
        self.assertNotIn("removed", layout)

    def test_rows_round_trip_with_short_and_long_offsets(self) -> None:
        rows = [(10, row_bytes(300, 1.5, 1, 5)), (20, row_bytes(-1, 0.25, 0, 2))]
        for long_offsets in (False, True):
            with self.subTest(long_offsets=long_offsets):
                table = self.load(param_bytes(rows, long_offsets=long_offsets))
                self.assertEqual((table.param_type, table.data_version, table.row_size),
                                 ("TEST_PARAM_ST", 3, ROW_SIZE))
                self.assertEqual(
                    table.rows[10],
                    {"atkId": 300, "rate": 1.5, "flagA": True, "flagB": 5, "pad": b"\0\0", "raw": b"\x07\x09"},
                )
                self.assertEqual(table.rows[20]["flagA"], False)
        self.assertEqual(self.load(param_bytes(rows), ("atkId",)).rows[20], {"atkId": -1})
        with self.assertRaisesRegex(ValueError, "unknown fields"):
            self.load(param_bytes(rows), ("missing",))

    def test_malformed_files_fail_closed(self) -> None:
        good = [(10, row_bytes(1, 1.0, 0, 0)), (20, row_bytes(2, 2.0, 0, 0))]
        cases = {
            "too short": b"\0" * 63,
            "row-pointer table exceeds": param_bytes(good, row_count=40),
            "row-size mismatch": param_bytes(good, spacing=ROW_SIZE + 1),
            "duplicate PARAM row ID": param_bytes([good[0], (10, good[1][1])]),
            "type mismatch": param_bytes(good, param_type=b"OTHER_ST"),
            "data-version mismatch": param_bytes(good, data_version=4),
            "unterminated PARAM type": param_bytes(good, long_offsets=True, terminate_type=False),
            "non-finite PARAM field rate": param_bytes([(10, row_bytes(1, math.nan, 0, 0))]),
        }
        for message, data in cases.items():
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                self.load(data)
        truncated = param_bytes(good)[:-1]
        with self.assertRaisesRegex(ValueError, "invalid data offset"):
            self.load(truncated)

    def test_unsupported_definitions_are_rejected(self) -> None:
        for kwargs, message in (({"big_endian": "True"}, "big-endian"),
                                ({"bytes_kind": "f64"}, "unsupported Paramdex field type")):
            with self.subTest(message=message):
                with self.assertRaisesRegex(ValueError, message):
                    load_param_definition(self.write_definition(**kwargs))


if __name__ == "__main__":
    unittest.main()
