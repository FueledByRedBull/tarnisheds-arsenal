from __future__ import annotations

import os
import sys
import time
from pathlib import Path

from PyQt6 import QtWidgets

from smoke_ui import load_app_module, wait_until


def process_events() -> None:
    for _ in range(5):
        QtWidgets.QApplication.processEvents()
        time.sleep(0.02)


def save_window(window: QtWidgets.QWidget, path: Path, width: int, height: int) -> None:
    window.resize(width, height)
    window.show()
    process_events()
    pixmap = window.grab()
    path.parent.mkdir(parents=True, exist_ok=True)
    if not pixmap.save(str(path)):
        raise RuntimeError(f"failed to save screenshot: {path}")


def main() -> int:
    if os.name != "nt":
        os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    project_root = Path(__file__).resolve().parents[2]
    out_dir = project_root / "dist" / "phase4_visual_qa"
    app_module = load_app_module(project_root)

    app = QtWidgets.QApplication([])
    app_module.apply_dark_theme(app)
    window = app_module.MainWindow()

    save_window(window, out_dir / "initial_empty_1600.png", 1600, 980)
    save_window(window, out_dir / "initial_empty_1200.png", 1200, 760)

    window._set_combo_by_data(window.class_combo, "Wretch")
    window._on_class_changed()
    window._set_combo_by_data(window.weapon_combo, "Uchigatana")
    window._refresh_affinity_options()
    window._set_combo_by_data(window.affinity_combo, "Keen")
    window.str_spin.setValue(12)
    window.dex_spin.setValue(15)
    window.max_upgrade_spin.setValue(1)
    window.top_k_spin.setValue(5)
    window._start_search()
    wait_until(lambda: window.active_run_id is None)
    save_window(window, out_dir / "rankings_populated_1600.png", 1600, 980)

    window._set_combo_by_data(window.compare_weapon_combo, "Uchigatana")
    window._refresh_compare_affinity_options()
    window._set_combo_by_data(window.compare_affinity_combo, "Occult")
    window._rebuild_upgrade_table()
    window.main_tabs.setCurrentIndex(1)
    save_window(window, out_dir / "compare_explicit_target_1600.png", 1600, 980)

    window.level_path_horizon_spin.setValue(3)
    window.path_tab_open_button.click()
    wait_until(lambda: window.path_thread is None)
    save_window(window, out_dir / "paths_populated_1600.png", 1600, 980)

    window.affinity_tab_open_button.click()
    wait_until(lambda: window.affinity_watch_thread is None)
    save_window(window, out_dir / "affinity_watch_populated_1600.png", 1600, 980)

    app.quit()
    print(f"Saved visual QA screenshots to {out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
