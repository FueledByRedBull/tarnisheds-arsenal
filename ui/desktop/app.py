from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

from PyQt6 import QtCore, QtWidgets
from PyQt6.QtGui import QFont

try:
    import er_optimizer_core as core
except ImportError as exc:  # pragma: no cover
    raise SystemExit(
        "Failed to import er_optimizer_core. Build/install the extension first."
    ) from exc


OPEN_OPTION = "<Open>"
ALL_OPTION = "<All>"
COMPARE_AOW_MATCH_SELECTED = "<Match Selected>"

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


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


class OptimizeWorker(QtCore.QObject):
    progress = QtCore.pyqtSignal(int, int, int, int, float, int)
    finished = QtCore.pyqtSignal(int, object)
    failed = QtCore.pyqtSignal(int, str)

    def __init__(self, run_id: int, data: Any, kwargs: dict[str, Any]) -> None:
        super().__init__()
        self.run_id = run_id
        self.data = data
        self.kwargs = kwargs

    @QtCore.pyqtSlot()
    def run(self) -> None:
        try:
            results = core.optimize_builds(
                data=self.data,
                progress_cb=self._progress_cb,
                **self.kwargs,
            )
            self.finished.emit(self.run_id, results)
        except Exception as exc:
            self.failed.emit(self.run_id, str(exc))

    def _progress_cb(
        self,
        checked: int,
        total: int,
        eligible: int,
        best_score: float,
        elapsed_ms: int,
    ) -> None:
        self.progress.emit(
            self.run_id,
            checked,
            total,
            eligible,
            best_score,
            elapsed_ms,
        )


class MainWindow(QtWidgets.QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("Tarnished's Arsenal")
        self.setWindowFlag(QtCore.Qt.WindowType.MSWindowsFixedSizeDialogHint, False)
        self.setWindowFlag(QtCore.Qt.WindowType.WindowMinMaxButtonsHint, True)
        self.setMinimumSize(1200, 720)
        self.resize(1600, 980)

        data_path = repo_root() / "data" / "phase1"
        self.data = core.load_game_data(str(data_path))
        self.run_id = 0
        self.active_run_id: int | None = None
        self.worker_thread: QtCore.QThread | None = None
        self.worker: OptimizeWorker | None = None
        self.current_results: list[Any] = []
        self.locked_result_stats: dict[str, int] | None = None
        self.all_weapon_names: list[str] = []
        self.all_affinities: list[str] = []
        self.stat_widgets: dict[str, QtWidgets.QSpinBox] = {}

        self._build_ui()
        self._populate_static_lists()
        self._wire_events()
        self._refresh_affinity_options()
        self._refresh_compare_weapon_options()
        self._set_idle_progress()

    def _build_ui(self) -> None:
        root = QtWidgets.QWidget(self)
        root.setSizePolicy(
            QtWidgets.QSizePolicy.Policy.Expanding,
            QtWidgets.QSizePolicy.Policy.Expanding,
        )
        self.setCentralWidget(root)
        layout = QtWidgets.QHBoxLayout(root)

        left_panel = QtWidgets.QWidget()
        left_panel.setMinimumWidth(340)
        left_panel.setMaximumWidth(420)
        left_outer = QtWidgets.QVBoxLayout(left_panel)
        left_outer.setContentsMargins(0, 0, 0, 0)
        left_outer.setSpacing(8)

        left_content = QtWidgets.QWidget()
        left_content_layout = QtWidgets.QVBoxLayout(left_content)
        left_content_layout.setContentsMargins(0, 0, 0, 0)
        left_content_layout.setSpacing(8)
        left_content_layout.addWidget(self._build_character_group())
        left_content_layout.addWidget(self._build_weapon_group())
        left_content_layout.addWidget(self._build_options_group())
        left_content_layout.addStretch(1)

        left_scroll = QtWidgets.QScrollArea()
        left_scroll.setWidgetResizable(True)
        left_scroll.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        left_scroll.setWidget(left_content)
        left_outer.addWidget(left_scroll, 1)
        left_outer.addWidget(self._build_footer(), 0)

        self.main_tabs = QtWidgets.QTabWidget()
        self.main_tabs.setDocumentMode(True)
        self.main_tabs.addTab(self._build_results_group(), "RESULTS")
        self.main_tabs.addTab(self._build_upgrade_group(), "UPGRADE COMPARISON")

        splitter = QtWidgets.QSplitter()
        splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)
        splitter.setChildrenCollapsible(False)
        splitter.addWidget(left_panel)
        splitter.addWidget(self.main_tabs)
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        splitter.setSizes([360, 1240])
        layout.addWidget(splitter)

    def _build_character_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("CHARACTER")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(8)

        self.class_combo = QtWidgets.QComboBox()
        self.level_spin = self._u16_spin(1, 713, 150)
        self.level_spin.setReadOnly(True)
        self.level_spin.setButtonSymbols(QtWidgets.QAbstractSpinBox.ButtonSymbols.NoButtons)
        self.level_spin.setFocusPolicy(QtCore.Qt.FocusPolicy.NoFocus)
        top_form = QtWidgets.QFormLayout()
        top_form.addRow("Class", self.class_combo)
        top_form.addRow("Character Level (Derived)", self.level_spin)
        layout.addLayout(top_form)

        self.vig_spin = self._u8_spin(1, 99, 40)
        self.mnd_spin = self._u8_spin(1, 99, 20)
        self.end_spin = self._u8_spin(1, 99, 25)
        self.str_spin = self._u8_spin(1, 99, 18)
        self.dex_spin = self._u8_spin(1, 99, 40)
        self.int_spin = self._u8_spin(1, 99, 9)
        self.fai_spin = self._u8_spin(1, 99, 8)
        self.arc_spin = self._u8_spin(1, 99, 45)
        self.stat_widgets = {
            "str": self.str_spin,
            "dex": self.dex_spin,
            "int": self.int_spin,
            "fai": self.fai_spin,
            "arc": self.arc_spin,
        }

        self.min_str_spin = self._u8_spin(0, 99, 0)
        self.min_dex_spin = self._u8_spin(0, 99, 0)
        self.min_int_spin = self._u8_spin(0, 99, 0)
        self.min_fai_spin = self._u8_spin(0, 99, 0)
        self.min_arc_spin = self._u8_spin(0, 99, 0)

        grid = QtWidgets.QGridLayout()
        grid.setColumnStretch(0, 0)
        grid.setColumnStretch(1, 1)
        grid.setColumnStretch(2, 1)
        grid.addWidget(QtWidgets.QLabel(""), 0, 0)
        current_label = QtWidgets.QLabel("Current")
        current_label.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        grid.addWidget(current_label, 0, 1)
        min_label = QtWidgets.QLabel("Min Floor")
        min_label.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        grid.addWidget(min_label, 0, 2)

        self._add_stat_row(grid, 1, "VIG", self.vig_spin, None)
        self._add_stat_row(grid, 2, "MND", self.mnd_spin, None)
        self._add_stat_row(grid, 3, "END", self.end_spin, None)
        self._add_stat_row(grid, 4, "STR", self.str_spin, self.min_str_spin)
        self._add_stat_row(grid, 5, "DEX", self.dex_spin, self.min_dex_spin)
        self._add_stat_row(grid, 6, "INT", self.int_spin, self.min_int_spin)
        self._add_stat_row(grid, 7, "FAI", self.fai_spin, self.min_fai_spin)
        self._add_stat_row(grid, 8, "ARC", self.arc_spin, self.min_arc_spin)
        layout.addLayout(grid)
        return group

    def _build_weapon_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("WEAPON / AFFINITY")
        form = QtWidgets.QFormLayout(group)

        self.weapon_type_combo = QtWidgets.QComboBox()
        self.weapon_combo = QtWidgets.QComboBox()
        self.affinity_combo = QtWidgets.QComboBox()
        self.aow_combo = QtWidgets.QComboBox()

        form.addRow("Weapon Type", self.weapon_type_combo)
        form.addRow("Weapon", self.weapon_combo)
        form.addRow("Affinity", self.affinity_combo)
        form.addRow("AoW", self.aow_combo)
        return group

    def _build_options_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("OPTIONS")
        form = QtWidgets.QFormLayout(group)

        self.max_upgrade_spin = self._u8_spin(0, 25, 25)
        self.top_k_spin = self._u16_spin(1, 50, 10)
        self.lock_upgrade_exact = QtWidgets.QCheckBox("Lock Upgrade Exact")
        self.two_handing_check = QtWidgets.QCheckBox("Two Handing")
        self.lock_stats_checkbox = QtWidgets.QCheckBox("Lock Stats Too (Exact)")

        self.objective_combo = QtWidgets.QComboBox()
        self.objective_combo.addItem("Max AR", "max_ar")
        self.objective_combo.addItem("Max AR + Bleed", "max_ar_plus_bleed")

        self.somber_combo = QtWidgets.QComboBox()
        self.somber_combo.addItem("All", "all")
        self.somber_combo.addItem("Standard Only", "standard_only")
        self.somber_combo.addItem("Somber Only", "somber_only")

        form.addRow("Upgrade", self.max_upgrade_spin)
        form.addRow("Top K", self.top_k_spin)
        form.addRow("Objective", self.objective_combo)
        form.addRow("Somber Filter", self.somber_combo)

        row_a = QtWidgets.QHBoxLayout()
        row_a.addWidget(self.lock_upgrade_exact)
        row_a.addWidget(self.two_handing_check)
        form.addRow("", row_a)
        form.addRow("", self.lock_stats_checkbox)
        return group

    def _build_footer(self) -> QtWidgets.QWidget:
        footer = QtWidgets.QWidget()
        layout = QtWidgets.QVBoxLayout(footer)
        layout.setContentsMargins(4, 4, 4, 4)
        layout.setSpacing(6)

        self.free_points_label = QtWidgets.QLabel("Free Points: -")
        self.estimate_label = QtWidgets.QLabel("Search Space: -")
        self.requirement_label = QtWidgets.QLabel("Requirements: -")
        self.search_button = QtWidgets.QPushButton("Search")
        self.progress_label = QtWidgets.QLabel("Idle")
        self.progress_bar = QtWidgets.QProgressBar()
        self.progress_bar.setTextVisible(True)

        layout.addWidget(self.free_points_label)
        layout.addWidget(self.estimate_label)
        layout.addWidget(self.requirement_label)
        layout.addWidget(self.search_button)
        layout.addWidget(self.progress_label)
        layout.addWidget(self.progress_bar)
        return footer

    def _add_stat_row(
        self,
        grid: QtWidgets.QGridLayout,
        row: int,
        name: str,
        current_spin: QtWidgets.QSpinBox,
        min_spin: QtWidgets.QSpinBox | None,
    ) -> None:
        name_label = QtWidgets.QLabel(name)
        grid.addWidget(name_label, row, 0)
        grid.addWidget(current_spin, row, 1)
        if min_spin is None:
            dash = QtWidgets.QLabel("—")
            dash.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
            dash.setStyleSheet("color: #8f7f5a;")
            grid.addWidget(dash, row, 2)
        else:
            grid.addWidget(min_spin, row, 2)

    def _build_results_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("RESULTS")
        layout = QtWidgets.QVBoxLayout(group)
        self.results_table = QtWidgets.QTableWidget(0, 14)
        self.results_table.setHorizontalHeaderLabels(
            [
                "#",
                "Weapon",
                "Affinity",
                "AoW",
                "Upgrade",
                "STR",
                "DEX",
                "INT",
                "FAI",
                "ARC",
                "AR",
                "Bleed",
                "Score",
                "Lock",
            ]
        )
        self.results_table.horizontalHeader().setSectionResizeMode(
            QtWidgets.QHeaderView.ResizeMode.ResizeToContents
        )
        self.results_table.horizontalHeader().setStretchLastSection(True)
        self.results_table.setSelectionBehavior(
            QtWidgets.QAbstractItemView.SelectionBehavior.SelectRows
        )
        self.results_table.setSelectionMode(
            QtWidgets.QAbstractItemView.SelectionMode.SingleSelection
        )
        self.results_table.setEditTriggers(
            QtWidgets.QAbstractItemView.EditTrigger.NoEditTriggers
        )
        layout.addWidget(self.results_table)
        return group

    def _build_upgrade_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("UPGRADE COMPARISON")
        layout = QtWidgets.QVBoxLayout(group)

        toolbar_top = QtWidgets.QHBoxLayout()
        self.compare_weapon_type_combo = QtWidgets.QComboBox()
        toolbar_top.addWidget(QtWidgets.QLabel("Compare Type"))
        toolbar_top.addWidget(self.compare_weapon_type_combo, 1)
        self.compare_aow_combo = QtWidgets.QComboBox()
        toolbar_top.addWidget(QtWidgets.QLabel("AoW"))
        toolbar_top.addWidget(self.compare_aow_combo, 1)
        layout.addLayout(toolbar_top)

        toolbar_bottom = QtWidgets.QHBoxLayout()
        self.compare_weapon_combo = QtWidgets.QComboBox()
        self.compare_affinity_combo = QtWidgets.QComboBox()
        toolbar_bottom.addWidget(QtWidgets.QLabel("Compare Weapon"))
        toolbar_bottom.addWidget(self.compare_weapon_combo, 2)
        toolbar_bottom.addWidget(QtWidgets.QLabel("Affinity"))
        toolbar_bottom.addWidget(self.compare_affinity_combo, 1)
        layout.addLayout(toolbar_bottom)

        self.upgrade_table = QtWidgets.QTableWidget(0, 0)
        self.upgrade_table.setEditTriggers(
            QtWidgets.QAbstractItemView.EditTrigger.NoEditTriggers
        )
        self.upgrade_table.horizontalHeader().setSectionResizeMode(
            QtWidgets.QHeaderView.ResizeMode.ResizeToContents
        )
        self.upgrade_table.horizontalHeader().setStretchLastSection(True)
        layout.addWidget(self.upgrade_table)
        return group

    def _wire_events(self) -> None:
        self.search_button.clicked.connect(self._start_search)
        self.class_combo.currentIndexChanged.connect(self._on_class_changed)
        self.weapon_combo.currentIndexChanged.connect(self._refresh_affinity_options)
        self.compare_weapon_type_combo.currentIndexChanged.connect(self._refresh_compare_weapon_options)
        self.compare_weapon_combo.currentIndexChanged.connect(self._refresh_compare_affinity_options)
        self.results_table.itemSelectionChanged.connect(self._rebuild_upgrade_table)
        self.lock_stats_checkbox.stateChanged.connect(self._refresh_estimate)
        self.two_handing_check.stateChanged.connect(self._refresh_estimate)
        self.lock_upgrade_exact.stateChanged.connect(self._refresh_estimate)
        self.compare_weapon_combo.currentIndexChanged.connect(self._rebuild_upgrade_table)
        self.compare_affinity_combo.currentIndexChanged.connect(self._rebuild_upgrade_table)
        self.compare_aow_combo.currentIndexChanged.connect(self._rebuild_upgrade_table)

        watched = [
            self.level_spin,
            self.vig_spin,
            self.mnd_spin,
            self.end_spin,
            self.str_spin,
            self.dex_spin,
            self.int_spin,
            self.fai_spin,
            self.arc_spin,
            self.min_str_spin,
            self.min_dex_spin,
            self.min_int_spin,
            self.min_fai_spin,
            self.min_arc_spin,
            self.max_upgrade_spin,
            self.weapon_combo,
            self.affinity_combo,
            self.aow_combo,
            self.objective_combo,
            self.somber_combo,
            self.weapon_type_combo,
        ]
        for widget in watched:
            if isinstance(widget, QtWidgets.QComboBox):
                widget.currentIndexChanged.connect(self._refresh_estimate)
            else:
                widget.valueChanged.connect(self._refresh_estimate)

    def _populate_static_lists(self) -> None:
        self.class_combo.clear()
        for class_name in STARTING_CLASSES:
            self.class_combo.addItem(class_name, class_name)
        self._set_combo_by_data(self.class_combo, "Samurai")

        self.all_weapon_names = self.data.weapon_names()
        affinity_set: set[str] = set()
        for weapon_name in self.all_weapon_names:
            for affinity in self.data.affinities_for_weapon(weapon_name):
                affinity_set.add(affinity)
        self.all_affinities = sorted(affinity_set)

        self.weapon_type_combo.addItem(ALL_OPTION, None)
        for key in self.data.weapon_type_keys():
            self.weapon_type_combo.addItem(key, key)

        self.weapon_combo.addItem(OPEN_OPTION, None)
        for name in self.all_weapon_names:
            self.weapon_combo.addItem(name, name)

        self.compare_weapon_type_combo.addItem(ALL_OPTION, None)
        for key in self.data.weapon_type_keys():
            self.compare_weapon_type_combo.addItem(key, key)

        self.aow_combo.addItem(OPEN_OPTION, None)
        for name in self.data.aow_names():
            self.aow_combo.addItem(name, name)

        self.compare_aow_combo.addItem(COMPARE_AOW_MATCH_SELECTED, "__match_selected__")
        self.compare_aow_combo.addItem(OPEN_OPTION, None)
        for name in self.data.aow_names():
            self.compare_aow_combo.addItem(name, name)

        self._refresh_compare_weapon_options()
        self._enable_searchable_dropdowns()
        self._apply_class_baselines()
        self._refresh_estimate()

    def _refresh_affinity_options(self) -> None:
        selected_weapon = self._combo_value(self.weapon_combo)
        previous = self._combo_value(self.affinity_combo)

        self.affinity_combo.blockSignals(True)
        self.affinity_combo.clear()
        self.affinity_combo.addItem(OPEN_OPTION, None)

        if selected_weapon is None:
            affinities = self.all_affinities
        else:
            affinities = self.data.affinities_for_weapon(selected_weapon)

        for affinity in affinities:
            self.affinity_combo.addItem(affinity, affinity)

        if previous is not None:
            idx = self.affinity_combo.findData(previous)
            if idx >= 0:
                self.affinity_combo.setCurrentIndex(idx)
            else:
                self.affinity_combo.setCurrentIndex(0)
        else:
            self.affinity_combo.setCurrentIndex(0)

        self.affinity_combo.blockSignals(False)
        self._refresh_estimate()

    def _refresh_compare_weapon_options(self) -> None:
        selected_type = self._combo_value(self.compare_weapon_type_combo)
        previous_weapon = self._combo_value(self.compare_weapon_combo)

        self.compare_weapon_combo.blockSignals(True)
        self.compare_weapon_combo.clear()
        self.compare_weapon_combo.addItem(OPEN_OPTION, None)

        if selected_type is None:
            weapon_names = self.all_weapon_names
        else:
            weapon_names = self.data.weapon_names_for_type(selected_type)

        for weapon_name in weapon_names:
            self.compare_weapon_combo.addItem(weapon_name, weapon_name)

        if previous_weapon is not None:
            idx = self.compare_weapon_combo.findData(previous_weapon)
            if idx >= 0:
                self.compare_weapon_combo.setCurrentIndex(idx)
            else:
                self.compare_weapon_combo.setCurrentIndex(0)
        else:
            self.compare_weapon_combo.setCurrentIndex(0)

        self.compare_weapon_combo.blockSignals(False)
        self._refresh_compare_affinity_options()

    def _refresh_compare_affinity_options(self) -> None:
        selected_weapon = self._combo_value(self.compare_weapon_combo)
        previous = self._combo_value(self.compare_affinity_combo)

        self.compare_affinity_combo.blockSignals(True)
        self.compare_affinity_combo.clear()
        self.compare_affinity_combo.addItem(OPEN_OPTION, None)

        if selected_weapon is None:
            affinities = self.all_affinities
        else:
            affinities = self.data.affinities_for_weapon(selected_weapon)

        for affinity in affinities:
            self.compare_affinity_combo.addItem(affinity, affinity)

        if previous is not None:
            idx = self.compare_affinity_combo.findData(previous)
            if idx >= 0:
                self.compare_affinity_combo.setCurrentIndex(idx)
            else:
                self.compare_affinity_combo.setCurrentIndex(0)
        else:
            self.compare_affinity_combo.setCurrentIndex(0)

        self.compare_affinity_combo.blockSignals(False)
        self._rebuild_upgrade_table()

    def _on_class_changed(self) -> None:
        self._apply_class_baselines()
        self._refresh_estimate()

    def _resolved_class_name(self) -> str:
        typed = self.class_combo.currentText().strip()
        if typed in CLASS_BASE_LEVEL_TOTAL:
            return typed
        idx = self.class_combo.currentIndex()
        if idx >= 0:
            return self.class_combo.itemText(idx)
        return STARTING_CLASSES[0]

    def _apply_class_baselines(self) -> None:
        class_name = self._resolved_class_name()
        base_stats = CLASS_BASE_STATS.get(class_name)
        if base_stats is None:
            return

        mapping = {
            "vig": self.vig_spin,
            "mnd": self.mnd_spin,
            "end": self.end_spin,
            "str": self.str_spin,
            "dex": self.dex_spin,
            "int": self.int_spin,
            "fai": self.fai_spin,
            "arc": self.arc_spin,
        }
        for stat_name, spin in mapping.items():
            minimum = int(base_stats[stat_name])
            spin.setMinimum(minimum)
            if spin.value() < minimum:
                spin.setValue(minimum)

        self._sync_derived_level()

    def _current_stat_sum(self) -> int:
        return (
            self.vig_spin.value()
            + self.mnd_spin.value()
            + self.end_spin.value()
            + self.str_spin.value()
            + self.dex_spin.value()
            + self.int_spin.value()
            + self.fai_spin.value()
            + self.arc_spin.value()
        )

    def _derived_level(self) -> int:
        class_name = self._resolved_class_name()
        base_level, base_total = CLASS_BASE_LEVEL_TOTAL[class_name]
        return base_level + (self._current_stat_sum() - base_total)

    def _sync_derived_level(self) -> None:
        level = self._derived_level()
        self.level_spin.blockSignals(True)
        self.level_spin.setValue(level)
        self.level_spin.blockSignals(False)

    def _build_request_kwargs(self, include_progress: bool) -> dict[str, Any]:
        lock_stats = self.lock_stats_checkbox.isChecked() and self.locked_result_stats is not None
        lock_values = (
            self.locked_result_stats
            if lock_stats and self.locked_result_stats is not None
            else {}
        )
        self._sync_derived_level()
        class_name = self._resolved_class_name()
        class_base = CLASS_BASE_STATS[class_name]

        fixed_upgrade = self.max_upgrade_spin.value() if self.lock_upgrade_exact.isChecked() else None

        kwargs = {
            "class_name": class_name,
            "character_level": self._derived_level(),
            "vig": self.vig_spin.value(),
            "mnd": self.mnd_spin.value(),
            "end": self.end_spin.value(),
            # Combat stats are redistributed from class/min floors at this level budget.
            "str_stat": int(class_base["str"]),
            "dex": int(class_base["dex"]),
            "int_stat": int(class_base["int"]),
            "fai": int(class_base["fai"]),
            "arc": int(class_base["arc"]),
            "max_upgrade": self.max_upgrade_spin.value(),
            "fixed_upgrade": fixed_upgrade,
            "two_handing": self.two_handing_check.isChecked(),
            "weapon_name": self._combo_value(self.weapon_combo),
            "affinity": self._combo_value(self.affinity_combo),
            "aow_name": self._combo_value(self.aow_combo),
            "objective": self.objective_combo.currentData(),
            "top_k": self.top_k_spin.value(),
            "weapon_type_key": self._combo_value(self.weapon_type_combo),
            "somber_filter": self.somber_combo.currentData(),
            "min_str": self.min_str_spin.value(),
            "min_dex": self.min_dex_spin.value(),
            "min_int": self.min_int_spin.value(),
            "min_fai": self.min_fai_spin.value(),
            "min_arc": self.min_arc_spin.value(),
            "lock_str": lock_values.get("str"),
            "lock_dex": lock_values.get("dex"),
            "lock_int": lock_values.get("int"),
            "lock_fai": lock_values.get("fai"),
            "lock_arc": lock_values.get("arc"),
        }
        if include_progress:
            kwargs["progress_every"] = 5000
        return kwargs

    def _refresh_estimate(self) -> None:
        self._sync_derived_level()
        try:
            kwargs = self._build_request_kwargs(include_progress=False)
            estimate = core.estimate_search_space(
                data=self.data, **self._estimate_kwargs(kwargs)
            )
            self.estimate_label.setText(
                f"Search Space: {estimate.combinations:,} "
                f"({estimate.weapon_candidates} weapons x {estimate.stat_candidates:,} stat states)"
            )
            self.free_points_label.setText(self._compute_free_points_text())
            self._update_requirement_highlights()
        except Exception as exc:
            self.estimate_label.setText(f"Search Space: invalid ({exc})")
            self.free_points_label.setText("Free Points: invalid")
            self._update_requirement_highlights()

    def _compute_free_points_text(self) -> str:
        class_name = self._resolved_class_name()
        base_level, base_total = CLASS_BASE_LEVEL_TOTAL[class_name]
        base_stats = CLASS_BASE_STATS[class_name]
        level = self._derived_level()
        total = base_total + (level - base_level)
        floor_sum = (
            self.vig_spin.value()
            + self.mnd_spin.value()
            + self.end_spin.value()
            + max(int(base_stats["str"]), self.min_str_spin.value())
            + max(int(base_stats["dex"]), self.min_dex_spin.value())
            + max(int(base_stats["int"]), self.min_int_spin.value())
            + max(int(base_stats["fai"]), self.min_fai_spin.value())
            + max(int(base_stats["arc"]), self.min_arc_spin.value())
        )
        redistributable = total - floor_sum
        return (
            f"Redistributable Combat Points: {redistributable} "
            f"(Level {level}, Budget {total})"
        )

    def _start_search(self) -> None:
        if self.active_run_id is not None:
            return

        try:
            kwargs = self._build_request_kwargs(include_progress=True)
            estimate = core.estimate_search_space(
                data=self.data, **self._estimate_kwargs(kwargs)
            )
            total = int(estimate.combinations)
            if total <= 0:
                self._set_idle_progress("No valid search space for current constraints.")
                return
        except Exception as exc:
            self._set_idle_progress(f"Invalid inputs: {exc}")
            return

        self.run_id += 1
        run_id = self.run_id
        self.active_run_id = run_id
        self.search_button.setEnabled(False)
        self.progress_bar.setRange(0, max(total, 1))
        self.progress_bar.setValue(0)
        self.progress_label.setText(f"Searching 0 / {total:,}...")

        self.worker_thread = QtCore.QThread(self)
        self.worker = OptimizeWorker(run_id=run_id, data=self.data, kwargs=kwargs)
        self.worker.moveToThread(self.worker_thread)
        self.worker_thread.started.connect(self.worker.run)
        self.worker.progress.connect(self._on_progress)
        self.worker.finished.connect(self._on_finished)
        self.worker.failed.connect(self._on_failed)
        self.worker.finished.connect(self._teardown_worker)
        self.worker.failed.connect(self._teardown_worker)
        self.worker_thread.start()

    @QtCore.pyqtSlot(int, int, int, int, float, int)
    def _on_progress(
        self,
        run_id: int,
        checked: int,
        total: int,
        eligible: int,
        best_score: float,
        elapsed_ms: int,
    ) -> None:
        if run_id != self.active_run_id:
            return
        if total > 0:
            self.progress_bar.setRange(0, total)
            self.progress_bar.setValue(min(checked, total))
        else:
            self.progress_bar.setRange(0, 0)
        self.progress_label.setText(
            f"Searching {checked:,} / {total:,} | Eligible {eligible:,} | "
            f"Best {best_score:.2f} | {elapsed_ms / 1000.0:.1f}s"
        )

    @QtCore.pyqtSlot(int, object)
    def _on_finished(self, run_id: int, results: object) -> None:
        if run_id != self.active_run_id:
            return
        self.search_button.setEnabled(True)
        self.active_run_id = None
        self.current_results = list(results)
        self._populate_results_table()
        self._rebuild_upgrade_table()
        self._set_idle_progress(f"Done. {len(self.current_results)} result(s).")

    @QtCore.pyqtSlot(int, str)
    def _on_failed(self, run_id: int, message: str) -> None:
        if run_id != self.active_run_id:
            return
        self.search_button.setEnabled(True)
        self.active_run_id = None
        self._set_idle_progress(f"Failed: {message}")

    @QtCore.pyqtSlot()
    def _teardown_worker(self) -> None:
        if self.worker_thread is not None:
            self.worker_thread.quit()
            self.worker_thread.wait(1000)
        self.worker = None
        self.worker_thread = None

    def _populate_results_table(self) -> None:
        self.results_table.setRowCount(len(self.current_results))
        for row_idx, result in enumerate(self.current_results):
            values = [
                str(row_idx + 1),
                result.weapon_name,
                result.affinity,
                result.aow_name or "-",
                f"+{result.upgrade}",
                str(result.str_stat),
                str(result.dex),
                str(result.int_stat),
                str(result.fai),
                str(result.arc),
                f"{result.ar_total:.2f}",
                f"{result.bleed_buildup_add:.2f}",
                f"{result.score:.2f}",
            ]
            for col_idx, value in enumerate(values):
                self.results_table.setItem(row_idx, col_idx, QtWidgets.QTableWidgetItem(value))

            lock_button = QtWidgets.QPushButton("Use As Locks")
            lock_button.clicked.connect(lambda _checked=False, idx=row_idx: self._lock_from_result(idx))
            self.results_table.setCellWidget(row_idx, 13, lock_button)

        if self.current_results:
            self.results_table.selectRow(0)

    def _lock_from_result(self, row_idx: int) -> None:
        if row_idx >= len(self.current_results):
            return
        result = self.current_results[row_idx]

        self._set_combo_by_data(self.weapon_combo, result.weapon_name)
        self._refresh_affinity_options()
        self._set_combo_by_data(self.affinity_combo, result.affinity)
        self._set_combo_by_data(self.aow_combo, result.aow_name)

        self.max_upgrade_spin.setValue(result.upgrade)
        self.lock_upgrade_exact.setChecked(True)
        self.locked_result_stats = {
            "str": int(result.str_stat),
            "dex": int(result.dex),
            "int": int(result.int_stat),
            "fai": int(result.fai),
            "arc": int(result.arc),
        }
        self._refresh_estimate()
        self._start_search()

    def _rebuild_upgrade_table(self) -> None:
        if not self.current_results:
            self.upgrade_table.setRowCount(0)
            self.upgrade_table.setColumnCount(0)
            return

        selected = self.results_table.selectionModel().selectedRows()
        selected_idx = selected[0].row() if selected else 0
        if selected_idx >= len(self.current_results):
            selected_idx = 0

        max_upgrade = self.max_upgrade_spin.value()
        headers = ["Result"] + [f"+{lv}" for lv in range(0, max_upgrade + 1)]

        self.upgrade_table.setColumnCount(len(headers))
        self.upgrade_table.setHorizontalHeaderLabels(headers)
        compare_weapon = self._combo_value(self.compare_weapon_combo)
        selected_result = self.current_results[selected_idx]

        selected_best = self._best_row_config(
            selected_result.weapon_name,
            selected_result.affinity,
            selected_result.aow_name,
        )
        if selected_best is None:
            selected_best = self._row_config_from_result(selected_result)

        rows_to_render: list[tuple[str, Any]] = [
            (
                f"Selected: {selected_best['weapon_name']} | {selected_best['affinity']} | "
                f"AoW {selected_best['aow_name'] or '-'} | {self._format_best_stats(selected_best)}",
                selected_best,
            )
        ]

        if compare_weapon is None:
            for row_idx in range(0, min(4, len(self.current_results))):
                if row_idx == selected_idx:
                    continue
                row = self.current_results[row_idx]
                row_best = self._best_row_config(row.weapon_name, row.affinity, row.aow_name)
                if row_best is None:
                    row_best = self._row_config_from_result(row)
                rows_to_render.append(
                    (
                        f"Top #{row_idx + 1}: {row_best['weapon_name']} | {row_best['affinity']} | "
                        f"AoW {row_best['aow_name'] or '-'} | {self._format_best_stats(row_best)}",
                        row_best,
                    )
                )
        else:
            compare_affinity = self._combo_value(self.compare_affinity_combo)
            if compare_affinity is None:
                options = self.data.affinities_for_weapon(compare_weapon)
                if options:
                    compare_affinity = options[0]
                    self._set_combo_by_data(self.compare_affinity_combo, compare_affinity)
            compare_aow_value = self._combo_value(self.compare_aow_combo)
            if compare_aow_value == "__match_selected__":
                compare_aow = selected_best["aow_name"]
            else:
                compare_aow = compare_aow_value
            if compare_affinity is not None:
                compare_best = self._best_row_config(
                    compare_weapon,
                    compare_affinity,
                    compare_aow,
                )
                if compare_best is not None:
                    compare_label = (
                        f"Compare: {compare_best['weapon_name']} | {compare_best['affinity']} | "
                        f"AoW {compare_best['aow_name'] or '-'} | {self._format_best_stats(compare_best)}"
                    )
                else:
                    compare_label = (
                        f"Compare: {compare_weapon} | {compare_affinity} | AoW {compare_aow or '-'} | "
                        "No valid build"
                    )
                rows_to_render.append(
                    (
                        compare_label,
                        compare_best,
                    )
                )

        self.upgrade_table.setRowCount(len(rows_to_render))
        for row_idx, (label, row_data) in enumerate(rows_to_render):
            self.upgrade_table.setItem(row_idx, 0, QtWidgets.QTableWidgetItem(label))
            for lv in range(0, max_upgrade + 1):
                col = lv + 1
                ar = self._compute_locked_ar_for_config(row_data, lv)
                text = "-" if ar is None else f"{ar:.2f}"
                self.upgrade_table.setItem(row_idx, col, QtWidgets.QTableWidgetItem(text))

    def _compute_locked_ar_for_config(self, row_data: Any, level: int) -> float | None:
        if row_data is None:
            return None

        weapon_name = row_data["weapon_name"]
        affinity = row_data["affinity"]
        aow_name = row_data["aow_name"]
        lock_str = row_data["str_stat"]
        lock_dex = row_data["dex"]
        lock_int = row_data["int_stat"]
        lock_fai = row_data["fai"]
        lock_arc = row_data["arc"]

        kwargs = self._build_request_kwargs(include_progress=False)
        kwargs.update(
            {
                "weapon_name": weapon_name,
                "affinity": affinity,
                "aow_name": aow_name,
                "objective": self.objective_combo.currentData(),
                "top_k": 1,
                "fixed_upgrade": level,
                "max_upgrade": self.max_upgrade_spin.value(),
                "somber_filter": "all",
                "weapon_type_key": None,
                "min_str": 0,
                "min_dex": 0,
                "min_int": 0,
                "min_fai": 0,
                "min_arc": 0,
                "lock_str": lock_str,
                "lock_dex": lock_dex,
                "lock_int": lock_int,
                "lock_fai": lock_fai,
                "lock_arc": lock_arc,
            }
        )
        try:
            rows = core.optimize_builds(data=self.data, **kwargs)
        except Exception:
            return None
        if not rows:
            return None
        return float(rows[0].ar_total)

    def _best_row_config(
        self,
        weapon_name: str,
        affinity: str,
        aow_name: Any,
    ) -> dict[str, Any] | None:
        kwargs = self._build_request_kwargs(include_progress=False)
        kwargs.update(
            {
                "weapon_name": weapon_name,
                "affinity": affinity,
                "aow_name": aow_name,
                "top_k": 1,
                "weapon_type_key": None,
                "somber_filter": "all",
                "lock_str": None,
                "lock_dex": None,
                "lock_int": None,
                "lock_fai": None,
                "lock_arc": None,
            }
        )
        try:
            rows = core.optimize_builds(data=self.data, **kwargs)
        except Exception:
            return None
        if not rows:
            return None
        return self._row_config_from_result(rows[0])

    def _row_config_from_result(self, result: Any) -> dict[str, Any]:
        return {
            "weapon_name": result.weapon_name,
            "affinity": result.affinity,
            "aow_name": result.aow_name,
            "str_stat": int(result.str_stat),
            "dex": int(result.dex),
            "int_stat": int(result.int_stat),
            "fai": int(result.fai),
            "arc": int(result.arc),
            "best_upgrade": int(result.upgrade),
            "best_ar_total": float(result.ar_total),
        }

    def _format_best_stats(self, row_data: dict[str, Any]) -> str:
        return (
            f"Best +{row_data['best_upgrade']} "
            f"STR {row_data['str_stat']} DEX {row_data['dex']} "
            f"INT {row_data['int_stat']} FAI {row_data['fai']} ARC {row_data['arc']} "
            f"AR {row_data['best_ar_total']:.2f}"
        )

    def _update_requirement_highlights(self) -> None:
        selected_weapon = self._combo_value(self.weapon_combo)
        selected_affinity = self._combo_value(self.affinity_combo)
        if selected_weapon is None:
            self.requirement_label.setText("Requirements: -")
            for widget in self.stat_widgets.values():
                self._set_req_fail(widget, False)
            return

        try:
            req_str, req_dex, req_int, req_fai, req_arc = self.data.weapon_requirements(
                selected_weapon,
                selected_affinity,
            )
        except Exception:
            self.requirement_label.setText("Requirements: -")
            for widget in self.stat_widgets.values():
                self._set_req_fail(widget, False)
            return

        effective_str = self.str_spin.value()
        if self.two_handing_check.isChecked():
            effective_str = min(99, int(self.str_spin.value() * 1.5))

        self.requirement_label.setText(
            "Requirements: "
            f"STR {req_str} / DEX {req_dex} / INT {req_int} / FAI {req_fai} / ARC {req_arc}"
        )

        self._set_req_fail(self.str_spin, effective_str < req_str)
        self._set_req_fail(self.dex_spin, self.dex_spin.value() < req_dex)
        self._set_req_fail(self.int_spin, self.int_spin.value() < req_int)
        self._set_req_fail(self.fai_spin, self.fai_spin.value() < req_fai)
        self._set_req_fail(self.arc_spin, self.arc_spin.value() < req_arc)

    @staticmethod
    def _set_req_fail(widget: QtWidgets.QSpinBox, failed: bool) -> None:
        widget.setProperty("reqFail", failed)
        widget.style().unpolish(widget)
        widget.style().polish(widget)

    def _set_idle_progress(self, message: str = "Idle") -> None:
        self.progress_label.setText(message)
        self.progress_bar.setRange(0, 1)
        self.progress_bar.setValue(0)

    def _enable_searchable_dropdowns(self) -> None:
        combos = [
            self.class_combo,
            self.objective_combo,
            self.somber_combo,
            self.weapon_type_combo,
            self.weapon_combo,
            self.affinity_combo,
            self.aow_combo,
            self.compare_weapon_type_combo,
            self.compare_weapon_combo,
            self.compare_affinity_combo,
            self.compare_aow_combo,
        ]
        for combo in combos:
            self._enable_search_for_combo(combo)

    def _enable_search_for_combo(self, combo: QtWidgets.QComboBox) -> None:
        combo.setEditable(True)
        combo.setInsertPolicy(QtWidgets.QComboBox.InsertPolicy.NoInsert)
        completer = combo.completer()
        if completer is not None:
            completer.setCaseSensitivity(QtCore.Qt.CaseSensitivity.CaseInsensitive)
            completer.setCompletionMode(QtWidgets.QCompleter.CompletionMode.PopupCompletion)
            try:
                completer.setFilterMode(QtCore.Qt.MatchFlag.MatchContains)
            except Exception:
                pass

        line_edit = combo.lineEdit()
        if line_edit is not None:
            line_edit.setClearButtonEnabled(True)
            line_edit.editingFinished.connect(
                lambda c=combo: self._sync_combo_index_from_text(c)
            )

    def _sync_combo_index_from_text(self, combo: QtWidgets.QComboBox) -> None:
        text = combo.currentText().strip()
        if not text:
            return
        idx = self._find_index_by_text(combo, text)
        if idx >= 0:
            if idx != combo.currentIndex():
                combo.setCurrentIndex(idx)
            return
        current_idx = combo.currentIndex()
        if current_idx >= 0:
            combo.setEditText(combo.itemText(current_idx))

    @staticmethod
    def _estimate_kwargs(kwargs: dict[str, Any]) -> dict[str, Any]:
        out = dict(kwargs)
        out.pop("top_k", None)
        out.pop("progress_every", None)
        return out

    @staticmethod
    def _u8_spin(minimum: int, maximum: int, value: int) -> QtWidgets.QSpinBox:
        spin = QtWidgets.QSpinBox()
        spin.setRange(minimum, maximum)
        spin.setValue(value)
        return spin

    @staticmethod
    def _u16_spin(minimum: int, maximum: int, value: int) -> QtWidgets.QSpinBox:
        spin = QtWidgets.QSpinBox()
        spin.setRange(minimum, maximum)
        spin.setValue(value)
        return spin

    @staticmethod
    def _combo_value(combo: QtWidgets.QComboBox) -> Any:
        if combo.isEditable():
            text = combo.currentText().strip()
            if not text:
                return combo.currentData()
            idx = MainWindow._find_index_by_text(combo, text)
            if idx >= 0:
                return combo.itemData(idx)
            return combo.currentData()
        return combo.currentData()

    @staticmethod
    def _find_index_by_text(combo: QtWidgets.QComboBox, text: str) -> int:
        for idx in range(combo.count()):
            if combo.itemText(idx).strip().lower() == text.strip().lower():
                return idx
        return -1

    @staticmethod
    def _set_combo_by_data(combo: QtWidgets.QComboBox, value: Any) -> None:
        if value is None:
            combo.setCurrentIndex(0)
            return
        idx = combo.findData(value)
        if idx < 0:
            combo.setCurrentIndex(0)
        else:
            combo.setCurrentIndex(idx)


def apply_dark_theme(app: QtWidgets.QApplication) -> None:
    app.setStyle("Fusion")
    app.setFont(QFont("Palatino Linotype", 9))
    app.setStyleSheet(
        """
        QWidget { background: #0d0b08; color: #d4c9a8; }
        QGroupBox {
            border: 1px solid #4a3a1e;
            border-radius: 6px;
            margin-top: 8px;
            padding-top: 8px;
            background: #13110e;
        }
        QGroupBox::title {
            subcontrol-origin: margin;
            left: 10px;
            padding: 0 6px;
            color: #c8a84b;
            font-weight: bold;
            letter-spacing: 2px;
            text-transform: uppercase;
        }
        QLineEdit, QSpinBox, QComboBox, QTableWidget {
            background: #13110e;
            border: 1px solid #4a3a1e;
            border-radius: 4px;
            padding: 3px;
            color: #d4c9a8;
        }
        QComboBox QAbstractItemView {
            background: #13110e;
            selection-background-color: #3a2c10;
            color: #d4c9a8;
        }
        QHeaderView::section {
            background: #1a1510;
            color: #c8a84b;
            border: 1px solid #4a3a1e;
            padding: 4px 8px;
            font-weight: bold;
            letter-spacing: 1px;
        }
        QTabWidget::pane {
            border: 1px solid #4a3a1e;
            top: -1px;
            background: #13110e;
        }
        QTabBar::tab {
            background: #1a1510;
            color: #b99d5c;
            border: 1px solid #4a3a1e;
            border-bottom: none;
            padding: 6px 12px;
            min-width: 140px;
            font-weight: bold;
            letter-spacing: 1px;
        }
        QTabBar::tab:selected {
            background: #2e2410;
            color: #e8d48a;
            border-color: #c8a84b;
        }
        QTabBar::tab:!selected {
            margin-top: 2px;
        }
        QProgressBar {
            background: #13110e;
            border: 1px solid #4a3a1e;
            border-radius: 2px;
            text-align: center;
            color: #c8a84b;
        }
        QProgressBar::chunk {
            background: qlineargradient(x1:0, y1:0, x2:1, y2:0,
                stop:0 #8a6520, stop:0.5 #c8a84b, stop:1 #8a6520);
            border-radius: 2px;
        }
        QSpinBox[reqFail="true"] {
            background: #2a0f0f;
            border: 1px solid #d46872;
            color: #ffd5d9;
        }
        QPushButton {
            background: #1e1810;
            border: 1px solid #c8a84b;
            border-bottom: 2px solid #a07830;
            border-radius: 2px;
            padding: 8px 16px;
            color: #e8d48a;
            letter-spacing: 1px;
            font-weight: bold;
        }
        QPushButton:hover {
            background: #2e2410;
            border-color: #e8c860;
            color: #f5e4a0;
        }
        QPushButton:disabled { background: #1a1510; color: #8f7f5a; border-color: #6b5630; }
        QTableWidget::item:selected { background: #3a2c10; color: #f5e4a0; }
        QSplitter::handle {
            background: #4a3a1e;
            width: 2px;
            height: 2px;
        }
        QScrollBar:vertical {
            background: #13110e;
            width: 10px;
            border: 1px solid #4a3a1e;
            margin: 0;
        }
        QScrollBar::handle:vertical {
            background: #7a602d;
            min-height: 22px;
            border: 1px solid #c8a84b;
            border-radius: 2px;
        }
        QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {
            height: 0;
        }
        QScrollBar:horizontal {
            background: #13110e;
            height: 10px;
            border: 1px solid #4a3a1e;
            margin: 0;
        }
        QScrollBar::handle:horizontal {
            background: #7a602d;
            min-width: 22px;
            border: 1px solid #c8a84b;
            border-radius: 2px;
        }
        QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {
            width: 0;
        }
        """
    )


def main() -> int:
    app = QtWidgets.QApplication(sys.argv)
    apply_dark_theme(app)
    window = MainWindow()
    window.show()
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
