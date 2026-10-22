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
    weapon_ar_for_ammunition: bool
    class_budget: bool
    status_buildup: bool
    weapon_passives: bool
    aow_compatibility: bool
    aow_damage: bool
    aow_routes: bool

    def as_manifest_dict(self) -> dict[str, bool]:
        return {
            "weaponAr": self.weapon_ar,
            "weaponArForAmmunition": self.weapon_ar_for_ammunition,
            "classBudget": self.class_budget,
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
    somber_reinforce_types: frozenset[int]
    regulation_sha256: str
    capabilities: ProfileCapabilities
    rules: ProfileRules
    use_workbook_weapon_metadata: bool
    weapon_name_overrides: Mapping[int, str]
    sword_art_name_overrides: Mapping[int, str]
    weapon_name_xmls: tuple[Path, ...] = ()
    arts_name_xmls: tuple[Path, ...] = ()
    # GR_MenuText FMG XML naming the classes; required with class_catalog_from_regulation.
    class_name_xml: Path | None = None
    version_file: Path | None = None
    weapon_reference_path: Path | None = None
    # Raised when regenerated content changes for the same game and mod version.
    dataset_revision: int = 1
    class_catalog_from_regulation: bool = False

    @property
    def regulation_path(self) -> Path:
        return self.raw_dir / "regulation.bin"

    @property
    def dataset_version(self) -> str:
        version = f"{self.id}-{self.mod_version or self.game_version}"
        return version if self.dataset_revision == 1 else f"{version}-r{self.dataset_revision}"

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
        game_version="1.17.2",
        mod_version=None,
        raw_dir=Path("data/raw/Vanilla"),
        output_dir=Path("data/phase1"),
        affinity_by_slot=VANILLA_AFFINITIES,
        # 3000 is the unupgradeable Meteorite Staff, historically flagged somber.
        somber_reinforce_types=frozenset({2200, 2400, 3000, 3200, 3300, 3400, 8300, 8500}),
        regulation_sha256="b8cdab84c0772f66811fdc5176a381877ca6657b37d420cc2c1562a90d63870e",
        capabilities=ProfileCapabilities(
            weapon_ar=True,
            weapon_ar_for_ammunition=True,
            class_budget=True,
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
        weapon_name_overrides={
            3_560_000: "Leontiel's Greatsword",
            8_530_000: "Hefty Scimitar",
            13_510_000: "Golden Order Flail",
            31_540_000: "Silver Grooved Shield",
            62_520_000: "Ritual Thrusting Shield",
            64_530_000: "Reverse-Bladed Sword",
            66_530_000: "Reed Great Katana",
            67_530_000: "Idus Sword",
        },
        sword_art_name_overrides={
            1_200: "Muleta",
            1_201: "Causality's Wrath",
            8_000: "Shield Strike",
        },
    ),
    "convergence": ProfileDefinition(
        id="convergence",
        display_name="The Convergence",
        game_version="1.17.1",
        mod_version="3.0.3.0",
        raw_dir=Path("data/raw/Conv"),
        output_dir=Path("data/profiles/convergence"),
        affinity_by_slot=CONVERGENCE_AFFINITIES,
        somber_reinforce_types=frozenset({2200, 2400}),
        regulation_sha256="7bcd46b957ac49cfe4e9ad2a31a090a8bcc1d3681d3829c1f5aa4fd114530838",
        capabilities=ProfileCapabilities(
            weapon_ar=True,
            weapon_ar_for_ammunition=False,
            class_budget=True,
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
            zero_attack_element_uses_weapon_scaling=False,
            extended_scaling_grades=True,
            status_buildup_scales=False,
        ),
        use_workbook_weapon_metadata=False,
        weapon_name_overrides={},
        sword_art_name_overrides={},
        weapon_name_xmls=(
            Path("data/raw/Conv/WeaponName.fmg.xml"),
            Path("data/raw/Conv/WeaponName_dlc01.fmg.xml"),
        ),
        arts_name_xmls=(
            Path("data/raw/Conv/ArtsName.fmg.xml"),
            Path("data/raw/Conv/ArtsName_dlc01.fmg.xml"),
        ),
        class_name_xml=Path("data/raw/Conv/GR_MenuText.fmg.xml"),
        version_file=Path("data/raw/Conv/version.txt"),
        weapon_reference_path=Path("data/reference/convergence-3.0.3.0-weapons.json"),
        dataset_revision=2,
        class_catalog_from_regulation=True,
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
    candidates = sorted(root.glob("data/raw/WitchyBND-*/WitchyBND.exe"))
    if len(candidates) != 1:
        raise FileNotFoundError(
            f"expected one WitchyBND.exe under data/raw/WitchyBND-*/, found {len(candidates)}; "
            "pass --witchybnd <path>"
        )
    return candidates[0]
