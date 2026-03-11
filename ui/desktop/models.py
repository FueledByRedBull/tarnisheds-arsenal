from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Literal


SearchObjectiveId = Literal[
    "max_ar",
    "max_ar_plus_bleed",
    "aow_first_hit",
    "aow_full_sequence",
]

WorkspaceTab = Literal["rankings", "compare", "paths", "affinity_watch"]
UNSET = object()

STARTING_CLASSES = [
    "Vagabond",
    "Warrior",
    "Hero",
    "Bandit",
    "Astrologer",
    "Prophet",
    "Samurai",
    "Prisoner",
    "Confessor",
    "Wretch",
]

CLASS_BASE_LEVEL_TOTAL = {
    "Vagabond": (9, 88),
    "Warrior": (8, 87),
    "Hero": (7, 86),
    "Bandit": (5, 84),
    "Astrologer": (6, 85),
    "Prophet": (7, 86),
    "Samurai": (9, 88),
    "Prisoner": (9, 88),
    "Confessor": (10, 89),
    "Wretch": (1, 80),
}

CLASS_BASE_STATS = {
    "Vagabond": {"vig": 15, "mnd": 10, "end": 11, "str": 14, "dex": 13, "int": 9, "fai": 9, "arc": 7},
    "Warrior": {"vig": 11, "mnd": 12, "end": 11, "str": 10, "dex": 16, "int": 10, "fai": 8, "arc": 9},
    "Hero": {"vig": 14, "mnd": 9, "end": 12, "str": 16, "dex": 9, "int": 7, "fai": 8, "arc": 11},
    "Bandit": {"vig": 10, "mnd": 11, "end": 10, "str": 9, "dex": 13, "int": 9, "fai": 8, "arc": 14},
    "Astrologer": {"vig": 9, "mnd": 15, "end": 9, "str": 8, "dex": 12, "int": 16, "fai": 7, "arc": 9},
    "Prophet": {"vig": 10, "mnd": 14, "end": 8, "str": 11, "dex": 10, "int": 7, "fai": 16, "arc": 10},
    "Samurai": {"vig": 12, "mnd": 11, "end": 13, "str": 12, "dex": 15, "int": 9, "fai": 8, "arc": 8},
    "Prisoner": {"vig": 11, "mnd": 12, "end": 11, "str": 11, "dex": 14, "int": 14, "fai": 6, "arc": 9},
    "Confessor": {"vig": 10, "mnd": 13, "end": 10, "str": 12, "dex": 12, "int": 9, "fai": 14, "arc": 9},
    "Wretch": {"vig": 10, "mnd": 10, "end": 10, "str": 10, "dex": 10, "int": 10, "fai": 10, "arc": 10},
}


@dataclass(frozen=True)
class CombatState:
    str_stat: int
    dex: int
    int_stat: int
    fai: int
    arc: int

    def add_point(self, stat_key: str) -> CombatState | None:
        field_name = combat_state_attr(stat_key)
        current = getattr(self, field_name)
        if current >= 99:
            return None
        return CombatState(
            str_stat=self.str_stat + (1 if stat_key == "str" else 0),
            dex=self.dex + (1 if stat_key == "dex" else 0),
            int_stat=self.int_stat + (1 if stat_key == "int" else 0),
            fai=self.fai + (1 if stat_key == "fai" else 0),
            arc=self.arc + (1 if stat_key == "arc" else 0),
        )

    def summary(self) -> str:
        return (
            f"STR {self.str_stat}  DEX {self.dex}  INT {self.int_stat}  "
            f"FAI {self.fai}  ARC {self.arc}"
        )


@dataclass(frozen=True)
class LockedCombatStats:
    str_stat: int
    dex: int
    int_stat: int
    fai: int
    arc: int

    def as_optimize_kwargs(self) -> dict[str, int]:
        return {
            "lock_str": self.str_stat,
            "lock_dex": self.dex,
            "lock_int": self.int_stat,
            "lock_fai": self.fai,
            "lock_arc": self.arc,
        }

    def as_combat_state(self) -> CombatState:
        return CombatState(
            str_stat=self.str_stat,
            dex=self.dex,
            int_stat=self.int_stat,
            fai=self.fai,
            arc=self.arc,
        )


@dataclass(frozen=True)
class BuildSession:
    class_name: str
    vig: int
    mnd: int
    end: int
    str_stat: int
    dex: int
    int_stat: int
    fai: int
    arc: int
    min_str: int
    min_dex: int
    min_int: int
    min_fai: int
    min_arc: int
    two_handing: bool

    @property
    def current_stat_sum(self) -> int:
        return (
            self.vig
            + self.mnd
            + self.end
            + self.str_stat
            + self.dex
            + self.int_stat
            + self.fai
            + self.arc
        )

    @property
    def derived_level(self) -> int:
        base_level, base_total = CLASS_BASE_LEVEL_TOTAL[self.class_name]
        return base_level + (self.current_stat_sum - base_total)

    @property
    def current_combat_state(self) -> CombatState:
        return CombatState(
            str_stat=self.str_stat,
            dex=self.dex,
            int_stat=self.int_stat,
            fai=self.fai,
            arc=self.arc,
        )

    @property
    def class_base_stats(self) -> dict[str, int]:
        return CLASS_BASE_STATS[self.class_name]

    def floor_mins(self, state: CombatState | None = None) -> tuple[int, int, int, int, int]:
        current = self.current_combat_state if state is None else state
        return (
            max(current.str_stat, self.min_str),
            max(current.dex, self.min_dex),
            max(current.int_stat, self.min_int),
            max(current.fai, self.min_fai),
            max(current.arc, self.min_arc),
        )

    def budget_snapshot(self) -> dict[str, int]:
        base_level, base_total = CLASS_BASE_LEVEL_TOTAL[self.class_name]
        base_stats = CLASS_BASE_STATS[self.class_name]
        level = self.derived_level
        total = base_total + (level - base_level)
        floor_sum = (
            self.vig
            + self.mnd
            + self.end
            + max(int(base_stats["str"]), self.min_str)
            + max(int(base_stats["dex"]), self.min_dex)
            + max(int(base_stats["int"]), self.min_int)
            + max(int(base_stats["fai"]), self.min_fai)
            + max(int(base_stats["arc"]), self.min_arc)
        )
        return {
            "level": level,
            "total": total,
            "redistributable": total - floor_sum,
        }


@dataclass(frozen=True)
class SearchScope:
    weapon_type_key: str | None
    weapon_name: str | None
    affinity: str | None
    aow_name: str | None
    somber_filter: str
    max_upgrade: int
    exact_upgrade: bool
    top_k: int

    @property
    def fixed_upgrade(self) -> int | None:
        return self.max_upgrade if self.exact_upgrade else None


@dataclass(frozen=True)
class AnalysisState:
    selected_fingerprint: tuple[Any, ...] | None
    compare_weapon_type_key: str | None
    compare_weapon_name: str | None
    compare_affinity: str | None
    compare_aow_name: str | None
    compare_match_selected_aow: bool
    levels_ahead: int
    active_workspace: WorkspaceTab = "rankings"


@dataclass(frozen=True)
class GlobalSession:
    build: BuildSession
    scope: SearchScope
    objective_id: SearchObjectiveId
    locked_combat_stats: LockedCombatStats | None
    use_locked_stats: bool
    analysis: AnalysisState


@dataclass(frozen=True)
class RequestOverrides:
    character_level: Any = UNSET
    weapon_type_key: Any = UNSET
    weapon_name: Any = UNSET
    affinity: Any = UNSET
    aow_name: Any = UNSET
    somber_filter: Any = UNSET
    max_upgrade: Any = UNSET
    fixed_upgrade: Any = UNSET
    top_k: Any = UNSET
    objective_id: Any = UNSET
    min_str: Any = UNSET
    min_dex: Any = UNSET
    min_int: Any = UNSET
    min_fai: Any = UNSET
    min_arc: Any = UNSET
    locked_stats: Any = UNSET
    progress_every: Any = UNSET


@dataclass(frozen=True)
class SolvedBuild:
    weapon_id: int
    weapon_name: str
    affinity: str
    aow_name: str | None
    upgrade: int
    str_stat: int
    dex: int
    int_stat: int
    fai: int
    arc: int
    ar_total: float
    score: float
    bleed_buildup: float
    bleed_buildup_add: float
    frost_buildup: float
    poison_buildup: float
    scarlet_rot_buildup: float
    aow_first_hit_damage: float
    aow_full_sequence_damage: float

    @property
    def fingerprint(self) -> tuple[Any, ...]:
        return (
            self.weapon_id,
            self.weapon_name.casefold(),
            self.affinity.casefold(),
            (self.aow_name or "").casefold(),
            self.upgrade,
            self.str_stat,
            self.dex,
            self.int_stat,
            self.fai,
            self.arc,
        )

    @property
    def combat_state(self) -> CombatState:
        return CombatState(
            str_stat=self.str_stat,
            dex=self.dex,
            int_stat=self.int_stat,
            fai=self.fai,
            arc=self.arc,
        )

    @property
    def locked_stats(self) -> LockedCombatStats:
        return LockedCombatStats(
            str_stat=self.str_stat,
            dex=self.dex,
            int_stat=self.int_stat,
            fai=self.fai,
            arc=self.arc,
        )

    @property
    def best_upgrade(self) -> int:
        return self.upgrade

    @property
    def best_ar_total(self) -> float:
        return self.ar_total

    def metric_for_objective(self, objective_id: SearchObjectiveId) -> float:
        if objective_id == "aow_first_hit":
            return self.aow_first_hit_damage
        if objective_id == "aow_full_sequence":
            return self.aow_full_sequence_damage
        return self.ar_total

    def to_mapping(self) -> dict[str, Any]:
        return {
            "weapon_id": self.weapon_id,
            "weapon_name": self.weapon_name,
            "affinity": self.affinity,
            "aow_name": self.aow_name,
            "str_stat": self.str_stat,
            "dex": self.dex,
            "int_stat": self.int_stat,
            "fai": self.fai,
            "arc": self.arc,
            "best_upgrade": self.upgrade,
            "best_ar_total": self.ar_total,
            "score": self.score,
            "bleed_buildup": self.bleed_buildup,
            "bleed_buildup_add": self.bleed_buildup_add,
            "frost_buildup": self.frost_buildup,
            "poison_buildup": self.poison_buildup,
            "scarlet_rot_buildup": self.scarlet_rot_buildup,
            "aow_first_hit_damage": self.aow_first_hit_damage,
            "aow_full_sequence_damage": self.aow_full_sequence_damage,
        }

    def __getitem__(self, key: str) -> Any:
        return self.to_mapping()[key]


@dataclass(frozen=True)
class PathWeaponConfig:
    title: str
    solved: SolvedBuild
    start_state: CombatState

    @property
    def weapon_name(self) -> str:
        return self.solved.weapon_name

    @property
    def affinity(self) -> str:
        return self.solved.affinity

    @property
    def aow_name(self) -> str | None:
        return self.solved.aow_name

    @property
    def upgrade(self) -> int:
        return self.solved.upgrade


@dataclass(frozen=True)
class PathStep:
    level: int
    stats: CombatState
    metric: float | None
    score: float | None
    added_stat: str | None
    requirement_gap: int

    @property
    def ar(self) -> float | None:
        return self.metric


@dataclass(frozen=True)
class PathPreview:
    config: PathWeaponConfig
    steps: tuple[PathStep, ...]


@dataclass(frozen=True)
class AffinityWatchPoint:
    level: int
    metric: float | None
    solved: SolvedBuild | None


@dataclass(frozen=True)
class AffinityWatchLine:
    affinity: str
    points: tuple[AffinityWatchPoint, ...]
    start_metric: float | None
    end_metric: float | None
    final_build: SolvedBuild | None


@dataclass(frozen=True)
class AffinityBreakpoint:
    level: int
    outgoing_affinity: str
    incoming_affinity: str
    outgoing_metric: float | None
    incoming_metric: float | None


@dataclass(frozen=True)
class AffinityWatchPayload:
    lines: tuple[AffinityWatchLine, ...]
    breakpoints: tuple[AffinityBreakpoint, ...]


def combat_state_attr(stat_key: str) -> str:
    return {
        "str": "str_stat",
        "dex": "dex",
        "int": "int_stat",
        "fai": "fai",
        "arc": "arc",
    }[stat_key]


def result_rank_key(solved: SolvedBuild) -> tuple[float, float, float, float, float, int, int]:
    return (
        float(solved.score),
        float(solved.ar_total),
        float(solved.aow_full_sequence_damage),
        float(solved.aow_first_hit_damage),
        float(solved.bleed_buildup),
        -int(solved.weapon_id),
        int(solved.upgrade),
    )
