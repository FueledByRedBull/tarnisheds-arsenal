from __future__ import annotations

import importlib.util
import os
import sys
import time
from pathlib import Path

from PyQt6 import QtCore, QtWidgets


def load_app_module(project_root: Path):
    module_path = project_root / "ui" / "desktop" / "app.py"
    spec = importlib.util.spec_from_file_location("er_optimizer_ui", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load app module spec from {module_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def wait_until(predicate, timeout_seconds: float = 30.0) -> None:
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        QtWidgets.QApplication.processEvents()
        if predicate():
            return
        time.sleep(0.01)
    raise TimeoutError("timed out waiting for UI condition")


def main() -> int:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    project_root = Path(__file__).resolve().parents[2]
    app_module = load_app_module(project_root)

    app = QtWidgets.QApplication([])
    app_module.apply_dark_theme(app)
    window = app_module.MainWindow()

    if window.hero_panel is None:
        raise AssertionError("expected hero panel")
    if window.result_cards_container is None or len(window.result_cards) != 3:
        raise AssertionError("expected three result cards")
    if window.compare_summary_container is None:
        raise AssertionError("expected comparison summary container")
    if window.main_tabs.count() != 4:
        raise AssertionError("expected rankings, compare, paths, and affinity watch tabs")
    if window.main_tabs.tabText(2) != "PATHS":
        raise AssertionError("expected dedicated paths tab")
    if window.main_tabs.tabText(3) != "AFFINITY WATCH":
        raise AssertionError("expected dedicated affinity watch tab")

    # Requirement highlighting check
    window._set_combo_by_data(window.class_combo, "Wretch")
    window._on_class_changed()
    window._set_combo_by_data(window.weapon_combo, "Uchigatana")
    window._refresh_affinity_options()
    window._set_combo_by_data(window.affinity_combo, "Keen")
    window.str_spin.setValue(10)
    window.dex_spin.setValue(10)
    window._refresh_estimate()
    if not bool(window.str_spin.property("reqFail")):
        raise AssertionError("expected STR requirement highlight")
    if not bool(window.dex_spin.property("reqFail")):
        raise AssertionError("expected DEX requirement highlight")

    # Reset to valid stats and run search
    window.str_spin.setValue(12)
    window.dex_spin.setValue(15)
    window.max_upgrade_spin.setValue(1)
    window.top_k_spin.setValue(5)
    window._start_search()
    wait_until(lambda: window.active_run_id is None)
    if len(window.current_results) == 0:
        raise AssertionError("expected non-empty search results")
    if "SEARCH" not in window.hero_objective_label.text():
        raise AssertionError("expected hero objective text")
    if window.result_cards[0]["title"].text() == "No result yet":
        raise AssertionError("expected populated lead result card")
    original_row_count = window.results_table.rowCount()
    window.max_upgrade_spin.setValue(2)
    QtWidgets.QApplication.processEvents()
    if window.current_results:
        raise AssertionError("expected stale results to be cleared after input change")
    if window.results_table.rowCount() != 0:
        raise AssertionError("expected results table to clear after input change")
    if "Ready" in window.hero_search_chip.text():
        raise AssertionError("expected hero state to stop advertising stale results")
    window.max_upgrade_spin.setValue(1)
    window._start_search()
    wait_until(lambda: window.active_run_id is None)
    if window.results_table.rowCount() == 0 or window.results_table.rowCount() == original_row_count == 0:
        raise AssertionError("expected results to repopulate after rerun")

    # Lock from first row (triggers rerun)
    window._lock_from_result(0)
    wait_until(lambda: window.active_run_id is None)
    if len(window.current_results) == 0:
        raise AssertionError("expected results after lock rerun")

    # Explicit side-by-side compare row
    window._set_combo_by_data(window.compare_weapon_combo, "Uchigatana")
    window._refresh_compare_affinity_options()
    if window.compare_affinity_combo.count() <= 1:
        raise AssertionError("compare affinity options were not populated")
    window._set_combo_by_data(window.compare_affinity_combo, "Occult")
    window._rebuild_upgrade_table()
    if window.upgrade_table.rowCount() < 2:
        raise AssertionError("expected selected + compare rows in upgrade table")
    if "Waiting on" in window.selected_compare_panel["title"].text():
        raise AssertionError("expected selected comparison summary to populate")
    if window.compare_compare_panel["title"].text() == "Waiting on selection":
        raise AssertionError("expected comparison target summary to populate")
    if not window.level_path_button.isEnabled():
        raise AssertionError("expected path graph button to enable for a valid comparison")
    if not window.affinity_watch_button.isEnabled():
        raise AssertionError("expected affinity watcher button to enable for a selected result")
    if not window.path_tab_open_button.isEnabled():
        raise AssertionError("expected paths tab action to enable for a valid comparison")
    if not window.affinity_tab_open_button.isEnabled():
        raise AssertionError("expected affinity watch tab action to enable for a selected result")
    if "Selected:" not in window.path_workspace_summary.text():
        raise AssertionError("expected paths tab summary to reflect the selected lane")
    if "legal affinities" not in window.affinity_workspace_detail.text():
        raise AssertionError("expected affinity watch tab detail to reflect derived affinity state")
    previews = window._build_level_path_previews(3)
    if previews is None or len(previews) != 2:
        raise AssertionError("expected two level-path previews")
    if any(len(preview.steps) < 2 for preview in previews):
        raise AssertionError("expected each level-path preview to include forward steps")
    if any(preview.steps[1].added_stat is None for preview in previews):
        raise AssertionError("expected path preview to record the first added stat")
    for preview in previews:
        target_row = window._level_path_target_row(preview.config, 3)
        if target_row is None:
            raise AssertionError("expected a target row for the path preview")
        final_state = preview.steps[-1].stats
        if (
            final_state.str_stat != int(target_row["str_stat"])
            or final_state.dex != int(target_row["dex"])
            or final_state.int_stat != int(target_row["int_stat"])
            or final_state.fai != int(target_row["fai"])
            or final_state.arc != int(target_row["arc"])
        ):
            raise AssertionError("expected path preview to land on the exact Current+N target state")
    dialog = app_module.LevelPathDialog(window, previews, window._derived_level(), 3)
    dialog.show()
    QtWidgets.QApplication.processEvents()
    dialog.close()

    watcher_row = {
        "weapon_name": "Claymore",
        "affinity": "Fire",
        "aow_name": "Lion's Claw",
        "best_upgrade": 25,
        "str_stat": 20,
        "dex": 20,
        "int_stat": 9,
        "fai": 8,
        "arc": 8,
        "best_ar_total": 0.0,
        "score": 0.0,
        "bleed_buildup": 0.0,
        "bleed_buildup_add": 0.0,
        "frost_buildup": 0.0,
        "poison_buildup": 0.0,
        "aow_first_hit_damage": 0.0,
        "aow_full_sequence_damage": 0.0,
    }
    window.str_spin.setValue(20)
    window.dex_spin.setValue(20)
    window.int_spin.setValue(9)
    window.fai_spin.setValue(8)
    window.arc_spin.setValue(8)
    window.objective_combo.setCurrentIndex(window.objective_combo.findData("max_ar"))
    window._refresh_estimate()
    watcher_lines, watcher_breaks = window._build_affinity_watch_data(watcher_row, 3)
    if len(watcher_lines) < 2:
        raise AssertionError("expected multiple affinity watcher lines")
    if not any(point.metric is not None for line in watcher_lines for point in line.points):
        raise AssertionError("expected populated affinity watcher metrics")
    watcher_dialog = app_module.AffinityWatchDialog(
        window,
        watcher_row["weapon_name"],
        watcher_row["aow_name"],
        watcher_row["best_upgrade"],
        window._derived_level(),
        3,
        window._objective_metric_label(),
        watcher_lines,
        watcher_breaks,
    )
    watcher_dialog.show()
    QtWidgets.QApplication.processEvents()
    watcher_dialog.close()
    window.objective_combo.setCurrentIndex(window.objective_combo.findData("aow_full_sequence"))
    window._refresh_estimate()
    watcher_lines_aow, _ = window._build_affinity_watch_data(watcher_row, 3)
    if len(watcher_lines_aow) < 2:
        raise AssertionError("expected affinity watcher AoW lines")
    shared_affinities = sorted({line.affinity for line in watcher_lines}.intersection(line.affinity for line in watcher_lines_aow))
    if not shared_affinities:
        raise AssertionError("expected shared affinities across watcher objectives")
    if all(
        abs(
            next(line.end_metric for line in watcher_lines if line.affinity == affinity)
            - next(line.end_metric for line in watcher_lines_aow if line.affinity == affinity)
        ) < 1e-9
        for affinity in shared_affinities
    ):
        raise AssertionError("expected objective change to alter affinity watcher metrics")

    # AoW damage objective smoke
    window._set_combo_by_data(window.weapon_combo, "Sword Lance")
    window._refresh_affinity_options()
    window._set_combo_by_data(window.affinity_combo, "Magic")
    window._set_combo_by_data(window.aow_combo, "Glintstone Pebble")
    window.objective_combo.setCurrentIndex(window.objective_combo.findData("aow_first_hit"))
    window.max_upgrade_spin.setValue(25)
    window._start_search()
    wait_until(lambda: window.active_run_id is None)
    if not window.current_results:
        raise AssertionError("expected AoW objective to return results")
    lead = window.current_results[0]
    if float(lead.aow_first_hit_damage) <= 0.0:
        raise AssertionError("expected positive AoW first-hit damage")
    if float(lead.aow_full_sequence_damage) < float(lead.aow_first_hit_damage):
        raise AssertionError("expected AoW full-sequence damage to stay above first-hit damage")

    # One final event pump for queued signals
    QtCore.QTimer.singleShot(1, app.quit)
    app.exec()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
