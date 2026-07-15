from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from datetime import date
from pathlib import Path
from typing import Mapping, TypedDict, cast


SCHEMA_VERSION = 1
DATASET_VERSION = "phase1-app-1.16.1"
MODEL_VERSION = "aow-routes-effects-v1"
EXTRACTOR_VERSION = "phase1-python-v2"
RUNTIME_FILES = {
    "aow.csv",
    "aow_attack_data.csv",
    "aow_effect_data.csv",
    "aow_route_assignments.csv",
    "aow_weapon_compat.csv",
    "attack_element_correct.csv",
    "attack_element_correct_ext.csv",
    "calc_correct.csv",
    "native_skill_attack_data.csv",
    "reinforce.csv",
    "weapon_passive_overlays.csv",
    "weapon_passives.csv",
    "weapons.csv",
}


class FileRecord(TypedDict):
    path: str
    size: int
    sha256: str


class SourceRecord(FileRecord):
    kind: str


class SnapshotManifest(TypedDict):
    runtimeFiles: list[FileRecord]
    diagnosticFiles: list[FileRecord]
    sources: list[SourceRecord]


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _file_record(path: Path) -> FileRecord:
    return {
        "path": path.name,
        "size": path.stat().st_size,
        "sha256": _sha256(path),
    }


def _matches_record(path: Path, record: Mapping[str, object]) -> bool:
    return (
        path.is_file()
        and record.get("size") == path.stat().st_size
        and record.get("sha256") == _sha256(path)
    )


def write_snapshot_manifest(
    phase1_dir: Path,
    regulation_path: Path,
    *,
    generated_at: str | None = None,
) -> Path:
    phase1_dir = phase1_dir.resolve()
    regulation_path = regulation_path.resolve()
    workbook_path = phase1_dir / "ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx"
    missing_runtime = sorted(name for name in RUNTIME_FILES if not (phase1_dir / name).is_file())
    if missing_runtime:
        raise FileNotFoundError(
            "cannot write snapshot manifest; missing runtime files: "
            + ", ".join(missing_runtime)
        )
    if not workbook_path.is_file():
        raise FileNotFoundError(f"workbook source not found: {workbook_path}")
    if not regulation_path.is_file():
        raise FileNotFoundError(f"regulation source not found: {regulation_path}")

    runtime_files = [_file_record(phase1_dir / name) for name in sorted(RUNTIME_FILES)]
    diagnostic_files = [
        _file_record(path)
        for path in sorted(phase1_dir.glob("*.csv"), key=lambda item: item.name)
        if path.name not in RUNTIME_FILES
    ]
    manifest = {
        "schemaVersion": SCHEMA_VERSION,
        "datasetVersion": DATASET_VERSION,
        "modelVersion": MODEL_VERSION,
        "id": DATASET_VERSION,
        "label": "Phase 1 dataset - App Ver. 1.16.1",
        "appVersion": "1.16.1",
        "source": workbook_path.name,
        "generatedAt": generated_at or date.today().isoformat(),
        "extractorVersion": EXTRACTOR_VERSION,
        "provenance": "regulation.bin + verified motion workbook + numeric PARAM effect graph",
        "runtimeFiles": runtime_files,
        "diagnosticFiles": diagnostic_files,
        "sources": [
            {"kind": "regulation", **_file_record(regulation_path)},
            {"kind": "workbook", **_file_record(workbook_path)},
        ],
    }
    output_path = phase1_dir / "manifest.json"
    temporary_path = phase1_dir / "manifest.json.tmp"
    temporary_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    temporary_path.replace(output_path)
    return output_path


def validate_snapshot_manifest(phase1_dir: Path) -> SnapshotManifest:
    phase1_dir = phase1_dir.resolve()
    manifest_path = phase1_dir / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schemaVersion") != SCHEMA_VERSION:
        raise ValueError(
            f"snapshot schema is {manifest.get('schemaVersion')!r}; expected {SCHEMA_VERSION}"
        )
    if manifest.get("datasetVersion") != DATASET_VERSION:
        raise ValueError("snapshot datasetVersion does not match the extractor")
    if manifest.get("modelVersion") != MODEL_VERSION:
        raise ValueError("snapshot modelVersion does not match the extractor")
    if manifest.get("extractorVersion") != EXTRACTOR_VERSION:
        raise ValueError("snapshot extractorVersion does not match the extractor")

    runtime_records = manifest.get("runtimeFiles")
    diagnostic_records = manifest.get("diagnosticFiles")
    if not isinstance(runtime_records, list) or not isinstance(diagnostic_records, list):
        raise ValueError("snapshot manifest file lists are malformed")
    runtime_paths = {record.get("path") for record in runtime_records}
    if runtime_paths != RUNTIME_FILES or len(runtime_records) != len(RUNTIME_FILES):
        raise ValueError("snapshot runtime file set is not exact")

    all_records = runtime_records + diagnostic_records
    listed_paths: set[str] = set()
    for record in all_records:
        relative = record.get("path")
        if not isinstance(relative, str) or Path(relative).name != relative:
            raise ValueError(f"unsafe snapshot file path: {relative!r}")
        if relative in listed_paths:
            raise ValueError(f"duplicate snapshot file record: {relative}")
        listed_paths.add(relative)
        path = phase1_dir / relative
        if not path.is_file():
            raise ValueError(f"snapshot file is missing: {relative}")
        if record.get("size") != path.stat().st_size:
            raise ValueError(f"snapshot file size mismatch: {relative}")
        if record.get("sha256") != _sha256(path):
            raise ValueError(f"snapshot file SHA-256 mismatch: {relative}")

    actual_csvs = {path.name for path in phase1_dir.glob("*.csv")}
    if actual_csvs != listed_paths:
        missing = sorted(listed_paths - actual_csvs)
        unlisted = sorted(actual_csvs - listed_paths)
        raise ValueError(f"snapshot CSV set mismatch: missing={missing}, unlisted={unlisted}")

    sources = manifest.get("sources")
    if not isinstance(sources, list):
        raise ValueError("snapshot source records are malformed")
    source_by_kind = {record.get("kind"): record for record in sources}
    if set(source_by_kind) != {"regulation", "workbook"} or len(sources) != 2:
        raise ValueError("snapshot must contain exactly regulation and workbook source hashes")
    workbook = source_by_kind["workbook"]
    workbook_path = phase1_dir / str(workbook.get("path", ""))
    if (
        not workbook_path.is_file()
        or workbook.get("size") != workbook_path.stat().st_size
        or workbook.get("sha256") != _sha256(workbook_path)
    ):
        raise ValueError("snapshot workbook source hash is invalid")
    for kind, record in source_by_kind.items():
        sha256 = record.get("sha256")
        if (
            not isinstance(sha256, str)
            or len(sha256) != 64
            or any(character not in "0123456789abcdef" for character in sha256.lower())
        ):
            raise ValueError(f"snapshot {kind} source SHA-256 is malformed")
    return cast(SnapshotManifest, manifest)


def promote_snapshot(staging_dir: Path, destination_dir: Path) -> None:
    """Promote a validated snapshot, replacing its manifest last.

    File replacement is intentionally not presented as a filesystem transaction. During
    promotion the old manifest cannot validate against partially replaced data, so runtime
    loaders fail closed until the new manifest is installed as the commit marker.
    """
    staging_dir = staging_dir.resolve()
    destination_dir = destination_dir.resolve()
    manifest = validate_snapshot_manifest(staging_dir)
    destination_dir.mkdir(parents=True, exist_ok=True)

    records = manifest["runtimeFiles"] + manifest["diagnosticFiles"]
    file_records = {str(record["path"]): record for record in records}
    workbook_record = next(
        record for record in manifest["sources"] if record["kind"] == "workbook"
    )
    file_records[str(workbook_record["path"])] = workbook_record

    for file_name, record in file_records.items():
        source = staging_dir / file_name
        destination = destination_dir / file_name
        if _matches_record(destination, record):
            continue
        temporary = destination.with_name(f".{destination.name}.snapshot.tmp")
        shutil.copy2(source, temporary)
        temporary.replace(destination)

    manifest_source = staging_dir / "manifest.json"
    manifest_destination = destination_dir / "manifest.json"
    if manifest_destination.is_file() and manifest_source.read_bytes() == manifest_destination.read_bytes():
        validate_snapshot_manifest(destination_dir)
        return
    manifest_temporary = destination_dir / ".manifest.json.snapshot.tmp"
    shutil.copy2(manifest_source, manifest_temporary)
    manifest_temporary.replace(manifest_destination)
    validate_snapshot_manifest(destination_dir)


def main() -> None:
    parser = argparse.ArgumentParser(description="Write the atomic Phase 1 snapshot manifest.")
    parser.add_argument("--phase1", type=Path, default=Path("data/phase1"))
    parser.add_argument("--regulation", type=Path, default=Path("data/raw/regulation.bin"))
    parser.add_argument("--generated-at")
    args = parser.parse_args()
    output = write_snapshot_manifest(
        args.phase1,
        args.regulation,
        generated_at=args.generated_at,
    )
    print(f"Wrote snapshot manifest to {output}")


if __name__ == "__main__":
    main()
