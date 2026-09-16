from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import random
import subprocess
import tempfile
import unicodedata
import urllib.request
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterator

ROOT = Path(__file__).resolve().parents[2]
STATS = ("str", "dex", "int", "fai", "arc")
STAT_CHOICES = (10, 15, 20, 25, 30, 40, 45, 50, 60, 70, 80, 99)

REFERENCE_REPOSITORY = "https://github.com/ThomasJClark/elden-ring-weapon-calculator"
REFERENCE_COMMIT = "b8a1cf8847fe67aacc7f8fcb038a9cfd6725f19a"
REFERENCE_RAW_URL = f"https://raw.githubusercontent.com/ThomasJClark/elden-ring-weapon-calculator/{REFERENCE_COMMIT}"
REFERENCE_FILES = {
    "COPYING": "a0809e15daa0ec44337939232dc8de895234932bea405870ed9a1eac9456befe",
    "src/regulationData.ts": "1949a2f85ef0c0439660c6711dd55c0d008951318e908c68caf26e98ca8c072c",
    "src/calculator/calculator.ts": "6b3733e28f50403f13d5915ec2a48e6774f1ff144203ead522153853bff99f46",
    "src/calculator/weapon.ts": "64e6da031744a80f35eb6ed5fe5342c066247a3f5a3d45ad75acc414e1874027",
    "src/calculator/attributes.ts": "64d8feb29cbe6f3c8230b4298cb55db73da23d624fe887771a922c09a899d439",
    "src/calculator/attackPowerTypes.ts": "5d62367b87ea27d697710e6742c03c185860e206d1225160553a6a19bbabefd4",
    "src/calculator/weaponTypes.ts": "b38f3710e186c5d455eaa133f0bd562cdd47696286d5d479021df8b912e248e8",
}
REGULATION_URL = (
    "https://raw.githubusercontent.com/ThomasJClark/elden-ring-weapon-calculator/"
    f"{REFERENCE_COMMIT}/public/regulation-vanilla-v1.17.js"
)
REGULATION_FILE = "public/regulation-vanilla-v1.17.js"
REGULATION_SHA256 = "6ae98b99429cf63f3f2c6a737cf90f6e7507fa67c1dd7e45cad5937a2f3824a5"
AFFINITIES = {
    -1: "Standard",
    0: "Standard",
    1: "Heavy",
    2: "Keen",
    3: "Quality",
    4: "Fire",
    5: "Flame Art",
    6: "Lightning",
    7: "Sacred",
    8: "Magic",
    9: "Cold",
    10: "Poison",
    11: "Blood",
    12: "Occult",
}


@dataclass(frozen=True)
class ComparisonCase:
    case_id: int
    sample_weapon: int
    external_index: int
    weapon_name: str
    affinity: str
    aow_name: str | None
    upgrade: int
    stats: dict[str, int]
    two_handing: bool

    def payload(self) -> dict[str, Any]:
        return {
            "caseId": self.case_id,
            "externalIndex": self.external_index,
            "weaponName": self.weapon_name,
            "affinity": self.affinity,
            "aowName": self.aow_name,
            "upgrade": self.upgrade,
            "stats": self.stats,
            "twoHanding": self.two_handing,
        }


def normalize_name(value: str) -> str:
    value = value.replace("’", "'").replace("–", "-").replace("—", "-")
    return "".join(
        char for char in unicodedata.normalize("NFKD", value) if not unicodedata.combining(char)
    ).casefold()


def floor_status(value: float) -> int:
    return math.floor(value + 1e-9)


def _as_bool(value: Any) -> bool:
    return value is True or str(value).strip().lower() in {"1", "true", "yes"}


def _requirements(value: Any) -> dict[str, int]:
    if isinstance(value, dict):
        return {stat: int(value.get(stat, 0)) for stat in STATS}
    if isinstance(value, (list, tuple)):
        return {stat: int(value[index]) for index, stat in enumerate(STATS)}
    return dict.fromkeys(STATS, 0)


def _affinity_name(value: Any) -> str:
    try:
        return AFFINITIES[int(value)]
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"unknown external affinity id: {value!r}") from error


def _external_max_upgrade(weapon: dict[str, Any]) -> int:
    if "maxUpgrade" in weapon:
        return int(weapon["maxUpgrade"])
    return int(weapon["attackLength"]) - 1


def _local_key(name: str, affinity: str) -> tuple[str, str]:
    return normalize_name(name), affinity.casefold()


def select_weapon_names(
    external_weapons: list[dict[str, Any]],
    local_catalog: list[dict[str, Any]],
    count: int,
    rng: random.Random,
) -> list[str]:
    local_names = {
        normalize_name(str(row["name"]))
        for row in local_catalog
        if _as_bool(row.get("supported", True))
    }
    candidates = sorted(
        {
            str(weapon["weaponName"])
            for weapon in external_weapons
            if str(weapon.get("weaponName", "")) != "Unarmed"
            and normalize_name(str(weapon["weaponName"])) in local_names
        },
        key=normalize_name,
    )
    if count < 1:
        raise ValueError("weapon sample count must be positive")
    if len(candidates) < count:
        raise ValueError(f"only {len(candidates)} usable external weapon names; need {count}")
    return rng.sample(candidates, count)


def _unbuffed(skill: dict[str, Any]) -> bool:
    for field in (
        "buff",
        "statusAdd",
        "statusScale",
        "persistentWeaponStatus",
        "persistentOnHitStatus",
    ):
        if any(float(value) != 0.0 for value in skill.get(field, ())):
            return False
    return True


def _choose_skill(local_weapon: dict[str, Any], rng: random.Random) -> str | None:
    skills = []
    for skill in local_weapon.get("ashes", ()):
        if str(skill.get("name", "")) == "No Skill":
            continue
        unbuffed = skill.get("unbuffed")
        if unbuffed is None:
            unbuffed = _unbuffed(skill)
        if unbuffed:
            skills.append(skill)
    return str(rng.choice(skills)["name"]) if skills else None


def _random_stats(
    local_weapon: dict[str, Any], external_weapon: dict[str, Any], rng: random.Random
) -> dict[str, int]:
    local_requirements = _requirements(local_weapon.get("requirements"))
    external_requirements = _requirements(external_weapon.get("requirements"))
    stats = {
        stat: max(10, local_requirements[stat], external_requirements[stat], rng.choice(STAT_CHOICES))
        for stat in STATS
    }
    if any(value > 99 for value in stats.values()):
        raise ValueError(f"external requirements exceed supported stat range: {stats}")
    return stats


def build_cases(
    external_weapons: list[dict[str, Any]],
    local_catalog: list[dict[str, Any]],
    selected_names: list[str],
    rng: random.Random,
) -> list[ComparisonCase]:
    external_by_name: dict[str, list[tuple[int, dict[str, Any]]]] = {}
    for index, weapon in enumerate(external_weapons):
        external_by_name.setdefault(normalize_name(str(weapon["weaponName"])), []).append(
            (index, weapon)
        )
    local_by_config = {
        _local_key(str(row["name"]), str(row["affinity"])): row
        for row in local_catalog
        if _as_bool(row.get("supported", True))
    }
    cases: list[ComparisonCase] = []
    for sample_index, selected_name in enumerate(selected_names, 1):
        configurations: dict[str, tuple[int, dict[str, Any], dict[str, Any]]] = {}
        for index, external in external_by_name.get(normalize_name(selected_name), ()):
            affinity = _affinity_name(external.get("affinityId"))
            local = local_by_config.get(_local_key(selected_name, affinity))
            if local is None:
                continue
            if _external_max_upgrade(external) != int(local["maxUpgrade"]):
                raise ValueError(
                    f"upgrade cap mismatch for {selected_name} / {affinity}: "
                    f"external={_external_max_upgrade(external)} local={local['maxUpgrade']}"
                )
            configurations.setdefault(affinity, (index, external, local))
        if not configurations:
            raise ValueError(f"no usable local configuration for sampled weapon {selected_name}")
        index, external, local = rng.choice(list(configurations.values()))
        max_upgrade = _external_max_upgrade(external)
        for phase in ("zero", "random", "max"):
            upgrade = {
                "zero": 0,
                "random": rng.randint(1, max(1, max_upgrade - 1)) if max_upgrade else 0,
                "max": max_upgrade,
            }[phase]
            stats = _random_stats(local, external, rng)
            skill = _choose_skill(local, rng)
            for two_handing in (False, True):
                cases.append(
                    ComparisonCase(
                        case_id=len(cases) + 1,
                        sample_weapon=sample_index,
                        external_index=index,
                        weapon_name=str(local["name"]),
                        affinity=str(local["affinity"]),
                        aow_name=skill,
                        upgrade=upgrade,
                        stats=stats,
                        two_handing=two_handing,
                    )
                )
    if len({case.sample_weapon for case in cases}) != len(selected_names):
        raise ValueError("case sampler did not preserve unique sampled weapon names")
    return cases


def sample_cases(
    external_weapons: list[dict[str, Any]],
    local_catalog: list[dict[str, Any]],
    count: int = 100,
    seed: int = 20260916,
) -> tuple[list[str], list[ComparisonCase]]:
    rng = random.Random(seed)
    selected = select_weapon_names(external_weapons, local_catalog, count, rng)
    return selected, build_cases(external_weapons, local_catalog, selected, rng)


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _verified(path: Path, digest: str) -> bool:
    return path.is_file() and _sha256(path) == digest


def fetch_bytes(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "Tarnisheds-Arsenal-validator"})
    with urllib.request.urlopen(request, timeout=60) as response:  # noqa: S310
        return response.read()


def _download_sources(tclark_dir: Path) -> None:
    for relative, digest in REFERENCE_FILES.items():
        content = fetch_bytes(f"{REFERENCE_RAW_URL}/{relative}")
        if hashlib.sha256(content).hexdigest() != digest:
            raise ValueError(f"pinned T. Clark source hash mismatch for {relative}")
        destination = tclark_dir / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)


def prepare_reference(workdir: Path) -> tuple[Path, Path]:
    tclark_dir = workdir / "tclark"
    tclark_dir.mkdir(parents=True, exist_ok=True)
    source_ready = all(_verified(tclark_dir / path, digest) for path, digest in REFERENCE_FILES.items())
    if not source_ready:
        _download_sources(tclark_dir)
    regulation = tclark_dir / REGULATION_FILE
    if not regulation.is_file():
        regulation.parent.mkdir(parents=True, exist_ok=True)
        regulation.write_bytes(fetch_bytes(REGULATION_URL))
    if not _verified(regulation, REGULATION_SHA256):
        raise ValueError(f"pinned T. Clark regulation hash mismatch: {regulation}")
    return tclark_dir / "src", regulation


@contextmanager
def work_directory(root: Path, cache_dir: Path | None) -> Iterator[Path]:
    if cache_dir is not None:
        cache_dir = cache_dir.resolve()
        cache_dir.mkdir(parents=True, exist_ok=True)
        yield cache_dir
        return
    temporary_root = root / ".codex-tmp"
    temporary_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="external-calculator-", dir=temporary_root) as path:
        yield Path(path)


NODE_RUNNER = r"""
const fs = require("node:fs");
const path = require("node:path");
const [sourceDir, regulationPath, typescriptPath, compiledDir] = process.argv.slice(1);
const ts = require(typescriptPath);
const sourceFiles = [
  "regulationData.ts",
  "calculator/attributes.ts",
  "calculator/attackPowerTypes.ts",
  "calculator/calculator.ts",
  "calculator/weapon.ts",
  "calculator/weaponTypes.ts",
];
fs.mkdirSync(compiledDir, { recursive: true });
for (const file of sourceFiles) {
  const input = fs.readFileSync(path.join(sourceDir, file), "utf8");
  const rewritten = input.replace(/(from\s+|export\s+\*\s+from\s+)(["'])([^"']+)\.ts\2/g, "$1$2$3.js$2");
  const output = ts.transpileModule(rewritten, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  const destination = path.join(compiledDir, file.replace(/\.ts$/, ".js"));
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.writeFileSync(destination, output);
}
const { decodeRegulationData } = require(path.join(compiledDir, "regulationData.js"));
const getWeaponAttack = require(path.join(compiledDir, "calculator/calculator.js")).default;
const weapons = decodeRegulationData(JSON.parse(fs.readFileSync(regulationPath, "utf8")));
const request = JSON.parse(fs.readFileSync(0, "utf8"));
if (request.mode === "metadata") {
  process.stdout.write(JSON.stringify(weapons.map((weapon, index) => ({
    index,
    name: weapon.name,
    weaponName: weapon.weaponName,
    affinityId: weapon.affinityId,
    requirements: weapon.requirements,
    maxUpgrade: weapon.attack.length - 1,
    paired: Boolean(weapon.paired),
    weaponType: weapon.weaponType,
    dlc: Boolean(weapon.dlc),
    url: weapon.url ?? null,
  }))));
} else if (request.mode === "evaluate") {
  const results = request.cases.map((item) => {
    const weapon = weapons[item.externalIndex];
    if (!weapon) throw new Error(`unknown external weapon index ${item.externalIndex}`);
    const result = getWeaponAttack({
      weapon,
      attributes: item.stats,
      twoHanding: item.twoHanding,
      upgradeLevel: item.upgrade,
    });
    return {
      caseId: item.caseId,
      attackPower: Array.from({ length: 12 }, (_, index) => result.attackPower[index] ?? 0),
      ineffectiveAttributes: result.ineffectiveAttributes,
    };
  });
  process.stdout.write(JSON.stringify(results));
} else {
  throw new Error(`unknown reference runner mode ${request.mode}`);
}
"""


def run_node_reference(
    source_dir: Path,
    regulation: Path,
    root: Path,
    workdir: Path,
    request: dict[str, Any],
) -> Any:
    typescript = root / "apps" / "desktop" / "node_modules" / "typescript"
    if not typescript.is_dir():
        raise RuntimeError(f"installed TypeScript package is missing: {typescript}")
    completed = subprocess.run(
        [
            "node",
            "-e",
            NODE_RUNNER,
            str(source_dir),
            str(regulation),
            str(typescript),
            str(workdir / "compiled"),
        ],
        cwd=root,
        input=json.dumps(request),
        capture_output=True,
        check=False,
        text=True,
        encoding="utf-8",
    )
    if completed.returncode:
        detail = completed.stderr.strip().splitlines()
        raise RuntimeError(
            f"T. Clark Node runner failed with exit code {completed.returncode}: "
            f"{detail[0] if detail else 'no stderr'}"
        )
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("T. Clark Node runner returned invalid JSON") from error


def run_cargo(
    root: Path, data_dir: Path, cases: list[ComparisonCase] | None, details: bool = False, catalog: bool = False
) -> Any:
    command = [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "--locked",
        "--manifest-path",
        str(root / "core" / "er_optimizer_core" / "Cargo.toml"),
        "--example",
        "evaluate_ar",
        "--",
        str(data_dir),
    ]
    if details:
        command.append("--details")
    if catalog:
        command.append("--catalog")
    payload = "" if cases is None else json.dumps([case.payload() for case in cases])
    completed = subprocess.run(
        command,
        cwd=root,
        input=payload,
        capture_output=True,
        check=False,
        text=True,
        encoding="utf-8",
    )
    if completed.returncode:
        detail = completed.stderr.strip().splitlines()
        raise RuntimeError(
            f"evaluate_ar failed with exit code {completed.returncode}: "
            f"{detail[0] if detail else 'no stderr'}"
        )
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("evaluate_ar returned invalid JSON") from error


def _snapshot_identity(data_dir: Path) -> dict[str, Any]:
    manifest = data_dir / "manifest.json"
    if not manifest.is_file():
        return {"path": str(data_dir)}
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    return {
        "profile": payload.get("profile", {}).get("id"),
        "manifestSha256": _sha256(manifest),
        "datasetVersion": payload.get("datasetVersion"),
        "modelVersion": payload.get("modelVersion"),
        "schemaVersion": payload.get("schemaVersion"),
    }


def _finite_values(value: Any, length: int) -> list[float] | None:
    if not isinstance(value, list) or len(value) != length:
        return None
    try:
        values = [float(item) for item in value]
    except (TypeError, ValueError):
        return None
    return values if all(math.isfinite(item) for item in values) else None


def compare_cases(
    cases: list[ComparisonCase], local: list[dict[str, Any]], external: list[dict[str, Any]], tolerance: float
) -> tuple[dict[str, Any], list[dict[str, Any]], list[dict[str, Any]]]:
    if len(local) != len(cases) or len(external) != len(cases):
        raise ValueError(
            f"case result count mismatch: cases={len(cases)} local={len(local)} external={len(external)}"
        )
    failures: list[dict[str, Any]] = []
    rows: list[dict[str, Any]] = []
    ar_errors: list[float] = []
    status_failure_count = 0
    skill_failure_count = 0
    skill_case_count = sum(case.aow_name is not None for case in cases)
    for case, local_result, external_result in zip(cases, local, external, strict=True):
        case_failures: list[str] = []
        if not isinstance(local_result, dict) or not isinstance(external_result, dict):
            case_failures.append("result_shape")
            local_result = local_result if isinstance(local_result, dict) else {}
            external_result = external_result if isinstance(external_result, dict) else {}
        if external_result.get("ineffectiveAttributes"):
            case_failures.append("external_requirements")
        if (
            local_result.get("weapon") != case.weapon_name
            or local_result.get("affinity") != case.affinity
            or local_result.get("upgrade") != case.upgrade
            or local_result.get("stats") != [case.stats[stat] for stat in STATS]
        ):
            case_failures.append("local_identity")
        local_components = _finite_values(local_result.get("arComponents"), 5)
        external_power = _finite_values(external_result.get("attackPower"), 12)
        external_components = external_power[:5] if external_power is not None else None
        if local_components is None or external_components is None:
            case_failures.extend(("ar_shape", "attack_rating"))
            deltas: list[float] = []
        else:
            deltas = [abs(left - right) for left, right in zip(local_components, external_components)]
            ar_errors.extend(deltas)
            if any(delta > tolerance for delta in deltas):
                case_failures.append("attack_rating")
        local_status_values = _finite_values(local_result.get("baseStatus"), 7)
        external_status_values = external_power[5:12] if external_power is not None else None
        local_status = (
            [floor_status(value) for value in local_status_values]
            if local_status_values is not None
            else []
        )
        external_status = (
            [floor_status(value) for value in external_status_values]
            if external_status_values is not None
            else []
        )
        if local_status_values is None or external_status_values is None or local_status != external_status:
            case_failures.append("base_passive_status")
            status_failure_count += 1
        selected_skill = local_result.get("selectedSkill")
        selected = selected_skill.get("name") if isinstance(selected_skill, dict) else None
        if case.aow_name is not None and (not isinstance(selected, str) or selected.casefold() != case.aow_name.casefold()):
            case_failures.append("skill_identity")
            skill_failure_count += 1
        if case_failures:
            failure = {
                "caseId": case.case_id,
                "weapon": case.weapon_name,
                "affinity": case.affinity,
                "upgrade": case.upgrade,
                "twoHanding": case.two_handing,
                "checks": case_failures,
                "arDeltas": deltas,
                "localStatus": local_status,
                "externalStatus": external_status,
            }
            failures.append(failure)
        rows.append(
            {
                "caseId": case.case_id,
                "weapon": case.weapon_name,
                "affinity": case.affinity,
                "upgrade": case.upgrade,
                "twoHanding": case.two_handing,
                "skill": case.aow_name or "",
                "stats": "/".join(str(case.stats[stat]) for stat in STATS),
                "externalAr": sum(external_components) if external_components is not None else None,
                "localAr": sum(local_components) if local_components is not None else None,
                "maxArDelta": max(deltas) if deltas else None,
                "externalStatus": "/".join(map(str, external_status)),
                "localStatus": "/".join(map(str, local_status)),
                "checks": ";".join(case_failures),
            }
        )
    checks = {
        "attackRating": {
            "passed": len(cases) - sum("attack_rating" in failure["checks"] for failure in failures),
            "failed": sum("attack_rating" in failure["checks"] for failure in failures),
            "maxComponentError": max(ar_errors) if ar_errors else None,
            "tolerance": tolerance,
        },
        "basePassiveStatus": {
            "passed": len(cases) - status_failure_count,
            "failed": status_failure_count,
            "comparison": "integer floor",
        },
        "skillIdentity": {
            "checked": skill_case_count,
            "passed": skill_case_count - skill_failure_count,
            "failed": skill_failure_count,
            "notApplicable": len(cases) - skill_case_count,
        },
    }
    return checks, rows, failures


def write_csv(path: Path, rows: list[dict[str, Any]]) -> None:
    if not rows:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


def build_report(
    data_dir: Path,
    source_dir: Path,
    regulation: Path,
    seed: int,
    selected: list[str],
    cases: list[ComparisonCase],
    checks: dict[str, Any],
    failures: list[dict[str, Any]],
) -> dict[str, Any]:
    return {
        "status": "passed" if not failures else "failed",
        "source": {
            "repository": REFERENCE_REPOSITORY,
            "commit": REFERENCE_COMMIT,
            "license": "MIT",
            "regulationUrl": REGULATION_URL,
            "regulationSha256": _sha256(regulation),
            "sourceFiles": {
                relative: _sha256(source_dir / relative.removeprefix("src/"))
                if relative.startswith("src/")
                else _sha256(source_dir.parent / relative)
                for relative in REFERENCE_FILES
            },
        },
        "localSnapshot": _snapshot_identity(data_dir),
        "localSource": {
            "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "corePatchSha256": hashlib.sha256(subprocess.check_output(
                ["git", "diff", "--binary", "HEAD", "--", "core/er_optimizer_core"], cwd=ROOT,
            )).hexdigest(),
        },
        "sampling": {
            "seed": seed,
            "uniqueWeapons": len(selected),
            "cases": len(cases),
            "phases": ["zero", "random", "max"],
            "handing": ["1H", "2H"],
            "weapons": selected,
        },
        "checks": checks,
        "failures": failures,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Compare the local Vanilla AR/passive model with pinned T. Clark 1.17 code."
    )
    parser.add_argument("--seed", type=int, default=20260916)
    parser.add_argument("--count", type=int, default=100)
    parser.add_argument("--data-dir", type=Path, default=ROOT / "data" / "phase1")
    parser.add_argument("--cache-dir", type=Path)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--csv", type=Path)
    parser.add_argument("--tolerance", type=float, default=0.001)
    args = parser.parse_args(argv)
    if not math.isfinite(args.tolerance) or args.tolerance < 0 or args.tolerance > 0.001:
        parser.error("tolerance must be between 0 and 0.001")
    if not args.data_dir.is_dir():
        parser.error(f"data directory is missing: {args.data_dir}")

    try:
        identity = _snapshot_identity(args.data_dir)
        if identity.get("profile") != "vanilla" or identity.get("datasetVersion") != "vanilla-1.17":
            raise ValueError("the pinned reference requires the Vanilla 1.17 dataset")
        with work_directory(ROOT, args.cache_dir) as workdir:
            source_dir, regulation = prepare_reference(workdir)
            external_weapons = run_node_reference(
                source_dir,
                regulation,
                ROOT,
                workdir,
                {"mode": "metadata"},
            )
            local_catalog = run_cargo(ROOT, args.data_dir, None, catalog=True)
            selected, cases = sample_cases(
                external_weapons, local_catalog, count=args.count, seed=args.seed
            )
            local_results = run_cargo(ROOT, args.data_dir, cases, details=True)
            external_results = run_node_reference(
                source_dir,
                regulation,
                ROOT,
                workdir,
                {"mode": "evaluate", "cases": [case.payload() for case in cases]},
            )
            checks, rows, failures = compare_cases(
                cases, local_results, external_results, args.tolerance
            )
            report = build_report(
                args.data_dir,
                source_dir,
                regulation,
                args.seed,
                selected,
                cases,
                checks,
                failures,
            )
            if args.report:
                args.report.parent.mkdir(parents=True, exist_ok=True)
                args.report.write_text(json.dumps(report, separators=(",", ":")), encoding="utf-8")
            if args.csv:
                write_csv(args.csv, rows)
            print(json.dumps(report, separators=(",", ":")))
            return 0 if report["status"] == "passed" else 1
    except (OSError, RuntimeError, ValueError) as error:
        print(json.dumps({"status": "error", "error": str(error)}, separators=(",", ":")))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
