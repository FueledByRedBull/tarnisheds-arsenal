from __future__ import annotations

import sys
from pathlib import Path
from typing import Any, Callable

THIS_DIR = Path(__file__).resolve().parent
if str(THIS_DIR) not in sys.path:
    sys.path.insert(0, str(THIS_DIR))

try:
    import er_optimizer_core as core
except ImportError as exc:  # pragma: no cover
    raise SystemExit(
        "Failed to import er_optimizer_core. Build/install the extension first."
    ) from exc

from models import (
    CLASS_BASE_STATS,
    AffinityBreakpoint,
    AffinityWatchLine,
    AffinityWatchPayload,
    AffinityWatchPoint,
    BuildSession,
    CombatState,
    GlobalSession,
    LockedCombatStats,
    PathPreview,
    PathStep,
    PathWeaponConfig,
    RequestOverrides,
    SearchObjectiveId,
    SolvedBuild,
    UNSET,
    combat_state_attr,
    result_rank_key,
)


class DesktopOptimizerService:
    def __init__(self, data: Any) -> None:
        self.data = data
        self.best_build_cache: dict[tuple[Any, ...], SolvedBuild | None] = {}
        self.upgrade_series_cache: dict[tuple[Any, ...], dict[int, float]] = {}
        self.path_step_cache: dict[tuple[Any, ...], PathStep] = {}
        self.path_target_cache: dict[tuple[Any, ...], SolvedBuild | None] = {}
        self.affinity_watch_cache: dict[tuple[Any, ...], SolvedBuild | None] = {}

    def clear_caches(self) -> None:
        self.best_build_cache.clear()
        self.upgrade_series_cache.clear()
        self.path_step_cache.clear()
        self.path_target_cache.clear()
        self.affinity_watch_cache.clear()

    def build_optimize_request(
        self,
        session: GlobalSession,
        include_progress: bool = False,
        overrides: RequestOverrides | None = None,
    ) -> dict[str, Any]:
        overrides = RequestOverrides() if overrides is None else overrides
        build = session.build
        scope = session.scope
        locked_stats = self._resolve_locked_stats(session, overrides)
        class_base = build.class_base_stats

        def pick(value: Any, fallback: Any) -> Any:
            return fallback if value is UNSET else value

        request = {
            "class_name": build.class_name,
            "character_level": int(pick(overrides.character_level, build.derived_level)),
            "vig": build.vig,
            "mnd": build.mnd,
            "end": build.end,
            "str_stat": int(class_base["str"]),
            "dex": int(class_base["dex"]),
            "int_stat": int(class_base["int"]),
            "fai": int(class_base["fai"]),
            "arc": int(class_base["arc"]),
            "max_upgrade": int(pick(overrides.max_upgrade, scope.max_upgrade)),
            "fixed_upgrade": pick(overrides.fixed_upgrade, scope.fixed_upgrade),
            "two_handing": build.two_handing,
            "weapon_name": pick(overrides.weapon_name, scope.weapon_name),
            "affinity": pick(overrides.affinity, scope.affinity),
            "aow_name": pick(overrides.aow_name, scope.aow_name),
            "objective": pick(overrides.objective_id, session.objective_id),
            "top_k": int(pick(overrides.top_k, scope.top_k)),
            "weapon_type_key": pick(overrides.weapon_type_key, scope.weapon_type_key),
            "somber_filter": pick(overrides.somber_filter, scope.somber_filter),
            "min_str": int(pick(overrides.min_str, build.min_str)),
            "min_dex": int(pick(overrides.min_dex, build.min_dex)),
            "min_int": int(pick(overrides.min_int, build.min_int)),
            "min_fai": int(pick(overrides.min_fai, build.min_fai)),
            "min_arc": int(pick(overrides.min_arc, build.min_arc)),
            "lock_str": None if locked_stats is None else locked_stats.str_stat,
            "lock_dex": None if locked_stats is None else locked_stats.dex,
            "lock_int": None if locked_stats is None else locked_stats.int_stat,
            "lock_fai": None if locked_stats is None else locked_stats.fai,
            "lock_arc": None if locked_stats is None else locked_stats.arc,
        }
        progress_every = pick(overrides.progress_every, 5000 if include_progress else UNSET)
        if progress_every is not UNSET:
            request["progress_every"] = int(progress_every)
        return request

    def estimate_search_space(self, session: GlobalSession) -> Any:
        request = self.build_optimize_request(session)
        request.pop("top_k", None)
        request.pop("progress_every", None)
        return core.estimate_search_space(data=self.data, **request)

    def normalize_result(self, result: Any) -> SolvedBuild:
        return SolvedBuild(
            weapon_id=int(result.weapon_id),
            weapon_name=str(result.weapon_name),
            affinity=str(result.affinity),
            aow_name=result.aow_name,
            upgrade=int(result.upgrade),
            str_stat=int(result.str_stat),
            dex=int(result.dex),
            int_stat=int(result.int_stat),
            fai=int(result.fai),
            arc=int(result.arc),
            ar_total=float(result.ar_total),
            score=float(result.score),
            bleed_buildup=float(result.bleed_buildup),
            bleed_buildup_add=float(result.bleed_buildup_add),
            frost_buildup=float(result.frost_buildup),
            poison_buildup=float(result.poison_buildup),
            aow_first_hit_damage=float(result.aow_first_hit_damage),
            aow_full_sequence_damage=float(result.aow_full_sequence_damage),
        )

    def run_search(
        self,
        session: GlobalSession,
        progress_cb: Callable[..., None] | None = None,
    ) -> list[SolvedBuild]:
        request = self.build_optimize_request(session, include_progress=progress_cb is not None)
        rows = core.optimize_builds(data=self.data, progress_cb=progress_cb, **request)
        return [self.normalize_result(row) for row in rows]

    def search_request_signature(self, session: GlobalSession) -> tuple[Any, ...]:
        request = self.build_optimize_request(session)
        return tuple(sorted(request.items(), key=lambda item: item[0]))

    def optimizer_context_key(self, session: GlobalSession) -> tuple[Any, ...]:
        build = session.build
        return (
            build,
            session.objective_id,
            session.scope.max_upgrade,
            session.scope.exact_upgrade,
        )

    def solve_build(
        self,
        session: GlobalSession,
        weapon_name: str,
        affinity: str,
        aow_name: str | None,
    ) -> SolvedBuild | None:
        cache_key = (
            self.optimizer_context_key(session),
            weapon_name.casefold(),
            affinity.casefold(),
            (aow_name or "").casefold(),
        )
        if cache_key in self.best_build_cache:
            return self.best_build_cache[cache_key]
        request = self.build_optimize_request(
            session,
            overrides=RequestOverrides(
                weapon_name=weapon_name,
                affinity=affinity,
                aow_name=aow_name,
                top_k=1,
                weapon_type_key=None,
                somber_filter="all",
                locked_stats=None,
            ),
        )
        try:
            rows = core.optimize_builds(data=self.data, **request)
        except Exception:
            rows = []
        solved = self.normalize_result(rows[0]) if rows else None
        self.best_build_cache[cache_key] = solved
        return solved

    def build_upgrade_series(
        self,
        session: GlobalSession,
        solved: SolvedBuild,
        max_upgrade: int,
    ) -> dict[int, float]:
        cache_key = (
            self.optimizer_context_key(session),
            solved.fingerprint,
            int(max_upgrade),
        )
        cached = self.upgrade_series_cache.get(cache_key)
        if cached is not None:
            return cached
        request = self.build_optimize_request(
            session,
            overrides=RequestOverrides(
                weapon_name=solved.weapon_name,
                affinity=solved.affinity,
                aow_name=solved.aow_name,
                objective_id=session.objective_id,
                top_k=max_upgrade + 1,
                fixed_upgrade=None,
                max_upgrade=max_upgrade,
                somber_filter="all",
                weapon_type_key=None,
                min_str=0,
                min_dex=0,
                min_int=0,
                min_fai=0,
                min_arc=0,
                locked_stats=solved.locked_stats,
            ),
        )
        try:
            rows = core.optimize_builds(data=self.data, **request)
        except Exception:
            rows = []
        series = {int(row.upgrade): self.normalize_result(row).metric_for_objective(session.objective_id) for row in rows}
        self.upgrade_series_cache[cache_key] = series
        return series

    def build_path_preview(
        self,
        session: GlobalSession,
        solved: SolvedBuild,
        levels_ahead: int,
        title: str,
    ) -> PathPreview:
        config = PathWeaponConfig(
            title=title,
            solved=solved,
            start_state=solved.combat_state,
        )
        steps: list[PathStep] = []
        current_state = config.start_state

        start_step = self._evaluate_path_step(session, config, session.build.derived_level, current_state, None)
        steps.append(start_step)
        target_build = self._path_target_build(session, config, levels_ahead)
        if target_build is None:
            return PathPreview(config=config, steps=tuple(steps))
        target_state = target_build.combat_state

        for delta in range(1, levels_ahead + 1):
            next_step = self._choose_next_path_step(
                session,
                config,
                session.build.derived_level + delta,
                current_state,
                target_state,
            )
            if next_step is None:
                break
            steps.append(next_step)
            current_state = next_step.stats
        return PathPreview(config=config, steps=tuple(steps))

    def build_affinity_watch(
        self,
        session: GlobalSession,
        solved: SolvedBuild,
        levels_ahead: int,
        progress_cb: Callable[[int, int, str, int], None] | None = None,
    ) -> AffinityWatchPayload:
        affinities = self.affinity_watch_affinities(solved)
        levels = [session.build.derived_level + offset for offset in range(0, levels_ahead + 1)]
        lines: list[AffinityWatchLine] = []
        total = len(affinities) * len(levels)
        processed = 0
        for affinity in affinities:
            points: list[AffinityWatchPoint] = []
            for level in levels:
                cache_key = (
                    session.build,
                    session.objective_id,
                    solved.weapon_name.casefold(),
                    affinity.casefold(),
                    (solved.aow_name or "").casefold(),
                    solved.upgrade,
                    int(level),
                )
                build = self.affinity_watch_cache.get(cache_key)
                if cache_key not in self.affinity_watch_cache:
                    request = self.build_optimize_request(
                        session,
                        overrides=RequestOverrides(
                            character_level=level,
                            weapon_name=solved.weapon_name,
                            affinity=affinity,
                            aow_name=solved.aow_name,
                            max_upgrade=solved.upgrade,
                            fixed_upgrade=solved.upgrade,
                            top_k=1,
                            weapon_type_key=None,
                            somber_filter="all",
                        ),
                    )
                    try:
                        rows = core.optimize_builds(data=self.data, **request)
                    except Exception:
                        rows = []
                    build = self.normalize_result(rows[0]) if rows else None
                    self.affinity_watch_cache[cache_key] = build
                metric = build.metric_for_objective(session.objective_id) if build is not None else None
                points.append(AffinityWatchPoint(level=int(level), metric=metric, solved=build))
                processed += 1
                if progress_cb is not None:
                    progress_cb(processed, total, affinity, int(level))
            valid_points = [point for point in points if point.solved is not None and point.metric is not None]
            if not valid_points:
                continue
            lines.append(
                AffinityWatchLine(
                    affinity=affinity,
                    points=tuple(points),
                    start_metric=valid_points[0].metric,
                    end_metric=valid_points[-1].metric,
                    final_build=valid_points[-1].solved,
                )
            )
        lines.sort(
            key=lambda line: (
                float(line.end_metric if line.end_metric is not None else float("-inf")),
                result_rank_key(line.final_build) if line.final_build is not None else tuple(),
            ),
            reverse=True,
        )
        return AffinityWatchPayload(
            lines=tuple(lines),
            breakpoints=tuple(self.detect_affinity_breakpoints(list(lines), levels)),
        )

    def affinity_watch_affinities(self, solved: SolvedBuild) -> list[str]:
        affinities = list(self.data.affinities_for_weapon(solved.weapon_name))
        if solved.aow_name is not None:
            affinities = [
                affinity
                for affinity in affinities
                if solved.aow_name in self.compatible_aow_names(solved.weapon_name, affinity)
            ]
        if solved.affinity not in affinities:
            affinities.append(solved.affinity)
        affinities.sort(key=lambda affinity: (affinity != solved.affinity, affinity.casefold()))
        return affinities

    def compatible_aow_names(self, weapon_name: str | None, affinity: str | None) -> list[str]:
        if weapon_name is None:
            if affinity is None:
                return list(self.data.aow_names())
            return list(self.data.compatible_aow_names_for_affinity(affinity))
        return list(self.data.compatible_aow_names(weapon_name, affinity))

    def detect_affinity_breakpoints(
        self,
        lines: list[AffinityWatchLine],
        levels: list[int],
    ) -> list[AffinityBreakpoint]:
        line_maps = {line.affinity: {point.level: point for point in line.points} for line in lines}
        breakpoints: list[AffinityBreakpoint] = []
        leader_affinity: str | None = None
        for level in levels:
            contenders = [
                point.solved
                for line in lines
                if (point := line_maps[line.affinity].get(level)) is not None and point.solved is not None
            ]
            if not contenders:
                continue
            leader = max(contenders, key=result_rank_key)
            current_affinity = leader.affinity
            if leader_affinity is not None and current_affinity != leader_affinity:
                outgoing = line_maps.get(leader_affinity, {}).get(level)
                incoming = line_maps.get(current_affinity, {}).get(level)
                breakpoints.append(
                    AffinityBreakpoint(
                        level=int(level),
                        outgoing_affinity=leader_affinity,
                        incoming_affinity=current_affinity,
                        outgoing_metric=None if outgoing is None else outgoing.metric,
                        incoming_metric=None if incoming is None else incoming.metric,
                    )
                )
            leader_affinity = current_affinity
        return breakpoints

    def _resolve_locked_stats(
        self,
        session: GlobalSession,
        overrides: RequestOverrides,
    ) -> LockedCombatStats | None:
        if overrides.locked_stats is UNSET:
            if not session.use_locked_stats:
                return None
            return session.locked_combat_stats
        return overrides.locked_stats

    def _path_target_build(
        self,
        session: GlobalSession,
        config: PathWeaponConfig,
        levels_ahead: int,
    ) -> SolvedBuild | None:
        floor_mins = session.build.floor_mins(config.start_state)
        target_level = session.build.derived_level + levels_ahead
        cache_key = (
            session.build,
            session.objective_id,
            config.solved.fingerprint,
            int(target_level),
            floor_mins,
        )
        if cache_key in self.path_target_cache:
            return self.path_target_cache[cache_key]
        request = self.build_optimize_request(
            session,
            overrides=RequestOverrides(
                character_level=target_level,
                weapon_name=config.weapon_name,
                affinity=config.affinity,
                aow_name=config.aow_name,
                max_upgrade=config.upgrade,
                fixed_upgrade=config.upgrade,
                top_k=1,
                weapon_type_key=None,
                somber_filter="all",
                min_str=floor_mins[0],
                min_dex=floor_mins[1],
                min_int=floor_mins[2],
                min_fai=floor_mins[3],
                min_arc=floor_mins[4],
                locked_stats=None,
            ),
        )
        try:
            rows = core.optimize_builds(data=self.data, **request)
        except Exception:
            rows = []
        solved = self.normalize_result(rows[0]) if rows else None
        self.path_target_cache[cache_key] = solved
        return solved

    def _choose_next_path_step(
        self,
        session: GlobalSession,
        config: PathWeaponConfig,
        target_level: int,
        current_state: CombatState,
        target_state: CombatState,
    ) -> PathStep | None:
        candidates: list[PathStep] = []
        for stat_key in ("str", "dex", "int", "fai", "arc"):
            if getattr(current_state, combat_state_attr(stat_key)) >= getattr(target_state, combat_state_attr(stat_key)):
                continue
            next_state = current_state.add_point(stat_key)
            if next_state is None:
                continue
            candidates.append(
                self._evaluate_path_step(session, config, target_level, next_state, stat_key)
            )
        if not candidates:
            return None
        return max(candidates, key=self._path_step_sort_key)

    def _evaluate_path_step(
        self,
        session: GlobalSession,
        config: PathWeaponConfig,
        level: int,
        state: CombatState,
        added_stat: str | None,
    ) -> PathStep:
        cache_key = (
            session.build,
            session.objective_id,
            config.solved.fingerprint,
            int(level),
            state,
        )
        cached = self.path_step_cache.get(cache_key)
        if cached is not None:
            return PathStep(
                level=cached.level,
                stats=cached.stats,
                metric=cached.metric,
                score=cached.score,
                added_stat=added_stat,
                requirement_gap=cached.requirement_gap,
            )
        request = self.build_optimize_request(
            session,
            overrides=RequestOverrides(
                character_level=level,
                weapon_name=config.weapon_name,
                affinity=config.affinity,
                aow_name=config.aow_name,
                max_upgrade=config.upgrade,
                fixed_upgrade=config.upgrade,
                top_k=1,
                weapon_type_key=None,
                somber_filter="all",
                min_str=0,
                min_dex=0,
                min_int=0,
                min_fai=0,
                min_arc=0,
                locked_stats=LockedCombatStats(
                    str_stat=state.str_stat,
                    dex=state.dex,
                    int_stat=state.int_stat,
                    fai=state.fai,
                    arc=state.arc,
                ),
            ),
        )
        try:
            rows = core.optimize_builds(data=self.data, **request)
        except Exception:
            rows = []
        solved = self.normalize_result(rows[0]) if rows else None
        step = PathStep(
            level=level,
            stats=state,
            metric=None if solved is None else solved.metric_for_objective(session.objective_id),
            score=None if solved is None else float(solved.score),
            added_stat=None,
            requirement_gap=self._requirement_gap(session.build, config, state) if solved is None else 0,
        )
        self.path_step_cache[cache_key] = step
        return PathStep(
            level=step.level,
            stats=step.stats,
            metric=step.metric,
            score=step.score,
            added_stat=added_stat,
            requirement_gap=step.requirement_gap,
        )

    def _requirement_gap(
        self,
        build: BuildSession,
        config: PathWeaponConfig,
        state: CombatState,
    ) -> int:
        try:
            req_str, req_dex, req_int, req_fai, req_arc = self.data.weapon_requirements(
                config.weapon_name,
                config.affinity,
            )
        except Exception:
            return 999
        effective_str = state.str_stat
        if build.two_handing:
            effective_str = min(99, int(state.str_stat * 1.5))
        return (
            max(req_str - effective_str, 0)
            + max(req_dex - state.dex, 0)
            + max(req_int - state.int_stat, 0)
            + max(req_fai - state.fai, 0)
            + max(req_arc - state.arc, 0)
        )

    @staticmethod
    def _path_step_sort_key(step: PathStep) -> tuple[int, float, float, int, int]:
        return (
            1 if step.metric is not None and step.score is not None else 0,
            float(step.score or 0.0),
            float(step.metric or 0.0),
            -int(step.requirement_gap),
            -DesktopOptimizerService._stat_priority(step.added_stat),
        )

    @staticmethod
    def _stat_priority(stat_key: str | None) -> int:
        order = ("str", "dex", "int", "fai", "arc", None)
        return order.index(stat_key)
