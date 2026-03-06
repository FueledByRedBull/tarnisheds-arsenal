from __future__ import annotations

import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from PyQt6 import QtCore, QtWidgets
from PyQt6.QtGui import QColor, QFont, QPainter, QPainterPath, QPen

try:
    import er_optimizer_core as core
except ImportError as exc:  # pragma: no cover
    raise SystemExit(
        "Failed to import er_optimizer_core. Build/install the extension first."
    ) from exc


OPEN_OPTION = "<Open>"
ALL_OPTION = "<All>"
COMPARE_AOW_MATCH_SELECTED = "<Match Selected>"
QT_PROGRESS_MAX = 2_147_483_647

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

FONT_FAMILY = "Palatino Linotype"
THEME = {
    "bg": "#090a0c",
    "panel": "#101216",
    "panel_alt": "#15181d",
    "panel_soft": "#1a1e24",
    "hero": "#0e1116",
    "input": "#12151a",
    "border": "#2f3640",
    "border_bright": "#c9a44c",
    "text": "#d8cfbd",
    "text_soft": "#978d7a",
    "text_bright": "#f4e7c2",
    "accent": "#c9a44c",
    "accent_deep": "#896520",
    "accent_dark": "#272114",
    "success_bg": "#162016",
    "success_border": "#7d9f47",
    "danger_bg": "#251113",
    "danger_border": "#d46872",
    "info_bg": "#171a20",
    "muted_bg": "#0f1114",
    "row_alt": "#0f1217",
}


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class CombatState:
    str_stat: int
    dex: int
    int_stat: int
    fai: int
    arc: int

    def add_point(self, stat_key: str) -> CombatState | None:
        field_map = {
            "str": "str_stat",
            "dex": "dex",
            "int": "int_stat",
            "fai": "fai",
            "arc": "arc",
        }
        field_name = field_map[stat_key]
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
class PathWeaponConfig:
    title: str
    weapon_name: str
    affinity: str
    aow_name: str | None
    upgrade: int
    start_state: CombatState


@dataclass(frozen=True)
class PathStep:
    level: int
    stats: CombatState
    ar: float | None
    score: float | None
    added_stat: str | None
    requirement_gap: int


@dataclass(frozen=True)
class PathPreview:
    config: PathWeaponConfig
    steps: tuple[PathStep, ...]


class PathChartWidget(QtWidgets.QWidget):
    def __init__(self, parent: QtWidgets.QWidget | None = None) -> None:
        super().__init__(parent)
        self.previews: list[PathPreview] = []
        self.series_colors = [QColor("#c9a44c"), QColor("#b8643c")]
        self.setMinimumHeight(280)

    def set_previews(self, previews: list[PathPreview]) -> None:
        self.previews = previews
        self.update()

    def paintEvent(self, _event: Any) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.fillRect(self.rect(), QColor(THEME["panel_alt"]))
        painter.setPen(QPen(QColor(THEME["border"]), 1))
        painter.drawRoundedRect(self.rect().adjusted(0, 0, -1, -1), 8, 8)

        valid_points = [
            (step.level, step.ar)
            for preview in self.previews
            for step in preview.steps
            if step.ar is not None
        ]
        if not valid_points:
            painter.setPen(QColor(THEME["text_soft"]))
            painter.drawText(
                self.rect().adjusted(18, 18, -18, -18),
                QtCore.Qt.AlignmentFlag.AlignCenter,
                "No valid metric path yet for the selected comparison.",
            )
            return

        levels = [level for level, _ in valid_points]
        ars = [float(ar) for _, ar in valid_points if ar is not None]
        level_min = min(levels)
        level_max = max(levels)
        ar_min = min(ars)
        ar_max = max(ars)
        if level_min == level_max:
            level_max += 1
        if abs(ar_max - ar_min) < 0.01:
            ar_max += 1.0

        chart_rect = self.rect().adjusted(54, 24, -22, -44)
        painter.setPen(QPen(QColor(THEME["border"]), 1))
        for idx in range(5):
            ratio = idx / 4
            y = chart_rect.bottom() - ratio * chart_rect.height()
            painter.drawLine(
                int(chart_rect.left()),
                int(y),
                int(chart_rect.right()),
                int(y),
            )

        painter.setPen(QColor(THEME["text_soft"]))
        painter.drawText(
            QtCore.QRectF(chart_rect.left(), chart_rect.bottom() + 8, 80, 20),
            f"Lv {level_min}",
        )
        painter.drawText(
            QtCore.QRectF(chart_rect.right() - 80, chart_rect.bottom() + 8, 80, 20),
            QtCore.Qt.AlignmentFlag.AlignRight,
            f"Lv {level_max}",
        )
        painter.drawText(
            QtCore.QRectF(10, chart_rect.top() - 6, 40, 20),
            QtCore.Qt.AlignmentFlag.AlignLeft,
            f"{ar_max:.0f}",
        )
        painter.drawText(
            QtCore.QRectF(10, chart_rect.bottom() - 10, 40, 20),
            QtCore.Qt.AlignmentFlag.AlignLeft,
            f"{ar_min:.0f}",
        )

        legend_x = chart_rect.left()
        for idx, preview in enumerate(self.previews):
            color = self.series_colors[idx % len(self.series_colors)]
            painter.setPen(QPen(color, 2))
            painter.drawLine(legend_x, 10, legend_x + 18, 10)
            painter.setPen(QColor(THEME["text"]))
            painter.drawText(
                QtCore.QRectF(legend_x + 24, 2, 260, 18),
                preview.config.title,
            )
            legend_x += 290

        for idx, preview in enumerate(self.previews):
            color = self.series_colors[idx % len(self.series_colors)]
            pen = QPen(color, 2.5)
            painter.setPen(pen)
            path = QPainterPath()
            started = False
            points: list[QtCore.QPointF] = []
            for step in preview.steps:
                if step.ar is None:
                    started = False
                    continue
                x_ratio = (step.level - level_min) / (level_max - level_min)
                y_ratio = (step.ar - ar_min) / (ar_max - ar_min)
                point = QtCore.QPointF(
                    chart_rect.left() + x_ratio * chart_rect.width(),
                    chart_rect.bottom() - y_ratio * chart_rect.height(),
                )
                points.append(point)
                if not started:
                    path.moveTo(point)
                    started = True
                else:
                    path.lineTo(point)
            painter.drawPath(path)
            brush = color
            for point in points if len(points) <= 60 else points[:: max(1, len(points) // 24)]:
                painter.setBrush(brush)
                painter.drawEllipse(point, 3.0, 3.0)


class LevelPathDialog(QtWidgets.QDialog):
    def __init__(
        self,
        parent: QtWidgets.QWidget,
        previews: list[PathPreview],
        start_level: int,
        levels_ahead: int,
    ) -> None:
        super().__init__(parent)
        self.setWindowTitle("Level Path Preview")
        self.resize(1180, 760)

        layout = QtWidgets.QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(12)

        heading = QtWidgets.QLabel(
            f"Current +{levels_ahead} horizon-target combat path from level {start_level}"
        )
        heading.setProperty("role", "cardTitle")
        layout.addWidget(heading)

        subtitle = QtWidgets.QLabel(
            "Each lane starts from the current best solved build at this level, then solves the exact best end-state at Current + N and orders the required points into that target."
        )
        subtitle.setProperty("role", "sectionHint")
        subtitle.setWordWrap(True)
        layout.addWidget(subtitle)

        chart = PathChartWidget()
        chart.set_previews(previews)
        layout.addWidget(chart)

        tables = QtWidgets.QSplitter(QtCore.Qt.Orientation.Horizontal)
        tables.setChildrenCollapsible(False)
        for preview in previews:
            tables.addWidget(self._build_path_table(preview))
        layout.addWidget(tables, 1)

    def _build_path_table(self, preview: PathPreview) -> QtWidgets.QWidget:
        shell = QtWidgets.QGroupBox(preview.config.title.upper())
        layout = QtWidgets.QVBoxLayout(shell)
        layout.setSpacing(8)

        summary = QtWidgets.QLabel(
            f"{preview.config.weapon_name} | {preview.config.affinity} | AoW {preview.config.aow_name or '-'} | +{preview.config.upgrade}"
        )
        summary.setProperty("role", "summaryBody")
        layout.addWidget(summary)

        table = QtWidgets.QTableWidget(len(preview.steps), 5)
        table.setHorizontalHeaderLabels(["Level", "Metric", "Gain", "Added", "Stats"])
        table.horizontalHeader().setSectionResizeMode(QtWidgets.QHeaderView.ResizeMode.ResizeToContents)
        table.horizontalHeader().setStretchLastSection(True)
        table.verticalHeader().setVisible(False)
        table.setEditTriggers(QtWidgets.QAbstractItemView.EditTrigger.NoEditTriggers)
        table.setSelectionMode(QtWidgets.QAbstractItemView.SelectionMode.NoSelection)
        table.setAlternatingRowColors(True)
        table.setShowGrid(False)

        last_ar: float | None = None
        for row_idx, step in enumerate(preview.steps):
            gain_text = "--"
            if step.ar is not None and last_ar is not None:
                gain_text = f"{step.ar - last_ar:+.2f}"
            ar_text = "-" if step.ar is None else f"{step.ar:.2f}"
            added_text = step.added_stat.upper() if step.added_stat is not None else "START"
            if step.ar is None and step.requirement_gap > 0:
                added_text = f"{added_text} (gap {step.requirement_gap})"

            values = [
                str(step.level),
                ar_text,
                gain_text,
                added_text,
                step.stats.summary(),
            ]
            for col_idx, value in enumerate(values):
                table.setItem(row_idx, col_idx, QtWidgets.QTableWidgetItem(value))
            if step.ar is not None:
                last_ar = step.ar

        layout.addWidget(table, 1)
        return shell


class OptimizeWorker(QtCore.QObject):
    progress = QtCore.pyqtSignal(int, object, object, object, float, object)
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
        self.results_signature: tuple[Any, ...] | None = None
        self.active_request_signature: tuple[Any, ...] | None = None
        self.discard_active_results = False
        self.locked_result_stats: dict[str, int] | None = None
        self.all_weapon_names: list[str] = []
        self.all_affinities: list[str] = []
        self.stat_widgets: dict[str, QtWidgets.QSpinBox] = {}
        self.best_row_cache: dict[tuple[Any, ...], dict[str, Any] | None] = {}
        self.locked_ar_cache: dict[tuple[Any, ...], dict[int, float]] = {}
        self.path_eval_cache: dict[tuple[Any, ...], PathStep] = {}
        self.path_target_cache: dict[tuple[Any, ...], dict[str, Any] | None] = {}
        self.scaling_cache: dict[tuple[str, str], tuple[float, float, float, float, float]] = {}
        self.result_cards: list[dict[str, Any]] = []
        self.active_compare_selected: dict[str, Any] | None = None
        self.active_compare_target: dict[str, Any] | None = None

        self._build_ui()
        self._populate_static_lists()
        self._wire_events()
        self._refresh_affinity_options()
        self._refresh_compare_weapon_options()
        self._set_idle_progress()

    def _build_ui(self) -> None:
        root = QtWidgets.QWidget(self)
        root.setObjectName("RootShell")
        root.setSizePolicy(
            QtWidgets.QSizePolicy.Policy.Expanding,
            QtWidgets.QSizePolicy.Policy.Expanding,
        )
        self.setCentralWidget(root)
        layout = QtWidgets.QHBoxLayout(root)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(14)

        left_panel = QtWidgets.QWidget()
        left_panel.setObjectName("LeftRail")
        left_panel.setMinimumWidth(360)
        left_panel.setMaximumWidth(460)
        left_outer = QtWidgets.QVBoxLayout(left_panel)
        left_outer.setContentsMargins(0, 0, 0, 0)
        left_outer.setSpacing(10)

        left_content = QtWidgets.QWidget()
        left_content_layout = QtWidgets.QVBoxLayout(left_content)
        left_content_layout.setContentsMargins(0, 0, 0, 0)
        left_content_layout.setSpacing(10)
        left_content_layout.addWidget(self._build_character_group())
        left_content_layout.addWidget(self._build_weapon_group())
        left_content_layout.addStretch(1)

        left_scroll = QtWidgets.QScrollArea()
        left_scroll.setWidgetResizable(True)
        left_scroll.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        left_scroll.setWidget(left_content)
        left_outer.addWidget(left_scroll, 1)
        left_outer.addWidget(self._build_options_group(), 0)

        right_panel = QtWidgets.QWidget()
        right_panel.setObjectName("RightStage")
        right_layout = QtWidgets.QVBoxLayout(right_panel)
        right_layout.setContentsMargins(0, 0, 0, 0)
        right_layout.setSpacing(12)
        right_layout.addWidget(self._build_hero_header(), 0)
        self.main_tabs = QtWidgets.QTabWidget()
        self.main_tabs.setDocumentMode(True)
        self.main_tabs.addTab(self._build_results_group(), "RESULTS")
        self.main_tabs.addTab(self._build_upgrade_group(), "UPGRADE COMPARISON")
        right_layout.addWidget(self.main_tabs, 1)

        splitter = QtWidgets.QSplitter()
        splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)
        splitter.setChildrenCollapsible(False)
        splitter.addWidget(left_panel)
        splitter.addWidget(right_panel)
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        splitter.setSizes([392, 1208])
        layout.addWidget(splitter)

    def _build_character_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("BUILD")
        group.setObjectName("BuildGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(10)
        layout.addWidget(
            self._helper_label(
                "Set the class shell first. VIG, MND, and END stay fixed while combat stats can move inside floors."
            )
        )

        self.class_combo = QtWidgets.QComboBox()
        self.level_spin = self._u16_spin(1, 713, 150)
        self.level_spin.setReadOnly(True)
        self.level_spin.setButtonSymbols(QtWidgets.QAbstractSpinBox.ButtonSymbols.NoButtons)
        self.level_spin.setFocusPolicy(QtCore.Qt.FocusPolicy.NoFocus)

        top_row = QtWidgets.QGridLayout()
        top_row.setHorizontalSpacing(10)
        top_row.addWidget(self._field_stack("Starting Class", self.class_combo), 0, 0)
        top_row.addWidget(self._field_stack("Derived Level", self.level_spin), 0, 1)
        layout.addLayout(top_row)

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
        grid.setHorizontalSpacing(8)
        grid.setVerticalSpacing(6)
        grid.setColumnStretch(0, 0)
        grid.setColumnStretch(1, 1)
        grid.setColumnStretch(2, 1)
        stat_header = QtWidgets.QLabel("STAT")
        stat_header.setProperty("role", "gridHeader")
        current_label = QtWidgets.QLabel("CURRENT")
        current_label.setProperty("role", "gridHeader")
        current_label.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        min_label = QtWidgets.QLabel("MIN FLOOR")
        min_label.setProperty("role", "gridHeader")
        min_label.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        grid.addWidget(stat_header, 0, 0)
        grid.addWidget(current_label, 0, 1)
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
        group = QtWidgets.QGroupBox("CONSTRAINTS")
        group.setObjectName("ConstraintsGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(10)
        layout.addWidget(
            self._helper_label(
                "Lock a lane when you know it. Leave selectors open when you want the optimizer to roam."
            )
        )

        self.weapon_type_combo = QtWidgets.QComboBox()
        self.weapon_combo = QtWidgets.QComboBox()
        self.affinity_combo = QtWidgets.QComboBox()
        self.aow_combo = QtWidgets.QComboBox()
        self.somber_combo = QtWidgets.QComboBox()
        self.somber_combo.addItem("All", "all")
        self.somber_combo.addItem("Standard Only", "standard_only")
        self.somber_combo.addItem("Somber Only", "somber_only")
        self.max_upgrade_spin = self._u8_spin(0, 25, 25)
        self.top_k_spin = self._u16_spin(1, 50, 10)

        upper_grid = QtWidgets.QGridLayout()
        upper_grid.setHorizontalSpacing(10)
        upper_grid.setVerticalSpacing(8)
        upper_grid.addWidget(self._field_stack("Weapon Type", self.weapon_type_combo), 0, 0)
        upper_grid.addWidget(self._field_stack("Weapon", self.weapon_combo), 0, 1)
        upper_grid.addWidget(self._field_stack("Affinity", self.affinity_combo), 1, 0)
        upper_grid.addWidget(self._field_stack("AoW", self.aow_combo), 1, 1)
        layout.addLayout(upper_grid)

        lower_grid = QtWidgets.QGridLayout()
        lower_grid.setHorizontalSpacing(10)
        lower_grid.setVerticalSpacing(8)
        lower_grid.addWidget(self._field_stack("Somber Filter", self.somber_combo), 0, 0)
        lower_grid.addWidget(self._field_stack("Max Upgrade", self.max_upgrade_spin), 0, 1)
        lower_grid.addWidget(self._field_stack("Top Results", self.top_k_spin), 1, 0)
        layout.addLayout(lower_grid)
        return group

    def _build_options_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("SEARCH")
        group.setObjectName("SearchGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(10)
        layout.addWidget(
            self._helper_label(
                "Choose the score rule, keep an eye on requirement health, then launch the run."
            )
        )

        self.lock_upgrade_exact = QtWidgets.QCheckBox("Lock Upgrade Exact")
        self.two_handing_check = QtWidgets.QCheckBox("Two Handing")
        self.lock_stats_checkbox = QtWidgets.QCheckBox("Use Locked Result Stats")

        self.objective_combo = QtWidgets.QComboBox()
        self.objective_combo.addItem("Max AR", "max_ar")
        self.objective_combo.addItem("Max AR + Bleed", "max_ar_plus_bleed")
        self.objective_combo.addItem("AoW First Hit (PvE)", "aow_first_hit")
        self.objective_combo.addItem("AoW Full Sequence (PvE)", "aow_full_sequence")
        layout.addWidget(self._field_stack("Objective", self.objective_combo))

        toggle_grid = QtWidgets.QGridLayout()
        toggle_grid.setHorizontalSpacing(8)
        toggle_grid.setVerticalSpacing(6)
        toggle_grid.addWidget(self.lock_upgrade_exact, 0, 0)
        toggle_grid.addWidget(self.two_handing_check, 0, 1)
        toggle_grid.addWidget(self.lock_stats_checkbox, 1, 0, 1, 2)
        layout.addLayout(toggle_grid)

        self.requirement_badge = self._chip_label("No weapon selected", "muted", "requirementBadge")
        self.free_points_label = QtWidgets.QLabel("Redistributable Combat Points: -")
        self.estimate_label = QtWidgets.QLabel("Search Space: -")
        self.requirement_label = QtWidgets.QLabel("Requirements: -")
        for label in (self.free_points_label, self.estimate_label, self.requirement_label):
            label.setProperty("role", "statusLine")

        self.search_button = QtWidgets.QPushButton("Search the Arsenal")
        self.search_button.setProperty("role", "ctaButton")
        self.progress_label = QtWidgets.QLabel("Idle")
        self.progress_label.setProperty("role", "progressLabel")
        self.progress_bar = QtWidgets.QProgressBar()
        self.progress_bar.setTextVisible(True)

        layout.addWidget(self.requirement_badge)
        layout.addWidget(self.free_points_label)
        layout.addWidget(self.estimate_label)
        layout.addWidget(self.requirement_label)
        layout.addWidget(self.search_button)
        layout.addWidget(self.progress_label)
        layout.addWidget(self.progress_bar)
        return group

    def _build_hero_header(self) -> QtWidgets.QFrame:
        self.hero_panel = QtWidgets.QFrame()
        self.hero_panel.setObjectName("HeroPanel")
        layout = QtWidgets.QHBoxLayout(self.hero_panel)
        layout.setContentsMargins(18, 18, 18, 18)
        layout.setSpacing(16)

        left_column = QtWidgets.QVBoxLayout()
        left_column.setSpacing(8)
        self.hero_objective_label = QtWidgets.QLabel("MAX AR SEARCH")
        self.hero_objective_label.setProperty("role", "heroTitle")
        self.hero_weapon_label = QtWidgets.QLabel("Open search across all weapons.")
        self.hero_weapon_label.setProperty("role", "heroSubtitle")
        left_column.addWidget(self.hero_objective_label)
        left_column.addWidget(self.hero_weapon_label)

        chip_row = QtWidgets.QHBoxLayout()
        chip_row.setSpacing(6)
        self.hero_search_chip = self._chip_label("Idle", "muted")
        self.hero_lock_chip = self._chip_label("Open Search", "info")
        self.hero_somber_chip = self._chip_label("All Paths", "muted")
        self.hero_handing_chip = self._chip_label("One Hand", "muted")
        self.hero_upgrade_chip = self._chip_label("Upgrade Range", "muted")
        self.hero_stats_chip = self._chip_label("Stats Open", "muted")
        for chip in (
            self.hero_search_chip,
            self.hero_lock_chip,
            self.hero_somber_chip,
            self.hero_handing_chip,
            self.hero_upgrade_chip,
            self.hero_stats_chip,
        ):
            chip_row.addWidget(chip)
        chip_row.addStretch(1)
        left_column.addLayout(chip_row)
        layout.addLayout(left_column, 1)

        metrics = QtWidgets.QHBoxLayout()
        metrics.setSpacing(10)
        level_card, self.hero_level_value = self._metric_card("Derived Level")
        budget_card, self.hero_budget_value = self._metric_card("Stat Budget")
        free_card, self.hero_free_value = self._metric_card("Redistributable")
        for card in (level_card, budget_card, free_card):
            metrics.addWidget(card)
        layout.addLayout(metrics, 0)
        return self.hero_panel

    def _metric_card(self, title: str) -> tuple[QtWidgets.QFrame, QtWidgets.QLabel]:
        frame = QtWidgets.QFrame()
        frame.setProperty("role", "metricCard")
        layout = QtWidgets.QVBoxLayout(frame)
        layout.setContentsMargins(12, 10, 12, 10)
        layout.setSpacing(2)
        title_label = QtWidgets.QLabel(title.upper())
        title_label.setProperty("role", "metricTitle")
        value_label = QtWidgets.QLabel("--")
        value_label.setProperty("role", "metricValue")
        layout.addWidget(title_label)
        layout.addWidget(value_label)
        return frame, value_label

    def _field_stack(self, label_text: str, widget: QtWidgets.QWidget) -> QtWidgets.QWidget:
        box = QtWidgets.QWidget()
        layout = QtWidgets.QVBoxLayout(box)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(4)
        label = QtWidgets.QLabel(label_text.upper())
        label.setProperty("role", "fieldLabel")
        layout.addWidget(label)
        layout.addWidget(widget)
        return box

    def _helper_label(self, text: str) -> QtWidgets.QLabel:
        label = QtWidgets.QLabel(text)
        label.setWordWrap(True)
        label.setProperty("role", "sectionHint")
        return label

    def _chip_label(
        self,
        text: str,
        tone: str = "muted",
        role: str = "chip",
    ) -> QtWidgets.QLabel:
        label = QtWidgets.QLabel(text)
        label.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        label.setProperty("role", role)
        label.setProperty("tone", tone)
        return label

    def _add_stat_row(
        self,
        grid: QtWidgets.QGridLayout,
        row: int,
        name: str,
        current_spin: QtWidgets.QSpinBox,
        min_spin: QtWidgets.QSpinBox | None,
    ) -> None:
        name_label = QtWidgets.QLabel(name)
        name_label.setProperty("role", "statName")
        grid.addWidget(name_label, row, 0)
        grid.addWidget(current_spin, row, 1)
        if min_spin is None:
            dash = QtWidgets.QLabel("--")
            dash.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
            dash.setProperty("role", "statDash")
            grid.addWidget(dash, row, 2)
        else:
            grid.addWidget(min_spin, row, 2)

    def _build_results_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("RESULTS")
        group.setObjectName("ResultsGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(10)
        layout.addWidget(self._helper_label("The first three contenders surface as cards. The full ranking stays below for exact inspection."))

        self.result_cards_container = QtWidgets.QWidget()
        cards_layout = QtWidgets.QHBoxLayout(self.result_cards_container)
        cards_layout.setContentsMargins(0, 0, 0, 0)
        cards_layout.setSpacing(10)
        for card_idx in range(3):
            card = self._build_result_card(card_idx)
            self.result_cards.append(card)
            cards_layout.addWidget(card["frame"], 1)
        layout.addWidget(self.result_cards_container)

        self.results_table = QtWidgets.QTableWidget(0, 17)
        self.results_table.setHorizontalHeaderLabels(
            [
                "#",
                "Weapon",
                "Affinity",
                "AoW",
                "Upgrade",
                "Scaling",
                "STR",
                "DEX",
                "INT",
                "FAI",
                "ARC",
                "AR",
                "Bleed",
                "AoW 1st",
                "AoW Full",
                "Score",
                "Lock",
            ]
        )
        self.results_table.horizontalHeader().setSectionResizeMode(
            QtWidgets.QHeaderView.ResizeMode.ResizeToContents
        )
        self.results_table.horizontalHeader().setStretchLastSection(True)
        self.results_table.verticalHeader().setVisible(False)
        self.results_table.setAlternatingRowColors(True)
        self.results_table.setShowGrid(False)
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
        group.setObjectName("UpgradeGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(10)
        layout.addWidget(self._helper_label("Selected build on one side, rival line on the other, upgrade spread underneath."))

        self.compare_summary_container = QtWidgets.QWidget()
        compare_summary_layout = QtWidgets.QHBoxLayout(self.compare_summary_container)
        compare_summary_layout.setContentsMargins(0, 0, 0, 0)
        compare_summary_layout.setSpacing(10)
        self.selected_compare_panel = self._build_compare_panel("Selected Build")
        self.compare_compare_panel = self._build_compare_panel("Comparison Target")
        compare_summary_layout.addWidget(self.selected_compare_panel["frame"], 1)
        compare_summary_layout.addWidget(self.compare_compare_panel["frame"], 1)
        layout.addWidget(self.compare_summary_container)

        toolbar_top = QtWidgets.QHBoxLayout()
        toolbar_top.setSpacing(10)
        self.compare_weapon_type_combo = QtWidgets.QComboBox()
        self.compare_aow_combo = QtWidgets.QComboBox()
        toolbar_top.addWidget(self._field_stack("Compare Type", self.compare_weapon_type_combo), 1)
        toolbar_top.addWidget(self._field_stack("Compare AoW", self.compare_aow_combo), 1)
        layout.addLayout(toolbar_top)

        toolbar_bottom = QtWidgets.QHBoxLayout()
        toolbar_bottom.setSpacing(10)
        self.compare_weapon_combo = QtWidgets.QComboBox()
        self.compare_affinity_combo = QtWidgets.QComboBox()
        toolbar_bottom.addWidget(self._field_stack("Compare Weapon", self.compare_weapon_combo), 2)
        toolbar_bottom.addWidget(self._field_stack("Compare Affinity", self.compare_affinity_combo), 1)
        layout.addLayout(toolbar_bottom)

        path_toolbar = QtWidgets.QHBoxLayout()
        path_toolbar.setSpacing(10)
        self.level_path_horizon_spin = self._u16_spin(1, 200, 40)
        self.level_path_button = QtWidgets.QPushButton("Path Graphs")
        self.level_path_button.setProperty("role", "inlineButton")
        self.level_path_button.setEnabled(False)
        path_toolbar.addWidget(self._field_stack("Current + N", self.level_path_horizon_spin), 0)
        path_toolbar.addWidget(self.level_path_button, 0)
        path_toolbar.addStretch(1)
        layout.addLayout(path_toolbar)

        self.upgrade_table = QtWidgets.QTableWidget(0, 0)
        self.upgrade_table.setEditTriggers(
            QtWidgets.QAbstractItemView.EditTrigger.NoEditTriggers
        )
        self.upgrade_table.horizontalHeader().setSectionResizeMode(
            QtWidgets.QHeaderView.ResizeMode.ResizeToContents
        )
        self.upgrade_table.horizontalHeader().setStretchLastSection(True)
        self.upgrade_table.verticalHeader().setVisible(False)
        self.upgrade_table.setAlternatingRowColors(True)
        self.upgrade_table.setShowGrid(False)
        layout.addWidget(self.upgrade_table)
        return group

    def _build_result_card(self, card_idx: int) -> dict[str, Any]:
        frame = QtWidgets.QFrame()
        frame.setProperty("role", "resultCard")
        frame.setProperty("cardState", "empty")
        frame.setMinimumHeight(168)
        layout = QtWidgets.QVBoxLayout(frame)
        layout.setContentsMargins(14, 14, 14, 14)
        layout.setSpacing(6)

        rank_chip = self._chip_label(f"#{card_idx + 1}", "muted")
        title = QtWidgets.QLabel("No result yet")
        title.setProperty("role", "cardTitle")
        detail = QtWidgets.QLabel("Run a search to surface ranked weapon lines.")
        detail.setProperty("role", "cardDetail")
        detail.setWordWrap(True)
        stats = QtWidgets.QLabel("STR --  DEX --  INT --  FAI --  ARC --")
        stats.setProperty("role", "cardStats")
        metrics = QtWidgets.QLabel("AR --   Bleed --   1st --   Full --   Score --")
        metrics.setProperty("role", "cardMetric")

        chip_row = QtWidgets.QHBoxLayout()
        chip_row.addWidget(rank_chip, 0)
        chip_row.addStretch(1)

        button_row = QtWidgets.QHBoxLayout()
        button_row.setSpacing(6)
        focus_button = QtWidgets.QPushButton("Focus")
        focus_button.setProperty("role", "inlineButton")
        focus_button.clicked.connect(lambda _checked=False, idx=card_idx: self._focus_result_row(idx))
        lock_button = QtWidgets.QPushButton("Lock")
        lock_button.setProperty("role", "inlineButton")
        lock_button.clicked.connect(lambda _checked=False, idx=card_idx: self._lock_from_result(idx))
        button_row.addWidget(focus_button)
        button_row.addWidget(lock_button)
        button_row.addStretch(1)

        layout.addLayout(chip_row)
        layout.addWidget(title)
        layout.addWidget(detail)
        layout.addWidget(stats)
        layout.addWidget(metrics)
        layout.addLayout(button_row)
        return {
            "frame": frame,
            "rank": rank_chip,
            "title": title,
            "detail": detail,
            "stats": stats,
            "metrics": metrics,
            "focus": focus_button,
            "lock": lock_button,
        }

    def _build_compare_panel(self, heading: str) -> dict[str, Any]:
        frame = QtWidgets.QFrame()
        frame.setProperty("role", "summaryPanel")
        frame.setMinimumHeight(148)
        layout = QtWidgets.QVBoxLayout(frame)
        layout.setContentsMargins(14, 14, 14, 14)
        layout.setSpacing(6)
        heading_label = QtWidgets.QLabel(heading.upper())
        heading_label.setProperty("role", "summaryHeading")
        title = QtWidgets.QLabel("Waiting on selection")
        title.setProperty("role", "summaryTitle")
        body = QtWidgets.QLabel("Search results and an active comparison will populate this lane.")
        body.setWordWrap(True)
        body.setProperty("role", "summaryBody")
        stats = QtWidgets.QLabel("STR --  DEX --  INT --  FAI --  ARC --")
        stats.setProperty("role", "summaryStats")
        metrics = QtWidgets.QLabel("Best +--   AR --   Bleed --   1st --   Full --")
        metrics.setProperty("role", "summaryMetric")
        layout.addWidget(heading_label)
        layout.addWidget(title)
        layout.addWidget(body)
        layout.addWidget(stats)
        layout.addWidget(metrics)
        return {
            "frame": frame,
            "heading": heading_label,
            "title": title,
            "body": body,
            "stats": stats,
            "metrics": metrics,
        }

    def _wire_events(self) -> None:
        self.search_button.clicked.connect(self._start_search)
        self.class_combo.currentIndexChanged.connect(self._on_class_changed)
        self.weapon_combo.currentIndexChanged.connect(self._refresh_affinity_options)
        self.affinity_combo.currentIndexChanged.connect(self._refresh_aow_options)
        self.compare_weapon_type_combo.currentIndexChanged.connect(self._refresh_compare_weapon_options)
        self.compare_weapon_combo.currentIndexChanged.connect(self._refresh_compare_affinity_options)
        self.compare_affinity_combo.currentIndexChanged.connect(self._refresh_compare_aow_options)
        self.results_table.itemSelectionChanged.connect(self._rebuild_upgrade_table)
        self.results_table.itemSelectionChanged.connect(self._refresh_result_cards)
        self.level_path_button.clicked.connect(self._open_level_path_dialog)
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
        self.all_aow_names = self.data.aow_names()
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
        self._refresh_aow_options()
        self._refresh_estimate()

    def _compatible_aow_names(self, weapon_name: str | None, affinity: str | None) -> list[str]:
        if weapon_name is None:
            if affinity is None:
                return self.all_aow_names
            return self.data.compatible_aow_names_for_affinity(affinity)
        return self.data.compatible_aow_names(weapon_name, affinity)

    def _refresh_aow_options(self) -> None:
        selected_weapon = self._combo_value(self.weapon_combo)
        selected_affinity = self._combo_value(self.affinity_combo)
        previous = self._combo_value(self.aow_combo)

        self.aow_combo.blockSignals(True)
        self.aow_combo.clear()
        self.aow_combo.addItem(OPEN_OPTION, None)
        for name in self._compatible_aow_names(selected_weapon, selected_affinity):
            self.aow_combo.addItem(name, name)

        if previous is not None:
            idx = self.aow_combo.findData(previous)
            self.aow_combo.setCurrentIndex(idx if idx >= 0 else 0)
        else:
            self.aow_combo.setCurrentIndex(0)
        self.aow_combo.blockSignals(False)

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
        self._refresh_compare_aow_options()
        self._rebuild_upgrade_table()

    def _refresh_compare_aow_options(self) -> None:
        selected_weapon = self._combo_value(self.compare_weapon_combo)
        selected_affinity = self._combo_value(self.compare_affinity_combo)
        previous = self._combo_value(self.compare_aow_combo)

        self.compare_aow_combo.blockSignals(True)
        self.compare_aow_combo.clear()
        self.compare_aow_combo.addItem(COMPARE_AOW_MATCH_SELECTED, "__match_selected__")
        self.compare_aow_combo.addItem(OPEN_OPTION, None)
        for name in self._compatible_aow_names(selected_weapon, selected_affinity):
            self.compare_aow_combo.addItem(name, name)

        if previous == "__match_selected__":
            self.compare_aow_combo.setCurrentIndex(0)
        elif previous is not None:
            idx = self.compare_aow_combo.findData(previous)
            self.compare_aow_combo.setCurrentIndex(idx if idx >= 0 else 1)
        else:
            self.compare_aow_combo.setCurrentIndex(1)
        self.compare_aow_combo.blockSignals(False)

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
        self.best_row_cache.clear()
        self.locked_ar_cache.clear()
        self.path_eval_cache.clear()
        self.path_target_cache.clear()
        self._sync_derived_level()
        request_signature = self._search_request_signature()
        if self.results_signature is not None and request_signature != self.results_signature:
            self._clear_results_state()
        if self.active_run_id is not None:
            self.discard_active_results = request_signature != self.active_request_signature
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
            self._refresh_hero_summary()
        except Exception as exc:
            self.estimate_label.setText(f"Search Space: invalid ({exc})")
            self.free_points_label.setText("Redistributable Combat Points: invalid")
            self._update_requirement_highlights()
            self._refresh_hero_summary()

    def _compute_free_points_text(self) -> str:
        snapshot = self._budget_snapshot()
        return (
            f"Redistributable Combat Points: {snapshot['redistributable']} "
            f"(Level {snapshot['level']}, Budget {snapshot['total']})"
        )

    def _budget_snapshot(self) -> dict[str, int]:
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
        return {
            "level": level,
            "total": total,
            "redistributable": total - floor_sum,
        }

    def _refresh_hero_summary(self) -> None:
        snapshot = self._budget_snapshot()
        objective_text = self.objective_combo.currentText().upper()
        self.hero_objective_label.setText(f"{objective_text} SEARCH")
        self.hero_weapon_label.setText(self._selected_weapon_summary())
        self.hero_level_value.setText(str(snapshot["level"]))
        self.hero_budget_value.setText(str(snapshot["total"]))
        self.hero_free_value.setText(str(snapshot["redistributable"]))

        progress_text = self.progress_label.text().strip()
        if self.active_run_id is not None:
            search_text, search_tone = "Searching", "accent"
        elif progress_text.startswith("Failed") or progress_text.startswith("Invalid") or progress_text.startswith("No valid"):
            search_text, search_tone = "Invalid", "danger"
        elif self.current_results:
            search_text, search_tone = f"{len(self.current_results)} Ready", "success"
        else:
            search_text, search_tone = "Idle", "muted"

        self._set_toned_label(self.hero_search_chip, search_text, search_tone)
        self._set_toned_label(
            self.hero_lock_chip,
            "Locked Search" if self._has_locked_filters() else "Open Search",
            "info" if self._has_locked_filters() else "muted",
        )
        self._set_toned_label(self.hero_somber_chip, self._somber_chip_text(), "muted")
        self._set_toned_label(
            self.hero_handing_chip,
            "Two Handing" if self.two_handing_check.isChecked() else "One Hand",
            "accent" if self.two_handing_check.isChecked() else "muted",
        )
        self._set_toned_label(
            self.hero_upgrade_chip,
            "Exact Upgrade" if self.lock_upgrade_exact.isChecked() else "Upgrade Range",
            "info" if self.lock_upgrade_exact.isChecked() else "muted",
        )
        exact_stats = self.lock_stats_checkbox.isChecked() and self.locked_result_stats is not None
        self._set_toned_label(
            self.hero_stats_chip,
            "Exact Stats" if exact_stats else "Stats Open",
            "info" if exact_stats else "muted",
        )

    def _selected_weapon_summary(self) -> str:
        weapon = self._combo_value(self.weapon_combo)
        affinity = self._combo_value(self.affinity_combo)
        aow_name = self._combo_value(self.aow_combo)
        parts = []
        if weapon is not None:
            parts.append(weapon)
        if affinity is not None:
            parts.append(affinity)
        if aow_name is not None:
            parts.append(f"AoW {aow_name}")
        if parts:
            return "Locked lane: " + " | ".join(parts)
        return "Open search across all weapons."

    def _somber_chip_text(self) -> str:
        value = self.somber_combo.currentData()
        if value == "standard_only":
            return "Standard Only"
        if value == "somber_only":
            return "Somber Only"
        return "All Paths"

    def _has_locked_filters(self) -> bool:
        return any(
            value is not None
            for value in (
                self._combo_value(self.weapon_combo),
                self._combo_value(self.affinity_combo),
                self._combo_value(self.aow_combo),
                self._combo_value(self.weapon_type_combo),
            )
        )

    def _set_toned_label(self, label: QtWidgets.QLabel, text: str, tone: str) -> None:
        label.setText(text)
        label.setProperty("tone", tone)
        self._restyle_widget(label)

    def _search_request_signature(self) -> tuple[Any, ...]:
        lock_stats = self.lock_stats_checkbox.isChecked() and self.locked_result_stats is not None
        lock_values = self.locked_result_stats if lock_stats and self.locked_result_stats is not None else {}
        return (
            self._resolved_class_name(),
            self._derived_level(),
            self.vig_spin.value(),
            self.mnd_spin.value(),
            self.end_spin.value(),
            self.max_upgrade_spin.value(),
            self.lock_upgrade_exact.isChecked(),
            self.two_handing_check.isChecked(),
            self._combo_value(self.weapon_combo),
            self._combo_value(self.affinity_combo),
            self._combo_value(self.aow_combo),
            self.objective_combo.currentData(),
            self.top_k_spin.value(),
            self._combo_value(self.weapon_type_combo),
            self.somber_combo.currentData(),
            self.min_str_spin.value(),
            self.min_dex_spin.value(),
            self.min_int_spin.value(),
            self.min_fai_spin.value(),
            self.min_arc_spin.value(),
            lock_values.get("str"),
            lock_values.get("dex"),
            lock_values.get("int"),
            lock_values.get("fai"),
            lock_values.get("arc"),
        )

    def _clear_results_state(self) -> None:
        self.current_results = []
        self.results_signature = None
        self.active_compare_selected = None
        self.active_compare_target = None
        self.results_table.clearContents()
        self.results_table.setRowCount(0)
        self.upgrade_table.clearContents()
        self.upgrade_table.setRowCount(0)
        self.upgrade_table.setColumnCount(0)
        self.level_path_button.setEnabled(False)
        self._refresh_result_cards()
        self._refresh_compare_summary(None, None, None)

    @staticmethod
    def _restyle_widget(widget: QtWidgets.QWidget) -> None:
        widget.style().unpolish(widget)
        widget.style().polish(widget)

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
        self.active_request_signature = self._search_request_signature()
        self.discard_active_results = False
        self._clear_results_state()
        self.search_button.setEnabled(False)
        self.search_button.setText("Searching...")
        self._set_search_progress_bar(0, total)
        self.progress_label.setText(f"Searching 0 / {total:,}...")
        self._refresh_hero_summary()

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

    @QtCore.pyqtSlot(int, object, object, object, float, object)
    def _on_progress(
        self,
        run_id: int,
        checked: object,
        total: object,
        eligible: object,
        best_score: float,
        elapsed_ms: object,
    ) -> None:
        if run_id != self.active_run_id:
            return
        checked = int(checked)
        total = int(total)
        eligible = int(eligible)
        elapsed_ms = int(elapsed_ms)
        if total > 0:
            self._set_search_progress_bar(checked, total)
        else:
            self.progress_bar.setRange(0, 0)
        self.progress_label.setText(
            f"Searching {checked:,} / {total:,} | Eligible {eligible:,} | "
            f"Best {best_score:.2f} | {elapsed_ms / 1000.0:.1f}s"
        )
        self._refresh_hero_summary()

    @QtCore.pyqtSlot(int, object)
    def _on_finished(self, run_id: int, results: object) -> None:
        if run_id != self.active_run_id:
            return
        self.search_button.setEnabled(True)
        self.search_button.setText("Search the Arsenal")
        if self.discard_active_results or self.active_request_signature != self._search_request_signature():
            self.active_run_id = None
            self.active_request_signature = None
            self.discard_active_results = False
            self._clear_results_state()
            self._set_idle_progress("Inputs changed during search. Rerun search.")
            return
        self.active_run_id = None
        self.current_results = list(results)
        self.results_signature = self.active_request_signature
        self.active_request_signature = None
        self.discard_active_results = False
        self._populate_results_table()
        self._rebuild_upgrade_table()
        self._set_idle_progress(f"Done. {len(self.current_results)} result(s).")

    @QtCore.pyqtSlot(int, str)
    def _on_failed(self, run_id: int, message: str) -> None:
        if run_id != self.active_run_id:
            return
        self.search_button.setEnabled(True)
        self.search_button.setText("Search the Arsenal")
        self.active_run_id = None
        self.active_request_signature = None
        self.discard_active_results = False
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
                self._scaling_summary(result.weapon_name, result.affinity, int(result.upgrade)),
                str(result.str_stat),
                str(result.dex),
                str(result.int_stat),
                str(result.fai),
                str(result.arc),
                f"{result.ar_total:.2f}",
                f"{result.bleed_buildup:.2f}",
                f"{result.aow_first_hit_damage:.2f}",
                f"{result.aow_full_sequence_damage:.2f}",
                f"{result.score:.2f}",
            ]
            for col_idx, value in enumerate(values):
                self.results_table.setItem(row_idx, col_idx, QtWidgets.QTableWidgetItem(value))

            lock_button = QtWidgets.QPushButton("Use As Locks")
            lock_button.setProperty("role", "inlineButton")
            lock_button.clicked.connect(lambda _checked=False, idx=row_idx: self._lock_from_result(idx))
            self.results_table.setCellWidget(row_idx, 16, lock_button)

        if self.current_results:
            self.results_table.selectRow(0)
        self._refresh_result_cards()
        self._refresh_hero_summary()

    def _focus_result_row(self, row_idx: int) -> None:
        if row_idx >= len(self.current_results):
            return
        self.results_table.selectRow(row_idx)

    def _selected_result_index(self) -> int | None:
        selected = self.results_table.selectionModel().selectedRows()
        if not selected:
            return None
        idx = selected[0].row()
        if idx >= len(self.current_results):
            return None
        return idx

    def _refresh_result_cards(self) -> None:
        selected_idx = self._selected_result_index()
        for card_idx, card in enumerate(self.result_cards):
            if card_idx < len(self.current_results):
                result = self.current_results[card_idx]
                card["title"].setText(f"{result.weapon_name} | {result.affinity}")
                card["detail"].setText(
                    f"AoW {result.aow_name or '-'} | Upgrade +{result.upgrade} | {self._scaling_summary(result.weapon_name, result.affinity, int(result.upgrade))}"
                )
                card["stats"].setText(
                    f"STR {result.str_stat}  DEX {result.dex}  INT {result.int_stat}  "
                    f"FAI {result.fai}  ARC {result.arc}"
                )
                card["metrics"].setText(self._result_metrics_text(result))
                card["focus"].setEnabled(True)
                card["lock"].setEnabled(True)
                if selected_idx == card_idx:
                    state = "selected"
                elif card_idx == 0:
                    state = "best"
                else:
                    state = "filled"
            else:
                card["title"].setText("No result yet")
                card["detail"].setText("Run a search to surface ranked weapon lines.")
                card["stats"].setText("STR --  DEX --  INT --  FAI --  ARC --")
                card["metrics"].setText("AR --   Bleed --   1st --   Full --   Score --")
                card["focus"].setEnabled(False)
                card["lock"].setEnabled(False)
                state = "empty"
            card["frame"].setProperty("cardState", state)
            self._restyle_widget(card["frame"])

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
            self.active_compare_selected = None
            self.active_compare_target = None
            self.level_path_button.setEnabled(False)
            self._refresh_compare_summary(None, None, None)
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
        compare_summary_row: dict[str, Any] | None = None

        if compare_weapon is None:
            for row_idx in range(0, min(4, len(self.current_results))):
                if row_idx == selected_idx:
                    continue
                row = self.current_results[row_idx]
                row_best = self._best_row_config(row.weapon_name, row.affinity, row.aow_name)
                if row_best is None:
                    row_best = self._row_config_from_result(row)
                if compare_summary_row is None:
                    compare_summary_row = row_best
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
                compare_summary_row = compare_best
            else:
                compare_best = None

        self.upgrade_table.setRowCount(len(rows_to_render))
        for row_idx, (label, row_data) in enumerate(rows_to_render):
            self.upgrade_table.setItem(row_idx, 0, QtWidgets.QTableWidgetItem(label))
            ar_series = self._locked_metric_series_for_config(row_data, max_upgrade)
            for lv in range(0, max_upgrade + 1):
                col = lv + 1
                ar = ar_series.get(lv)
                text = "-" if ar is None else f"{ar:.2f}"
                self.upgrade_table.setItem(row_idx, col, QtWidgets.QTableWidgetItem(text))
        self.active_compare_selected = selected_best
        self.active_compare_target = compare_summary_row
        self.level_path_button.setEnabled(selected_best is not None and compare_summary_row is not None)
        self._refresh_compare_summary(selected_best, compare_summary_row, compare_weapon)

    def _locked_metric_series_for_config(
        self,
        row_data: Any,
        max_upgrade: int,
    ) -> dict[int, float]:
        if row_data is None:
            return {}

        weapon_name = row_data["weapon_name"]
        affinity = row_data["affinity"]
        aow_name = row_data["aow_name"]
        lock_str = row_data["str_stat"]
        lock_dex = row_data["dex"]
        lock_int = row_data["int_stat"]
        lock_fai = row_data["fai"]
        lock_arc = row_data["arc"]

        cache_key = (
            self._optimizer_context_key(),
            weapon_name.casefold(),
            affinity.casefold(),
            (aow_name or "").casefold(),
            int(lock_str),
            int(lock_dex),
            int(lock_int),
            int(lock_fai),
            int(lock_arc),
            int(max_upgrade),
        )
        cached = self.locked_ar_cache.get(cache_key)
        if cached is not None:
            return cached

        kwargs = self._build_request_kwargs(include_progress=False)
        kwargs.update(
            {
                "weapon_name": weapon_name,
                "affinity": affinity,
                "aow_name": aow_name,
                "objective": self.objective_combo.currentData(),
                "top_k": max_upgrade + 1,
                "fixed_upgrade": None,
                "max_upgrade": max_upgrade,
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
            return {}

        series: dict[int, float] = {}
        for row in rows:
            series[int(row.upgrade)] = self._result_series_value(row)
        self.locked_ar_cache[cache_key] = series
        return series

    def _result_series_value(self, row: Any) -> float:
        objective = self.objective_combo.currentData()
        if objective == "aow_first_hit":
            return float(row.aow_first_hit_damage)
        if objective == "aow_full_sequence":
            return float(row.aow_full_sequence_damage)
        return float(row.ar_total)

    def _best_row_config(
        self,
        weapon_name: str,
        affinity: str,
        aow_name: Any,
    ) -> dict[str, Any] | None:
        cache_key = (
            self._optimizer_context_key(),
            weapon_name.casefold(),
            affinity.casefold(),
            (aow_name or "").casefold(),
        )
        if cache_key in self.best_row_cache:
            return self.best_row_cache[cache_key]

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
            self.best_row_cache[cache_key] = None
            return None
        if not rows:
            self.best_row_cache[cache_key] = None
            return None
        best = self._row_config_from_result(rows[0])
        self.best_row_cache[cache_key] = best
        return best

    def _optimizer_context_key(self) -> tuple[Any, ...]:
        return (
            self._resolved_class_name(),
            self._derived_level(),
            self.vig_spin.value(),
            self.mnd_spin.value(),
            self.end_spin.value(),
            self.two_handing_check.isChecked(),
            self.objective_combo.currentData(),
            self.min_str_spin.value(),
            self.min_dex_spin.value(),
            self.min_int_spin.value(),
            self.min_fai_spin.value(),
            self.min_arc_spin.value(),
        )

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
            "score": float(result.score),
            "bleed_buildup": float(result.bleed_buildup),
            "bleed_buildup_add": float(result.bleed_buildup_add),
            "frost_buildup": float(result.frost_buildup),
            "poison_buildup": float(result.poison_buildup),
            "aow_first_hit_damage": float(result.aow_first_hit_damage),
            "aow_full_sequence_damage": float(result.aow_full_sequence_damage),
        }

    def _format_best_stats(self, row_data: dict[str, Any]) -> str:
        return (
            f"Best +{row_data['best_upgrade']} "
            f"STR {row_data['str_stat']} DEX {row_data['dex']} "
            f"INT {row_data['int_stat']} FAI {row_data['fai']} ARC {row_data['arc']} "
            f"AR {row_data['best_ar_total']:.2f} BLEED {row_data['bleed_buildup']:.2f} "
            f"1ST {row_data['aow_first_hit_damage']:.2f} FULL {row_data['aow_full_sequence_damage']:.2f}"
        )

    def _result_metrics_text(self, result: Any) -> str:
        return (
            f"AR {float(result.ar_total):.2f}   "
            f"Bleed {float(result.bleed_buildup):.2f}   "
            f"1st {float(result.aow_first_hit_damage):.2f}   "
            f"Full {float(result.aow_full_sequence_damage):.2f}   "
            f"Score {float(result.score):.2f}"
        )

    def _weapon_scaling_values(
        self,
        weapon_name: str,
        affinity: str,
        upgrade: int,
    ) -> tuple[float, float, float, float, float]:
        cache_key = (weapon_name.casefold(), affinity.casefold(), int(upgrade))
        cached = self.scaling_cache.get(cache_key)
        if cached is not None:
            return cached
        values = tuple(self.data.weapon_scaling_for_upgrade(weapon_name, affinity, int(upgrade)))
        self.scaling_cache[cache_key] = values
        return values

    @staticmethod
    def _scaling_letter(value: float) -> str:
        if value <= 0.0:
            return "-"
        if value >= 1.75:
            return "S"
        if value >= 1.4:
            return "A"
        if value >= 0.9:
            return "B"
        if value >= 0.6:
            return "C"
        if value >= 0.25:
            return "D"
        return "E"

    def _scaling_summary(self, weapon_name: str, affinity: str, upgrade: int) -> str:
        values = self._weapon_scaling_values(weapon_name, affinity, upgrade)
        labels = ("STR", "DEX", "INT", "FAI", "ARC")
        parts = []
        for label, value in zip(labels, values):
            parts.append(f"{label} {self._scaling_letter(value)} {value:.2f}")
        return " | ".join(parts)

    def _current_combat_state(self) -> CombatState:
        return CombatState(
            str_stat=self.str_spin.value(),
            dex=self.dex_spin.value(),
            int_stat=self.int_spin.value(),
            fai=self.fai_spin.value(),
            arc=self.arc_spin.value(),
        )

    def _remaining_path_levels(self) -> int:
        state = self._current_combat_state()
        return (
            (99 - state.str_stat)
            + (99 - state.dex)
            + (99 - state.int_stat)
            + (99 - state.fai)
            + (99 - state.arc)
        )

    def _path_preview_configs(self) -> list[PathWeaponConfig]:
        configs: list[PathWeaponConfig] = []
        if self.active_compare_selected is not None:
            configs.append(self._path_config_from_row("Selected", self.active_compare_selected))
        if self.active_compare_target is not None:
            configs.append(self._path_config_from_row("Compare", self.active_compare_target))
        return configs

    @staticmethod
    def _path_config_from_row(title: str, row_data: dict[str, Any]) -> PathWeaponConfig:
        return PathWeaponConfig(
            title=title,
            weapon_name=str(row_data["weapon_name"]),
            affinity=str(row_data["affinity"]),
            aow_name=row_data["aow_name"],
            upgrade=int(row_data["best_upgrade"]),
            start_state=MainWindow._combat_state_from_row(row_data),
        )

    def _open_level_path_dialog(self) -> None:
        configs = self._path_preview_configs()
        if len(configs) < 2:
            QtWidgets.QMessageBox.information(
                self,
                "Path Graphs",
                "Pick a selected result and a comparison weapon first.",
            )
            return

        requested_horizon = self.level_path_horizon_spin.value()
        levels_ahead = min(requested_horizon, self._remaining_path_levels())
        if levels_ahead <= 0:
            QtWidgets.QMessageBox.information(
                self,
                "Path Graphs",
                "Combat stats are already capped. There is no forward path to trace.",
            )
            return

        total_steps = len(configs) * (levels_ahead + 1)
        progress = QtWidgets.QProgressDialog("Tracing level path...", "Cancel", 0, total_steps, self)
        progress.setWindowTitle("Path Graphs")
        progress.setWindowModality(QtCore.Qt.WindowModality.WindowModal)
        progress.setMinimumDuration(0)
        progress.setValue(0)

        previews = self._build_level_path_previews(levels_ahead, progress)
        progress.close()
        if previews is None or not previews:
            return

        dialog = LevelPathDialog(self, previews, self._derived_level(), levels_ahead)
        dialog.exec()

    def _build_level_path_previews(
        self,
        levels_ahead: int,
        progress: QtWidgets.QProgressDialog | None = None,
    ) -> list[PathPreview] | None:
        configs = self._path_preview_configs()
        previews: list[PathPreview] = []
        progress_value = 0
        total_steps = len(configs) * (levels_ahead + 1)
        if progress is not None:
            progress.setMaximum(total_steps)

        for config in configs:
            preview, processed = self._build_level_path_for_config(config, levels_ahead, progress, progress_value)
            if preview is None:
                return None
            previews.append(preview)
            progress_value += processed
        if progress is not None:
            progress.setValue(total_steps)
        return previews

    def _build_level_path_for_config(
        self,
        config: PathWeaponConfig,
        levels_ahead: int,
        progress: QtWidgets.QProgressDialog | None,
        progress_offset: int,
    ) -> tuple[PathPreview | None, int]:
        steps: list[PathStep] = []
        current_state = config.start_state
        processed = 0

        start_step = self._evaluate_path_step(config, self._derived_level(), current_state, None)
        steps.append(start_step)
        processed += 1
        self._update_path_progress(progress, progress_offset + processed, config.title, start_step.level)

        target_row = self._level_path_target_row(config, levels_ahead)
        if target_row is None:
            return PathPreview(config=config, steps=tuple(steps)), processed
        target_state = self._combat_state_from_row(target_row)

        for delta in range(1, levels_ahead + 1):
            if progress is not None and progress.wasCanceled():
                return None, processed
            next_step = self._choose_next_path_step(
                config,
                self._derived_level() + delta,
                current_state,
                target_state,
            )
            if next_step is None:
                break
            steps.append(next_step)
            current_state = next_step.stats
            processed += 1
            self._update_path_progress(progress, progress_offset + processed, config.title, next_step.level)

        return PathPreview(config=config, steps=tuple(steps)), processed

    def _update_path_progress(
        self,
        progress: QtWidgets.QProgressDialog | None,
        value: int,
        title: str,
        level: int,
    ) -> None:
        if progress is None:
            return
        progress.setValue(value)
        progress.setLabelText(f"Tracing {title.lower()} path at level {level}...")
        QtWidgets.QApplication.processEvents()

    def _choose_next_path_step(
        self,
        config: PathWeaponConfig,
        target_level: int,
        current_state: CombatState,
        target_state: CombatState,
    ) -> PathStep | None:
        candidates: list[PathStep] = []
        for stat_key in ("str", "dex", "int", "fai", "arc"):
            if getattr(current_state, self._combat_state_attr(stat_key)) >= getattr(
                target_state, self._combat_state_attr(stat_key)
            ):
                continue
            next_state = current_state.add_point(stat_key)
            if next_state is None:
                continue
            candidates.append(
                self._evaluate_path_step(config, target_level, next_state, stat_key)
            )

        if not candidates:
            return None

        return max(
            candidates,
            key=lambda step: self._path_step_sort_key(step),
        )

    @staticmethod
    def _stat_priority(stat_key: str | None) -> int:
        order = ("str", "dex", "int", "fai", "arc", None)
        return order.index(stat_key)

    @staticmethod
    def _combat_state_attr(stat_key: str) -> str:
        return {
            "str": "str_stat",
            "dex": "dex",
            "int": "int_stat",
            "fai": "fai",
            "arc": "arc",
        }[stat_key]

    def _path_step_sort_key(self, step: PathStep) -> tuple[int, float, float, int, int]:
        return (
            1 if step.ar is not None and step.score is not None else 0,
            float(step.score or 0.0),
            float(step.ar or 0.0),
            -int(step.requirement_gap),
            -self._stat_priority(step.added_stat),
        )

    def _level_path_target_row(
        self,
        config: PathWeaponConfig,
        levels_ahead: int,
    ) -> dict[str, Any] | None:
        current_state = config.start_state
        floor_mins = self._path_floor_mins(current_state)
        target_level = self._derived_level() + levels_ahead
        cache_key = (
            self._resolved_class_name(),
            target_level,
            self.vig_spin.value(),
            self.mnd_spin.value(),
            self.end_spin.value(),
            self.two_handing_check.isChecked(),
            self.objective_combo.currentData(),
            config.weapon_name.casefold(),
            config.affinity.casefold(),
            (config.aow_name or "").casefold(),
            config.upgrade,
            floor_mins,
        )
        if cache_key in self.path_target_cache:
            return self.path_target_cache[cache_key]

        class_base = CLASS_BASE_STATS[self._resolved_class_name()]
        kwargs = {
            "class_name": self._resolved_class_name(),
            "character_level": target_level,
            "vig": self.vig_spin.value(),
            "mnd": self.mnd_spin.value(),
            "end": self.end_spin.value(),
            "str_stat": int(class_base["str"]),
            "dex": int(class_base["dex"]),
            "int_stat": int(class_base["int"]),
            "fai": int(class_base["fai"]),
            "arc": int(class_base["arc"]),
            "max_upgrade": config.upgrade,
            "fixed_upgrade": config.upgrade,
            "two_handing": self.two_handing_check.isChecked(),
            "weapon_name": config.weapon_name,
            "affinity": config.affinity,
            "aow_name": config.aow_name,
            "objective": self.objective_combo.currentData(),
            "top_k": 1,
            "weapon_type_key": None,
            "somber_filter": "all",
            "min_str": floor_mins[0],
            "min_dex": floor_mins[1],
            "min_int": floor_mins[2],
            "min_fai": floor_mins[3],
            "min_arc": floor_mins[4],
            "lock_str": None,
            "lock_dex": None,
            "lock_int": None,
            "lock_fai": None,
            "lock_arc": None,
        }
        try:
            rows = core.optimize_builds(data=self.data, **kwargs)
        except Exception:
            rows = []

        target_row = self._row_config_from_result(rows[0]) if rows else None
        self.path_target_cache[cache_key] = target_row
        return target_row

    def _path_floor_mins(self, current_state: CombatState) -> tuple[int, int, int, int, int]:
        return (
            max(current_state.str_stat, self.min_str_spin.value()),
            max(current_state.dex, self.min_dex_spin.value()),
            max(current_state.int_stat, self.min_int_spin.value()),
            max(current_state.fai, self.min_fai_spin.value()),
            max(current_state.arc, self.min_arc_spin.value()),
        )

    @staticmethod
    def _combat_state_from_row(row_data: dict[str, Any]) -> CombatState:
        return CombatState(
            str_stat=int(row_data["str_stat"]),
            dex=int(row_data["dex"]),
            int_stat=int(row_data["int_stat"]),
            fai=int(row_data["fai"]),
            arc=int(row_data["arc"]),
        )

    def _evaluate_path_step(
        self,
        config: PathWeaponConfig,
        level: int,
        state: CombatState,
        added_stat: str | None,
    ) -> PathStep:
        cache_key = (
            self._resolved_class_name(),
            level,
            self.vig_spin.value(),
            self.mnd_spin.value(),
            self.end_spin.value(),
            self.two_handing_check.isChecked(),
            self.objective_combo.currentData(),
            config.weapon_name.casefold(),
            config.affinity.casefold(),
            (config.aow_name or "").casefold(),
            config.upgrade,
            state.str_stat,
            state.dex,
            state.int_stat,
            state.fai,
            state.arc,
        )
        cached = self.path_eval_cache.get(cache_key)
        if cached is not None:
            return PathStep(
                level=cached.level,
                stats=cached.stats,
                ar=cached.ar,
                score=cached.score,
                added_stat=added_stat,
                requirement_gap=cached.requirement_gap,
            )

        class_base = CLASS_BASE_STATS[self._resolved_class_name()]
        kwargs = {
            "class_name": self._resolved_class_name(),
            "character_level": level,
            "vig": self.vig_spin.value(),
            "mnd": self.mnd_spin.value(),
            "end": self.end_spin.value(),
            "str_stat": int(class_base["str"]),
            "dex": int(class_base["dex"]),
            "int_stat": int(class_base["int"]),
            "fai": int(class_base["fai"]),
            "arc": int(class_base["arc"]),
            "max_upgrade": config.upgrade,
            "fixed_upgrade": config.upgrade,
            "two_handing": self.two_handing_check.isChecked(),
            "weapon_name": config.weapon_name,
            "affinity": config.affinity,
            "aow_name": config.aow_name,
            "objective": self.objective_combo.currentData(),
            "top_k": 1,
            "weapon_type_key": None,
            "somber_filter": "all",
            "min_str": 0,
            "min_dex": 0,
            "min_int": 0,
            "min_fai": 0,
            "min_arc": 0,
            "lock_str": state.str_stat,
            "lock_dex": state.dex,
            "lock_int": state.int_stat,
            "lock_fai": state.fai,
            "lock_arc": state.arc,
        }

        ar: float | None = None
        score: float | None = None
        try:
            rows = core.optimize_builds(data=self.data, **kwargs)
        except Exception:
            rows = []
        if rows:
            ar = self._result_series_value(rows[0])
            score = float(rows[0].score)

        step = PathStep(
            level=level,
            stats=state,
            ar=ar,
            score=score,
            added_stat=None,
            requirement_gap=self._requirement_gap_for_state(config, state) if ar is None else 0,
        )
        self.path_eval_cache[cache_key] = step
        return PathStep(
            level=step.level,
            stats=step.stats,
            ar=step.ar,
            score=step.score,
            added_stat=added_stat,
            requirement_gap=step.requirement_gap,
        )

    def _requirement_gap_for_state(
        self,
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
        if self.two_handing_check.isChecked():
            effective_str = min(99, int(state.str_stat * 1.5))
        return (
            max(req_str - effective_str, 0)
            + max(req_dex - state.dex, 0)
            + max(req_int - state.int_stat, 0)
            + max(req_fai - state.fai, 0)
            + max(req_arc - state.arc, 0)
        )

    def _refresh_compare_summary(
        self,
        selected_best: dict[str, Any] | None,
        compare_best: dict[str, Any] | None,
        compare_weapon: str | None,
    ) -> None:
        self._set_compare_panel(
            self.selected_compare_panel,
            "Selected Build",
            selected_best,
            "Pick a result row to inspect its optimized line.",
        )
        if compare_best is not None:
            fallback = "Comparison lane ready."
        elif compare_weapon is not None:
            fallback = "No valid build found for the requested comparison."
        else:
            fallback = "Choose a comparison weapon or use the top rows as rival lines."
        self._set_compare_panel(
            self.compare_compare_panel,
            "Comparison Target",
            compare_best,
            fallback,
        )

    def _set_compare_panel(
        self,
        panel: dict[str, Any],
        heading: str,
        row_data: dict[str, Any] | None,
        fallback: str,
    ) -> None:
        panel["heading"].setText(heading.upper())
        if row_data is None:
            panel["title"].setText("Waiting on a valid line")
            panel["body"].setText(fallback)
            panel["stats"].setText("STR --  DEX --  INT --  FAI --  ARC --")
            panel["metrics"].setText("Best +--   AR --   Bleed --   1st --   Full --")
            return
        panel["title"].setText(f"{row_data['weapon_name']} | {row_data['affinity']}")
        panel["body"].setText(f"AoW {row_data['aow_name'] or '-'}")
        panel["stats"].setText(
            f"STR {row_data['str_stat']}  DEX {row_data['dex']}  INT {row_data['int_stat']}  "
            f"FAI {row_data['fai']}  ARC {row_data['arc']}"
        )
        panel["metrics"].setText(
            f"Best +{row_data['best_upgrade']}   AR {row_data['best_ar_total']:.2f}   "
            f"Bleed {row_data['bleed_buildup']:.2f}   "
            f"1st {row_data['aow_first_hit_damage']:.2f}   Full {row_data['aow_full_sequence_damage']:.2f}"
        )

    def _update_requirement_highlights(self) -> None:
        selected_weapon = self._combo_value(self.weapon_combo)
        selected_affinity = self._combo_value(self.affinity_combo)
        if selected_weapon is None:
            self.requirement_label.setText("Requirements: -")
            self._set_toned_label(self.requirement_badge, "No weapon selected", "muted")
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
            self._set_toned_label(self.requirement_badge, "Requirements unavailable", "danger")
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
        str_failed = effective_str < req_str
        dex_failed = self.dex_spin.value() < req_dex
        int_failed = self.int_spin.value() < req_int
        fai_failed = self.fai_spin.value() < req_fai
        arc_failed = self.arc_spin.value() < req_arc
        any_failed = any((str_failed, dex_failed, int_failed, fai_failed, arc_failed))

        self._set_req_fail(self.str_spin, str_failed)
        self._set_req_fail(self.dex_spin, dex_failed)
        self._set_req_fail(self.int_spin, int_failed)
        self._set_req_fail(self.fai_spin, fai_failed)
        self._set_req_fail(self.arc_spin, arc_failed)
        self._set_toned_label(
            self.requirement_badge,
            "Requirements Unmet" if any_failed else "Requirements Clear",
            "danger" if any_failed else "success",
        )

    @staticmethod
    def _set_req_fail(widget: QtWidgets.QSpinBox, failed: bool) -> None:
        widget.setProperty("reqFail", failed)
        widget.style().unpolish(widget)
        widget.style().polish(widget)

    def _set_idle_progress(self, message: str = "Idle") -> None:
        self.progress_label.setText(message)
        self.progress_bar.setRange(0, 1)
        self.progress_bar.setValue(0)
        self.search_button.setText("Search the Arsenal")
        self._refresh_hero_summary()

    def _set_search_progress_bar(self, checked: int, total: int) -> None:
        safe_total = max(total, 1)
        if safe_total <= QT_PROGRESS_MAX:
            self.progress_bar.setRange(0, safe_total)
            self.progress_bar.setValue(min(max(checked, 0), safe_total))
            return

        scaled = int(min(max(checked, 0), safe_total) * QT_PROGRESS_MAX / safe_total)
        self.progress_bar.setRange(0, QT_PROGRESS_MAX)
        self.progress_bar.setValue(min(max(scaled, 0), QT_PROGRESS_MAX))

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
    c = THEME
    app.setStyle("Fusion")
    app.setFont(QFont(FONT_FAMILY, 9))
    app.setStyleSheet(
        f"""
        QWidget {{
            background: {c["bg"]};
            color: {c["text"]};
        }}
        QWidget#RootShell, QWidget#RightStage, QWidget#LeftRail {{
            background: {c["bg"]};
        }}
        QScrollArea {{
            background: transparent;
            border: none;
        }}
        QGroupBox {{
            border: 1px solid {c["border"]};
            border-radius: 10px;
            margin-top: 12px;
            padding-top: 12px;
            background: {c["panel"]};
        }}
        QGroupBox#BuildGroup, QGroupBox#ConstraintsGroup {{
            background: {c["panel_alt"]};
        }}
        QGroupBox#SearchGroup {{
            background: {c["panel_soft"]};
            border: 1px solid {c["border_bright"]};
        }}
        QGroupBox#ResultsGroup, QGroupBox#UpgradeGroup {{
            background: {c["panel"]};
        }}
        QGroupBox::title {{
            subcontrol-origin: margin;
            left: 12px;
            padding: 0 8px;
            color: {c["accent"]};
            font-weight: 700;
            letter-spacing: 2px;
        }}
        QGroupBox#SearchGroup::title {{
            color: {c["text_bright"]};
        }}
        QFrame#HeroPanel {{
            background: qlineargradient(x1:0, y1:0, x2:1, y2:1,
                stop:0 {c["hero"]}, stop:1 {c["panel_soft"]});
            border: 1px solid {c["border_bright"]};
            border-radius: 10px;
        }}
        QFrame[role="metricCard"] {{
            background: {c["panel_soft"]};
            border: 1px solid {c["border"]};
            border-radius: 8px;
        }}
        QFrame[role="summaryPanel"], QFrame[role="resultCard"] {{
            background: {c["panel_alt"]};
            border: 1px solid {c["border"]};
            border-radius: 8px;
        }}
        QFrame[role="resultCard"][cardState="best"] {{
            border: 1px solid {c["border_bright"]};
            background: qlineargradient(x1:0, y1:0, x2:1, y2:0,
                stop:0 {c["panel_alt"]}, stop:1 {c["accent_dark"]});
        }}
        QFrame[role="resultCard"][cardState="selected"] {{
            border: 1px solid {c["text_bright"]};
            background: {c["panel_soft"]};
        }}
        QFrame[role="resultCard"][cardState="empty"] {{
            background: {c["muted_bg"]};
        }}
        QLabel[role="heroTitle"] {{
            color: {c["text_bright"]};
            font-size: 24px;
            font-weight: 700;
            letter-spacing: 2px;
        }}
        QLabel[role="heroSubtitle"] {{
            color: {c["text"]};
            font-size: 13px;
        }}
        QLabel[role="metricTitle"], QLabel[role="fieldLabel"], QLabel[role="gridHeader"], QLabel[role="summaryHeading"] {{
            color: {c["text_soft"]};
            font-size: 10px;
            font-weight: 700;
            letter-spacing: 1px;
        }}
        QLabel[role="metricValue"] {{
            color: {c["text_bright"]};
            font-size: 22px;
            font-weight: 700;
        }}
        QLabel[role="sectionHint"], QLabel[role="cardDetail"], QLabel[role="summaryBody"], QLabel[role="statusLine"] {{
            color: {c["text_soft"]};
        }}
        QLabel[role="cardTitle"], QLabel[role="summaryTitle"] {{
            color: {c["text_bright"]};
            font-size: 15px;
            font-weight: 700;
        }}
        QLabel[role="cardStats"], QLabel[role="summaryStats"], QLabel[role="cardMetric"], QLabel[role="summaryMetric"] {{
            color: {c["text"]};
        }}
        QLabel[role="statName"] {{
            color: {c["text_bright"]};
            font-weight: 700;
        }}
        QLabel[role="statDash"] {{
            color: {c["text_soft"]};
        }}
        QLabel[role="chip"], QLabel[role="requirementBadge"] {{
            border: 1px solid {c["border"]};
            border-radius: 10px;
            padding: 4px 9px;
            font-size: 10px;
            font-weight: 700;
            letter-spacing: 1px;
            background: {c["muted_bg"]};
            color: {c["text"]};
        }}
        QLabel[tone="accent"] {{
            background: {c["panel_alt"]};
            border-color: {c["border_bright"]};
            color: {c["text_bright"]};
        }}
        QLabel[tone="success"] {{
            background: {c["success_bg"]};
            border-color: {c["success_border"]};
            color: {c["text_bright"]};
        }}
        QLabel[tone="danger"] {{
            background: {c["danger_bg"]};
            border-color: {c["danger_border"]};
            color: #ffd5d9;
        }}
        QLabel[tone="info"] {{
            background: {c["info_bg"]};
            border-color: {c["accent"]};
            color: {c["text_bright"]};
        }}
        QLineEdit, QSpinBox, QComboBox, QTableWidget {{
            background: {c["input"]};
            border: 1px solid {c["border"]};
            border-radius: 6px;
            padding: 5px 7px;
            color: {c["text"]};
            alternate-background-color: {c["row_alt"]};
        }}
        QLineEdit:focus, QSpinBox:focus, QComboBox:focus {{
            border: 1px solid {c["border_bright"]};
            background: {c["panel_soft"]};
            color: {c["text_bright"]};
        }}
        QComboBox::drop-down {{
            border: none;
            width: 22px;
        }}
        QComboBox::down-arrow {{
            width: 10px;
            height: 10px;
        }}
        QComboBox QAbstractItemView, QTableWidget {{
            background: {c["input"]};
            color: {c["text"]};
            selection-background-color: {c["panel_soft"]};
            selection-color: {c["text_bright"]};
        }}
        QHeaderView::section, QTableCornerButton::section {{
            background: {c["panel_soft"]};
            color: {c["accent"]};
            border: 1px solid {c["border"]};
            padding: 6px 8px;
            font-weight: 700;
            letter-spacing: 1px;
        }}
        QTabWidget::pane {{
            border: 1px solid {c["border"]};
            border-radius: 8px;
            background: {c["panel"]};
            top: -1px;
        }}
        QTabBar::tab {{
            background: {c["panel_alt"]};
            color: {c["text_soft"]};
            border: 1px solid {c["border"]};
            border-bottom: none;
            padding: 8px 14px;
            min-width: 150px;
            font-weight: 700;
            letter-spacing: 1px;
        }}
        QTabBar::tab:selected {{
            background: {c["panel_soft"]};
            color: {c["text_bright"]};
            border-color: {c["border_bright"]};
        }}
        QTabBar::tab:hover:!selected {{
            color: {c["text"]};
            border-color: {c["accent_deep"]};
        }}
        QProgressBar {{
            background: {c["input"]};
            border: 1px solid {c["border"]};
            border-radius: 4px;
            text-align: center;
            color: {c["accent"]};
            min-height: 18px;
        }}
        QProgressBar::chunk {{
            background: qlineargradient(x1:0, y1:0, x2:1, y2:0,
                stop:0 {c["accent_deep"]}, stop:0.5 {c["accent"]}, stop:1 {c["accent_deep"]});
            border-radius: 4px;
        }}
        QSpinBox[reqFail="true"] {{
            background: {c["danger_bg"]};
            border: 1px solid {c["danger_border"]};
            color: #ffd5d9;
        }}
        QPushButton {{
            background: {c["panel_alt"]};
            border: 1px solid {c["accent"]};
            border-bottom: 2px solid {c["accent_deep"]};
            border-radius: 3px;
            padding: 8px 14px;
            color: {c["text_bright"]};
            letter-spacing: 1px;
            font-weight: 700;
        }}
        QPushButton:hover {{
            background: {c["panel_soft"]};
            border-color: {c["text_bright"]};
        }}
        QPushButton[role="ctaButton"] {{
            padding: 10px 16px;
            font-size: 13px;
            background: {c["accent_dark"]};
        }}
        QPushButton[role="inlineButton"] {{
            padding: 5px 9px;
            font-size: 10px;
        }}
        QPushButton:disabled {{
            background: {c["panel_alt"]};
            color: {c["text_soft"]};
            border-color: {c["border"]};
        }}
        QCheckBox {{
            spacing: 7px;
            color: {c["text"]};
        }}
        QCheckBox::indicator {{
            width: 14px;
            height: 14px;
            border-radius: 3px;
            border: 1px solid {c["border"]};
            background: {c["input"]};
        }}
        QCheckBox::indicator:checked {{
            border-color: {c["border_bright"]};
            background: {c["accent_dark"]};
        }}
        QLabel[role="progressLabel"] {{
            color: {c["text_bright"]};
            font-weight: 700;
        }}
        QTableWidget {{
            gridline-color: {c["border"]};
        }}
        QTableWidget::item:selected {{
            background: {c["panel_soft"]};
            color: {c["text_bright"]};
        }}
        QSplitter::handle {{
            background: {c["panel_alt"]};
            width: 4px;
            height: 4px;
            margin: 10px 0;
        }}
        QScrollBar:vertical {{
            background: {c["input"]};
            width: 10px;
            border: 1px solid {c["border"]};
            margin: 0;
        }}
        QScrollBar::handle:vertical {{
            background: {c["accent_deep"]};
            min-height: 22px;
            border: 1px solid {c["border_bright"]};
            border-radius: 2px;
        }}
        QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{
            height: 0;
        }}
        QScrollBar:horizontal {{
            background: {c["input"]};
            height: 10px;
            border: 1px solid {c["border"]};
            margin: 0;
        }}
        QScrollBar::handle:horizontal {{
            background: {c["accent_deep"]};
            min-width: 22px;
            border: 1px solid {c["border_bright"]};
            border-radius: 2px;
        }}
        QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {{
            width: 0;
        }}
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
