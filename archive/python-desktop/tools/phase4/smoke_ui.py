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

    if window.findChild(QtWidgets.QWidget, "CommandRail") is None:
        raise AssertionError("expected workflow command rail")
    if window.findChild(QtWidgets.QWidget, "WorkspacePanel") is None:
        raise AssertionError("expected center workspace panel")
    if window.findChild(QtWidgets.QWidget, "InspectorPanel") is None:
        raise AssertionError("expected right inspector panel")
    if window.findChild(QtWidgets.QWidget, "AdvancedDrawer") is None:
        raise AssertionError("expected advanced drawer")
    if window.main_tabs.tabBar().isVisible():
        raise AssertionError("expected workspace tabs to be driven by workflow navigation")
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
    if window.inspector_title.text() == "No result selected":
        raise AssertionError("expected selected result inspector to update after rankings populate")
    if not window.inspector_lock_button.isEnabled():
        raise AssertionError("expected inspector lock action to enable for selected result")

    advanced_before = (
        window.weapon_combo.currentText(),
        window.affinity_combo.currentText(),
        window.aow_combo.currentText(),
        window.objective_combo.currentData(),
        window.max_upgrade_spin.value(),
        window.top_k_spin.value(),
    )
    window.advanced_drawer.set_open(True)
    QtWidgets.QApplication.processEvents()
    if not window.advanced_drawer.is_open():
        raise AssertionError("expected advanced drawer to open")
    window.advanced_drawer.set_open(False)
    QtWidgets.QApplication.processEvents()
    advanced_after = (
        window.weapon_combo.currentText(),
        window.affinity_combo.currentText(),
        window.aow_combo.currentText(),
        window.objective_combo.currentData(),
        window.max_upgrade_spin.value(),
        window.top_k_spin.value(),
    )
    if advanced_before != advanced_after:
        raise AssertionError("advanced drawer toggled changed session controls")

    # Lock from first row (triggers rerun)
    window._lock_from_result(0)
    wait_until(lambda: window.active_run_id is None)
    if len(window.current_results) == 0:
        raise AssertionError("expected results after lock rerun")
    if not window.lock_upgrade_exact.isChecked():
        raise AssertionError("expected Use As Locks to enable exact upgrade locking")
    if not window.lock_stats_checkbox.isChecked():
        raise AssertionError("expected Use As Locks to enable combat stat locking")
    if window.locked_result_stats is None:
        raise AssertionError("expected Use As Locks to store combat stats")
    locked_stats = window.locked_result_stats
    locked_upgrade = window.max_upgrade_spin.value()
    for row in window.current_results:
        if int(row.upgrade) != locked_upgrade:
            raise AssertionError("expected lock rerun to preserve exact upgrade")
        if (
            int(row.str_stat) != locked_stats.str_stat
            or int(row.dex) != locked_stats.dex
            or int(row.int_stat) != locked_stats.int_stat
            or int(row.fai) != locked_stats.fai
            or int(row.arc) != locked_stats.arc
        ):
            raise AssertionError("expected lock rerun to preserve exact combat stats")

    # Explicit side-by-side compare row
    window._set_combo_by_data(window.compare_weapon_combo, "Uchigatana")
    window._refresh_compare_affinity_options()
    if window.compare_affinity_combo.count() <= 1:
        raise AssertionError("compare affinity options were not populated")
    window.compare_affinity_combo.setCurrentIndex(0)
    window._rebuild_upgrade_table()
    if window._combo_value(window.compare_affinity_combo) is not None:
        raise AssertionError("compare affinity should stay open when <Open> is selected")
    if window.upgrade_table.rowCount() < 2:
        raise AssertionError("expected compare row with open affinity")
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
    if not window.inspector_path_button.isEnabled():
        raise AssertionError("expected inspector path action to enable for a valid comparison")
    if not window.inspector_affinity_button.isEnabled():
        raise AssertionError("expected inspector affinity action to enable for a selected result")
    window.level_path_horizon_spin.setValue(3)
    if not window.path_tab_open_button.isEnabled():
        raise AssertionError("expected paths tab action to enable for a valid comparison")
    if not window.affinity_tab_open_button.isEnabled():
        raise AssertionError("expected affinity watch tab action to enable for a selected result")
    if "Selected:" not in window.path_workspace_summary.text():
        raise AssertionError("expected paths tab summary to reflect the selected lane")
    if "legal affinities" not in window.affinity_workspace_detail.text():
        raise AssertionError("expected affinity watch tab detail to reflect derived affinity state")
    session = window._current_session()
    previews = [
        window.desktop_service.build_path_preview(session, config.solved, 3, config.title)
        for config in window._path_preview_configs()
    ]
    if len(previews) != 2:
        raise AssertionError("expected two level-path previews")
    if any(len(preview.steps) < 2 for preview in previews):
        raise AssertionError("expected each level-path preview to include forward steps")
    if any(preview.steps[1].added_stat is None for preview in previews):
        raise AssertionError("expected path preview to record the first added stat")
    for preview in previews:
        target_row = window.desktop_service._path_target_build(session, preview.config, 3)
        if target_row is None:
            raise AssertionError("expected a target row for the path preview")
        final_state = preview.steps[-1].stats
        if (
            final_state.str_stat != int(target_row.str_stat)
            or final_state.dex != int(target_row.dex)
            or final_state.int_stat != int(target_row.int_stat)
            or final_state.fai != int(target_row.fai)
            or final_state.arc != int(target_row.arc)
        ):
            raise AssertionError("expected path preview to land on the exact Current+N target state")
    dialog = app_module.LevelPathDialog(window, previews, window._derived_level(), 3)
    dialog.show()
    QtWidgets.QApplication.processEvents()
    dialog.close()
    window.path_tab_open_button.click()
    wait_until(lambda: window.path_thread is None)
    if window.path_progress_label.text().startswith("Failed"):
        raise AssertionError("expected embedded paths analysis to succeed")
    if window.path_tables_splitter.count() < 2:
        raise AssertionError("expected embedded paths tab to populate lane panels")
    window.affinity_tab_open_button.click()
    wait_until(lambda: window.affinity_watch_thread is None)
    if window.affinity_progress_label.text().startswith("Failed"):
        raise AssertionError("expected embedded affinity analysis to succeed")
    if window.affinity_summary_table.rowCount() < 2:
        raise AssertionError("expected embedded affinity watch table to populate")

    watcher_row = {
        "weapon_name": "Sword Lance",
        "affinity": "Magic",
        "aow_name": "Glintstone Pebble",
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
    window.lock_stats_checkbox.setChecked(False)
    window.locked_result_stats = None
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

    window._set_combo_by_data(window.class_combo, "Samurai")
    window._on_class_changed()
    window.vig_spin.setValue(12)
    window.mnd_spin.setValue(11)
    window.end_spin.setValue(13)
    window.str_spin.setValue(12)
    window.dex_spin.setValue(15)
    window.int_spin.setValue(9)
    window.fai_spin.setValue(8)
    window.arc_spin.setValue(45)
    window._set_combo_by_data(window.weapon_combo, "Uchigatana")
    window._refresh_affinity_options()
    window._set_combo_by_data(window.affinity_combo, "Blood")
    window.max_upgrade_spin.setValue(25)
    window.lock_upgrade_exact.setChecked(True)
    window.objective_combo.setCurrentIndex(window.objective_combo.findData("max_ar"))
    window._set_combo_by_data(window.aow_combo, "Double Slash")
    window._start_search()
    wait_until(lambda: window.active_run_id is None)
    if not window.current_results:
        raise AssertionError("expected base blood uchigatana result")
    base_ar = float(window.current_results[0].ar_total)
    base_bleed = float(window.current_results[0].bleed_buildup)
    window._set_combo_by_data(window.aow_combo, "Seppuku")
    window._start_search()
    wait_until(lambda: window.active_run_id is None)
    if not window.current_results:
        raise AssertionError("expected Seppuku blood uchigatana result")
    if float(window.current_results[0].ar_total) < base_ar + 29.9:
        raise AssertionError("expected Seppuku to add flat AR to the buffed weapon row")
    if float(window.current_results[0].bleed_buildup) < base_bleed + 30.0:
        raise AssertionError("expected Seppuku to add scaling bleed buildup")

    window._set_combo_by_data(window.class_combo, "Wretch")
    window._on_class_changed()
    window.vig_spin.setValue(10)
    window.mnd_spin.setValue(10)
    window.end_spin.setValue(10)
    window.str_spin.setValue(68)
    window.dex_spin.setValue(15)
    window.int_spin.setValue(10)
    window.fai_spin.setValue(10)
    window.arc_spin.setValue(10)
    window._set_combo_by_data(window.weapon_combo, "Iron Ball")
    window._refresh_affinity_options()
    window._set_combo_by_data(window.affinity_combo, "Heavy")
    window.aow_combo.setCurrentIndex(window.aow_combo.findData(None))
    window.objective_combo.setCurrentIndex(window.objective_combo.findData("max_ar"))
    window.max_upgrade_spin.setValue(25)
    window.lock_upgrade_exact.setChecked(True)
    window.two_handing_check.setChecked(False)
    window._start_search()
    wait_until(lambda: window.active_run_id is None)
    if not window.current_results:
        raise AssertionError("expected Iron Ball one-hand result")
    one_hand_ar = float(window.current_results[0].ar_total)
    window.two_handing_check.setChecked(True)
    window._start_search()
    wait_until(lambda: window.active_run_id is None)
    if not window.current_results:
        raise AssertionError("expected Iron Ball two-hand result")
    two_hand_ar = float(window.current_results[0].ar_total)
    if abs(one_hand_ar - two_hand_ar) > 0.01:
        raise AssertionError("paired weapon incorrectly gained two-hand AR")

    # One final event pump for queued signals
    QtCore.QTimer.singleShot(1, app.quit)
    app.exec()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
