from __future__ import annotations

import importlib.util
import os
import time
from pathlib import Path

from PyQt6 import QtCore, QtWidgets


def load_app_module(project_root: Path):
    module_path = project_root / "ui" / "desktop" / "app.py"
    spec = importlib.util.spec_from_file_location("er_optimizer_ui", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load app module spec from {module_path}")
    module = importlib.util.module_from_spec(spec)
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

    # Lock from first row (triggers rerun)
    window._lock_from_result(0)
    wait_until(lambda: window.active_run_id is None)
    if len(window.current_results) == 0:
        raise AssertionError("expected results after lock rerun")

    # Explicit side-by-side compare row
    window._set_combo_by_data(window.compare_weapon_combo, "Nagakiba")
    window._refresh_compare_affinity_options()
    if window.compare_affinity_combo.count() <= 1:
        raise AssertionError("compare affinity options were not populated")
    window.compare_affinity_combo.setCurrentIndex(1)
    window._rebuild_upgrade_table()
    if window.upgrade_table.rowCount() < 2:
        raise AssertionError("expected selected + compare rows in upgrade table")

    # One final event pump for queued signals
    QtCore.QTimer.singleShot(1, app.quit)
    app.exec()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
