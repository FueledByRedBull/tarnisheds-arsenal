from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Mapping


VANILLA_AFFINITIES: Mapping[int, str] = {
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

CONVERGENCE_AFFINITIES: Mapping[int, str] = {
    0: "Standard",
    1: "Heavy",
    2: "Keen",
    3: "Quality",
    4: "Glint",
    5: "Dragonkin",
    6: "Gravity",
    7: "Flame",
    8: "Golden",
    9: "Draconic",
    10: "Bestial",
    11: "Night",
    12: "Lava",
    13: "Frenzy",
    14: "Death",
    15: "Godslayer",
    16: "Frost",
    17: "Aberrant",
    18: "Bloodflame",
    19: "Rotten",
    20: "Storm",
    21: "Mystic",
}


@dataclass(frozen=True)
class ProfileCapabilities:
    weapon_ar: bool
    status_buildup: bool
    weapon_passives: bool
    aow_compatibility: bool
    aow_damage: bool
    aow_routes: bool

    def as_manifest_dict(self) -> dict[str, bool]:
        return {
            "weaponAr": self.weapon_ar,
            "statusBuildup": self.status_buildup,
            "weaponPassives": self.weapon_passives,
            "aowCompatibility": self.aow_compatibility,
            "aowDamage": self.aow_damage,
            "aowRoutes": self.aow_routes,
        }


@dataclass(frozen=True)
class ProfileRules:
    standard_max_upgrade: int
    somber_max_upgrade: int
    separate_upgrade_caps: bool
    scadutree_scaling: bool
    zero_attack_element_uses_weapon_scaling: bool
    extended_scaling_grades: bool
    status_buildup_scales: bool

    def as_manifest_dict(self) -> dict[str, bool | int]:
        return {
            "standardMaxUpgrade": self.standard_max_upgrade,
            "somberMaxUpgrade": self.somber_max_upgrade,
            "separateUpgradeCaps": self.separate_upgrade_caps,
            "scadutreeScaling": self.scadutree_scaling,
            "zeroAttackElementUsesWeaponScaling": self.zero_attack_element_uses_weapon_scaling,
            "extendedScalingGrades": self.extended_scaling_grades,
            "statusBuildupScales": self.status_buildup_scales,
        }


@dataclass(frozen=True)
class ProfileDefinition:
    id: str
    display_name: str
    game_version: str
    mod_version: str | None
    raw_dir: Path
    output_dir: Path
    affinity_by_slot: Mapping[int, str]
    capabilities: ProfileCapabilities
    rules: ProfileRules
    use_workbook_weapon_metadata: bool
    allow_param_weapon_names: bool
    weapon_name_xmls: tuple[Path, ...] = ()
    arts_name_xmls: tuple[Path, ...] = ()
    version_file: Path | None = None
    weapon_reference_path: Path | None = None

    @property
    def regulation_path(self) -> Path:
        return self.raw_dir / "regulation.bin"

    @property
    def dataset_version(self) -> str:
        if self.mod_version:
            return f"{self.id}-{self.mod_version}"
        return f"{self.id}-{self.game_version}"

    def as_manifest_dict(self) -> dict[str, str | None]:
        return {
            "id": self.id,
            "displayName": self.display_name,
            "gameVersion": self.game_version,
            "modVersion": self.mod_version,
        }


PROFILES: Mapping[str, ProfileDefinition] = {
    "vanilla": ProfileDefinition(
        id="vanilla",
        display_name="Vanilla",
        game_version="1.17",
        mod_version=None,
        raw_dir=Path("data/raw/Vanilla"),
        output_dir=Path("data/phase1"),
        affinity_by_slot=VANILLA_AFFINITIES,
        capabilities=ProfileCapabilities(
            weapon_ar=True,
            status_buildup=True,
            weapon_passives=True,
            aow_compatibility=True,
            aow_damage=True,
            aow_routes=True,
        ),
        rules=ProfileRules(
            standard_max_upgrade=25,
            somber_max_upgrade=10,
            separate_upgrade_caps=True,
            scadutree_scaling=True,
            zero_attack_element_uses_weapon_scaling=False,
            extended_scaling_grades=False,
            status_buildup_scales=True,
        ),
        use_workbook_weapon_metadata=True,
        allow_param_weapon_names=True,
    ),
    "convergence": ProfileDefinition(
        id="convergence",
        display_name="The Convergence",
        game_version="1.16.1",
        mod_version="3.0.0.1",
        raw_dir=Path("data/raw/Conv"),
        output_dir=Path("data/profiles/convergence"),
        affinity_by_slot=CONVERGENCE_AFFINITIES,
        capabilities=ProfileCapabilities(
            weapon_ar=True,
            status_buildup=True,
            weapon_passives=True,
            aow_compatibility=True,
            aow_damage=False,
            aow_routes=False,
        ),
        rules=ProfileRules(
            standard_max_upgrade=15,
            somber_max_upgrade=15,
            separate_upgrade_caps=False,
            scadutree_scaling=False,
            zero_attack_element_uses_weapon_scaling=True,
            extended_scaling_grades=True,
            status_buildup_scales=False,
        ),
        use_workbook_weapon_metadata=False,
        allow_param_weapon_names=True,
        weapon_name_xmls=(
            Path("data/raw/Conv/WeaponName.fmg.xml"),
            Path("data/raw/Conv/WeaponName_dlc01.fmg.xml"),
        ),
        arts_name_xmls=(
            Path("data/raw/Conv/ArtsName.fmg.xml"),
            Path("data/raw/Conv/ArtsName_dlc01.fmg.xml"),
        ),
        version_file=Path("data/raw/Conv/version.txt"),
        weapon_reference_path=Path("data/reference/convergence-3.0.0.1-weapons.json"),
    ),
}

PROFILE_ALIASES = {
    "conv": "convergence",
    "convergence": "convergence",
    "vanilla": "vanilla",
}


def profile_definition(value: str) -> ProfileDefinition:
    normalized = value.strip().lower()
    profile_id = PROFILE_ALIASES.get(normalized)
    if profile_id is None:
        choices = ", ".join(sorted(PROFILES))
        raise ValueError(f"unknown game profile {value!r}; expected one of: {choices}")
    return PROFILES[profile_id]


def discover_witchybnd(root: Path) -> Path:
    candidates = sorted(root.glob("data/raw/WitchyBND-*/WitchyBND.exe"), reverse=True)
    if not candidates:
        raise FileNotFoundError(
            "WitchyBND.exe was not found under data/raw/WitchyBND-*/; "
            "pass --witchybnd <path>"
        )
    return candidates[0]
