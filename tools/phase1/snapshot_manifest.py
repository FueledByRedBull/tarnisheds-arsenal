from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from datetime import date
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Mapping, TypedDict, cast

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.extract_motion_workbook import MOTION_WORKBOOK_NAME  # noqa: E402
from tools.phase1.profiles import ProfileDefinition, profile_definition  # noqa: E402


SCHEMA_VERSION = 4
MODEL_VERSION = "aow-routes-effects-v5-compact-compatibility"
EXTRACTOR_VERSION = "phase1-python-v9-compact-compatibility"
RUNTIME_FILES = {
    "aow.csv",
    "aow_attack_data.csv",
    "aow_effect_data.csv",
    "aow_route_assignments.csv",
    "attack_element_correct.csv",
    "attack_element_correct_ext.csv",
    "calc_correct.csv",
    "native_skill_attack_data.csv",
    "reinforce.csv",
    "weapon_passive_overlays.csv",
    "weapon_passives.csv",
    "weapons.csv",
}
IN_MEMORY_DIAGNOSTICS = {"aow_affinity_compat.csv", "weapon_scaling_summary.csv"}


class FileRecord(TypedDict):
    path: str
    size: int
    sha256: str


class SourceRecord(FileRecord):
    kind: str
    bundled: bool


class SnapshotManifest(TypedDict):
    schemaVersion: int
    datasetVersion: str
    modelVersion: str
    id: str
    label: str
    appVersion: str
    profile: dict[str, str | None]
    capabilities: dict[str, bool]
    rules: dict[str, bool | int]
    generatedAt: str
    extractorVersion: str
    provenance: str
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


def _validate_snapshot_file_name(value: object) -> str:
    if (
        not isinstance(value, str)
        or not value
        or PurePosixPath(value).name != value
        or PureWindowsPath(value).name != value
    ):
        raise ValueError(f"unsafe snapshot file path: {value!r}")
    return value


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
    profile: ProfileDefinition,
    source_paths: Mapping[str, Path] | None = None,
    generated_at: str | None = None,
) -> Path:
    phase1_dir = phase1_dir.resolve()
    regulation_path = regulation_path.resolve()
    workbook_path = phase1_dir / MOTION_WORKBOOK_NAME
    missing_runtime = sorted(name for name in RUNTIME_FILES if not (phase1_dir / name).is_file())
    if missing_runtime:
        raise FileNotFoundError(
            "cannot write snapshot manifest; missing runtime files: "
            + ", ".join(missing_runtime)
        )
    if (profile.capabilities.aow_damage or profile.capabilities.aow_routes) and not workbook_path.is_file():
        raise FileNotFoundError(f"workbook source not found: {workbook_path}")
    if not regulation_path.is_file():
        raise FileNotFoundError(f"regulation source not found: {regulation_path}")

    runtime_files = [_file_record(phase1_dir / name) for name in sorted(RUNTIME_FILES)]
    diagnostic_files = [
        _file_record(path)
        for path in sorted(phase1_dir.glob("*.csv"), key=lambda item: item.name)
        if path.name not in RUNTIME_FILES | IN_MEMORY_DIAGNOSTICS
    ]
    sources: list[SourceRecord] = [
        {"kind": "regulation", "bundled": False, **_file_record(regulation_path)},
    ]
    if workbook_path.is_file():
        sources.append({"kind": "workbook", "bundled": True, **_file_record(workbook_path)})
    for kind, path in sorted((source_paths or {}).items()):
        resolved = path.resolve()
        if not resolved.is_file():
            raise FileNotFoundError(f"{kind} source not found: {resolved}")
        sources.append({"kind": kind, "bundled": False, **_file_record(resolved)})

    version_label = profile.mod_version or profile.game_version
    provenance = "profile-bound regulation, names, and numeric PARAM effect graph"
    if workbook_path.is_file():
        provenance = "profile-bound regulation, names, motion data, and numeric PARAM effect graph"
    manifest = {
        "schemaVersion": SCHEMA_VERSION,
        "datasetVersion": profile.dataset_version,
        "modelVersion": MODEL_VERSION,
        "id": profile.dataset_version,
        "label": f"{profile.display_name} dataset - {version_label}",
        "appVersion": profile.game_version,
        "source": regulation_path.name,
        "profile": profile.as_manifest_dict(),
        "capabilities": profile.capabilities.as_manifest_dict(),
        "rules": profile.rules.as_manifest_dict(),
        "generatedAt": generated_at or date.today().isoformat(),
        "extractorVersion": EXTRACTOR_VERSION,
        "provenance": provenance,
        "runtimeFiles": runtime_files,
        "diagnosticFiles": diagnostic_files,
        "sources": sources,
    }
    output_path = phase1_dir / "manifest.json"
    temporary_path = phase1_dir / "manifest.json.tmp"
    temporary_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    temporary_path.replace(output_path)
    return output_path


def validate_snapshot_manifest(
    phase1_dir: Path,
    expected_profile: ProfileDefinition | None = None,
) -> SnapshotManifest:
    phase1_dir = phase1_dir.resolve()
    manifest_path = phase1_dir / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schemaVersion") != SCHEMA_VERSION:
        raise ValueError(
            f"snapshot schema is {manifest.get('schemaVersion')!r}; expected {SCHEMA_VERSION}"
        )
    if manifest.get("modelVersion") != MODEL_VERSION:
        raise ValueError("snapshot modelVersion does not match the extractor")
    if manifest.get("extractorVersion") != EXTRACTOR_VERSION:
        raise ValueError("snapshot extractorVersion does not match the extractor")
    profile_record = manifest.get("profile")
    capabilities = manifest.get("capabilities")
    rules = manifest.get("rules")
    if (
        not isinstance(profile_record, dict)
        or not isinstance(capabilities, dict)
        or not isinstance(rules, dict)
    ):
        raise ValueError("snapshot profile, capabilities, or rules are malformed")
    profile_id = profile_record.get("id")
    if not isinstance(profile_id, str) or not profile_id.strip():
        raise ValueError("snapshot profile id is missing")
    if expected_profile is not None:
        if profile_record != expected_profile.as_manifest_dict():
            raise ValueError("snapshot profile metadata does not match the selected profile")
        if capabilities != expected_profile.capabilities.as_manifest_dict():
            raise ValueError("snapshot capabilities do not match the selected profile")
        if rules != expected_profile.rules.as_manifest_dict():
            raise ValueError("snapshot rules do not match the selected profile")
        if manifest.get("datasetVersion") != expected_profile.dataset_version:
            raise ValueError("snapshot datasetVersion does not match the selected profile")
    expected_capability_keys = {
        "weaponAr",
        "weaponArForAmmunition",
        "classBudget",
        "statusBuildup",
        "weaponPassives",
        "aowCompatibility",
        "aowDamage",
        "aowRoutes",
    }
    if set(capabilities) != expected_capability_keys or not all(
        isinstance(value, bool) for value in capabilities.values()
    ):
        raise ValueError("snapshot capability set is not exact")
    expected_rule_keys = {
        "standardMaxUpgrade",
        "somberMaxUpgrade",
        "separateUpgradeCaps",
        "scadutreeScaling",
        "zeroAttackElementUsesWeaponScaling",
        "extendedScalingGrades",
        "statusBuildupScales",
    }
    if set(rules) != expected_rule_keys:
        raise ValueError("snapshot rule set is not exact")
    for key in ("standardMaxUpgrade", "somberMaxUpgrade"):
        if not isinstance(rules[key], int) or isinstance(rules[key], bool) or not 0 <= rules[key] <= 25:
            raise ValueError(f"snapshot {key} rule is malformed")
    for key in expected_rule_keys - {"standardMaxUpgrade", "somberMaxUpgrade"}:
        if not isinstance(rules[key], bool):
            raise ValueError(f"snapshot {key} rule is malformed")

    runtime_records = manifest.get("runtimeFiles")
    diagnostic_records = manifest.get("diagnosticFiles")
    if not isinstance(runtime_records, list) or not isinstance(diagnostic_records, list):
        raise ValueError("snapshot manifest file lists are malformed")
    all_records = runtime_records + diagnostic_records
    if not all(isinstance(record, dict) for record in all_records):
        raise ValueError("snapshot manifest file records are malformed")
    runtime_paths = {record.get("path") for record in runtime_records}
    if runtime_paths != RUNTIME_FILES or len(runtime_records) != len(RUNTIME_FILES):
        raise ValueError("snapshot runtime file set is not exact")

    listed_paths: set[str] = set()
    for record in all_records:
        relative = _validate_snapshot_file_name(record.get("path"))
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
    if not all(isinstance(record, dict) for record in sources):
        raise ValueError("snapshot source records are malformed")
    source_by_kind = {record.get("kind"): record for record in sources}
    if len(source_by_kind) != len(sources) or "regulation" not in source_by_kind:
        raise ValueError("snapshot source kinds must be unique and include regulation")
    if (capabilities["aowDamage"] or capabilities["aowRoutes"]) and "workbook" not in source_by_kind:
        raise ValueError("AoW-capable snapshots require a motion workbook source")
    for kind, record in source_by_kind.items():
        source_name = _validate_snapshot_file_name(record.get("path"))
        if not isinstance(record.get("bundled"), bool):
            raise ValueError(f"snapshot {kind} source bundled flag is malformed")
        sha256 = record.get("sha256")
        if (
            not isinstance(sha256, str)
            or len(sha256) != 64
            or any(character not in "0123456789abcdef" for character in sha256.lower())
        ):
            raise ValueError(f"snapshot {kind} source SHA-256 is malformed")
        if record["bundled"]:
            source_path = phase1_dir / source_name
            if (
                not source_path.is_file()
                or record.get("size") != source_path.stat().st_size
                or record.get("sha256") != _sha256(source_path)
            ):
                raise ValueError(f"snapshot bundled {kind} source hash is invalid")
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
    for record in manifest["sources"]:
        if record["bundled"]:
            file_records[str(record["path"])] = record

    for file_name, record in file_records.items():
        source = staging_dir / file_name
        destination = destination_dir / file_name
        if _matches_record(destination, record):
            continue
        temporary = destination.with_name(f".{destination.name}.snapshot.tmp")
        shutil.copy2(source, temporary)
        temporary.replace(destination)

    for stale_csv in destination_dir.glob("*.csv"):
        if stale_csv.name not in file_records:
            stale_csv.unlink()

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
    parser.add_argument("--profile", default="vanilla")
    parser.add_argument("--phase1", type=Path)
    parser.add_argument("--regulation", type=Path)
    parser.add_argument("--generated-at")
    args = parser.parse_args()
    profile = profile_definition(args.profile)
    output = write_snapshot_manifest(
        args.phase1 or profile.output_dir,
        args.regulation or profile.regulation_path,
        profile=profile,
        generated_at=args.generated_at,
    )
    print(f"Wrote snapshot manifest to {output}")


if __name__ == "__main__":
    main()
