from __future__ import annotations

import csv
import sys
from pathlib import Path
from typing import Any

from PyQt6 import QtCore, QtWidgets
from PyQt6.QtGui import QColor, QPainter, QPainterPath, QPen

THIS_DIR = Path(__file__).resolve().parent
if str(THIS_DIR) not in sys.path:
    sys.path.insert(0, str(THIS_DIR))

import models as desktop_models  # noqa: E402
import services as desktop_services  # noqa: E402
import theme as desktop_theme  # noqa: E402
import widgets as desktop_widgets  # noqa: E402

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

THEME = desktop_theme.THEME


def app_root() -> Path:
    if getattr(sys, "frozen", False):
        if meipass := getattr(sys, "_MEIPASS", None):
            return Path(meipass)
        return Path(sys.executable).resolve().parent
    sibling_data = THIS_DIR / "data" / "phase1"
    if sibling_data.exists():
        return THIS_DIR
    return Path(__file__).resolve().parents[2]


def data_snapshot_dir() -> Path:
    root = app_root()
    bundled = root / "data" / "phase1"
    if bundled.exists():
        return bundled
    local = Path(sys.executable).resolve().parent / "data" / "phase1"
    if local.exists():
        return local
    return bundled


def load_weapon_type_options(snapshot_dir: Path) -> list[tuple[str, str]]:
    weapons_csv = snapshot_dir / "weapons.csv"
    if not weapons_csv.exists():
        return []

    display_to_key: dict[str, str] = {}
    with weapons_csv.open("r", encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle):
            display_name = normalize_weapon_type_display(
                row.get("weapon_type_name", "").strip()
            )
            raw_keys = [key.strip() for key in row.get("weapon_type_keys", "").split("|") if key.strip()]
            if not display_name or not raw_keys or display_name in display_to_key:
                continue
            preferred_key = next(
                (key for key in raw_keys if key.casefold() == display_name.casefold()),
                raw_keys[0],
            )
            display_to_key[display_name] = preferred_key
    return sorted(display_to_key.items(), key=lambda item: item[0].casefold())


def normalize_weapon_type_display(raw_name: str) -> str:
    normalized = raw_name.strip()
    if not normalized:
        return ""
    overrides = {
        "Hand-to-Hand": "Hand-to-Hand Arts",
        "Heavy Spear": "Great Spear",
        "Reverse-hand Blade": "Backhand Blade",
        "Scythe": "Reaper",
        "Seal": "Sacred Seal",
        "Staff": "Glintstone Staff",
    }
    return overrides.get(normalized, normalized)


CombatState = desktop_models.CombatState
PathWeaponConfig = desktop_models.PathWeaponConfig
PathPreview = desktop_models.PathPreview
AffinityWatchLine = desktop_models.AffinityWatchLine
AffinityBreakpoint = desktop_models.AffinityBreakpoint


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
                table.setItem(row_idx, col_idx, self._centered_table_item(value))
            if step.ar is not None:
                last_ar = step.ar

        layout.addWidget(table, 1)
        return shell

    def _centered_table_item(self, value: str) -> QtWidgets.QTableWidgetItem:
        item = QtWidgets.QTableWidgetItem(str(value))
        item.setTextAlignment(
            int(
                QtCore.Qt.AlignmentFlag.AlignCenter
                | QtCore.Qt.AlignmentFlag.AlignVCenter
            )
        )
        return item


class AffinityWatchChartWidget(QtWidgets.QWidget):
    def __init__(self, parent: QtWidgets.QWidget | None = None) -> None:
        super().__init__(parent)
        self.lines: list[AffinityWatchLine] = []
        self.metric_label = "Metric"
        self.series_colors = [
            QColor("#c9a44c"),
            QColor("#b8643c"),
            QColor("#6f96d8"),
            QColor("#7abf8f"),
            QColor("#d87aa0"),
            QColor("#d0c36a"),
        ]
        self.setMinimumHeight(300)

    def set_payload(self, lines: list[AffinityWatchLine], metric_label: str) -> None:
        self.lines = lines
        self.metric_label = metric_label
        self.update()

    def paintEvent(self, _event: Any) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.fillRect(self.rect(), QColor(THEME["panel_alt"]))
        painter.setPen(QPen(QColor(THEME["border"]), 1))
        painter.drawRoundedRect(self.rect().adjusted(0, 0, -1, -1), 8, 8)

        valid_points = [
            (point.level, point.metric)
            for line in self.lines
            for point in line.points
            if point.metric is not None
        ]
        if not valid_points:
            painter.setPen(QColor(THEME["text_soft"]))
            painter.drawText(
                self.rect().adjusted(18, 18, -18, -18),
                QtCore.Qt.AlignmentFlag.AlignCenter,
                "No valid affinity data for the selected setup.",
            )
            return

        levels = [level for level, _ in valid_points]
        metrics = [float(metric) for _, metric in valid_points if metric is not None]
        level_min = min(levels)
        level_max = max(levels)
        metric_min = min(metrics)
        metric_max = max(metrics)
        if level_min == level_max:
            level_max += 1
        if abs(metric_max - metric_min) < 0.01:
            metric_max += 1.0

        chart_rect = self.rect().adjusted(54, 34, -22, -44)
        painter.setPen(QPen(QColor(THEME["border"]), 1))
        for idx in range(5):
            ratio = idx / 4
            y = chart_rect.bottom() - ratio * chart_rect.height()
            painter.drawLine(int(chart_rect.left()), int(y), int(chart_rect.right()), int(y))

        painter.setPen(QColor(THEME["text_soft"]))
        painter.drawText(QtCore.QRectF(chart_rect.left(), chart_rect.bottom() + 8, 80, 20), f"Lv {level_min}")
        painter.drawText(
            QtCore.QRectF(chart_rect.right() - 80, chart_rect.bottom() + 8, 80, 20),
            QtCore.Qt.AlignmentFlag.AlignRight,
            f"Lv {level_max}",
        )
        painter.drawText(
            QtCore.QRectF(8, chart_rect.top() - 6, 42, 20),
            QtCore.Qt.AlignmentFlag.AlignLeft,
            f"{metric_max:.0f}",
        )
        painter.drawText(
            QtCore.QRectF(8, chart_rect.bottom() - 10, 42, 20),
            QtCore.Qt.AlignmentFlag.AlignLeft,
            f"{metric_min:.0f}",
        )
        painter.drawText(
            QtCore.QRectF(chart_rect.left(), 8, chart_rect.width(), 18),
            QtCore.Qt.AlignmentFlag.AlignCenter,
            self.metric_label,
        )

        legend_x = chart_rect.left()
        legend_y = 12
        for idx, line in enumerate(self.lines):
            color = self.series_colors[idx % len(self.series_colors)]
            painter.setPen(QPen(color, 2))
            painter.drawLine(legend_x, legend_y, legend_x + 18, legend_y)
            painter.setPen(QColor(THEME["text"]))
            final_metric = "--" if line.end_metric is None else f"{line.end_metric:.2f}"
            painter.drawText(
                QtCore.QRectF(legend_x + 24, legend_y - 8, 220, 18),
                f"{line.affinity} ({final_metric})",
            )
            legend_x += 232
            if legend_x + 220 > chart_rect.right():
                legend_x = chart_rect.left()
                legend_y += 18

        for idx, line in enumerate(self.lines):
            color = self.series_colors[idx % len(self.series_colors)]
            painter.setPen(QPen(color, 2.5))
            path = QPainterPath()
            started = False
            plotted: list[QtCore.QPointF] = []
            for point in line.points:
                if point.metric is None:
                    started = False
                    continue
                x_ratio = (point.level - level_min) / (level_max - level_min)
                y_ratio = (point.metric - metric_min) / (metric_max - metric_min)
                chart_point = QtCore.QPointF(
                    chart_rect.left() + x_ratio * chart_rect.width(),
                    chart_rect.bottom() - y_ratio * chart_rect.height(),
                )
                plotted.append(chart_point)
                if not started:
                    path.moveTo(chart_point)
                    started = True
                else:
                    path.lineTo(chart_point)
            painter.drawPath(path)
            painter.setBrush(color)
            for chart_point in plotted if len(plotted) <= 80 else plotted[:: max(1, len(plotted) // 28)]:
                painter.drawEllipse(chart_point, 3.0, 3.0)


class AffinityWatchDialog(QtWidgets.QDialog):
    def __init__(
        self,
        parent: QtWidgets.QWidget,
        weapon_name: str,
        aow_name: str | None,
        upgrade: int,
        start_level: int,
        levels_ahead: int,
        metric_label: str,
        lines: list[AffinityWatchLine],
        breakpoints: list[AffinityBreakpoint],
    ) -> None:
        super().__init__(parent)
        self.setWindowTitle("Affinity Watcher")
        self.resize(1240, 820)

        layout = QtWidgets.QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(12)

        heading = QtWidgets.QLabel(
            f"{weapon_name} | AoW {aow_name or '-'} | +{upgrade} | Current +{levels_ahead}"
        )
        heading.setProperty("role", "cardTitle")
        layout.addWidget(heading)

        subtitle = QtWidgets.QLabel(
            "Each line keeps one affinity locked across the whole horizon while combat stats are re-optimized at every level."
        )
        subtitle.setProperty("role", "sectionHint")
        subtitle.setWordWrap(True)
        layout.addWidget(subtitle)
        if len(lines) == 1:
            single_line = QtWidgets.QLabel(
                f"Only one legal affinity is available for this setup: {lines[0].affinity}."
            )
            single_line.setProperty("role", "summaryBody")
            layout.addWidget(single_line)

        chart = AffinityWatchChartWidget()
        chart.set_payload(lines, metric_label)
        layout.addWidget(chart)

        summary_table = QtWidgets.QTableWidget(len(lines), 4)
        summary_table.setHorizontalHeaderLabels(["Affinity", f"Lv {start_level}", f"Lv {start_level + levels_ahead}", "Final Stats"])
        summary_table.horizontalHeader().setSectionResizeMode(QtWidgets.QHeaderView.ResizeMode.ResizeToContents)
        summary_table.horizontalHeader().setStretchLastSection(True)
        summary_table.verticalHeader().setVisible(False)
        summary_table.setEditTriggers(QtWidgets.QAbstractItemView.EditTrigger.NoEditTriggers)
        summary_table.setSelectionMode(QtWidgets.QAbstractItemView.SelectionMode.NoSelection)
        summary_table.setAlternatingRowColors(True)
        summary_table.setShowGrid(False)
        for row_idx, line in enumerate(lines):
            final_stats = "--"
            if getattr(line, "final_build", None) is not None:
                final_state = MainWindow._combat_state_from_row(line.final_build)
                final_stats = final_state.summary()
            values = [
                line.affinity,
                "--" if line.start_metric is None else f"{line.start_metric:.2f}",
                "--" if line.end_metric is None else f"{line.end_metric:.2f}",
                final_stats,
            ]
            for col_idx, value in enumerate(values):
                summary_table.setItem(row_idx, col_idx, MainWindow._centered_table_item(self, value))
        layout.addWidget(summary_table, 1)

        breakpoint_box = QtWidgets.QGroupBox("CROSSOVERS")
        breakpoint_layout = QtWidgets.QVBoxLayout(breakpoint_box)
        if breakpoints:
            breakpoint_table = QtWidgets.QTableWidget(len(breakpoints), 5)
            breakpoint_table.setHorizontalHeaderLabels(["Level", "From", "To", "Old Metric", "New Metric"])
            breakpoint_table.horizontalHeader().setSectionResizeMode(QtWidgets.QHeaderView.ResizeMode.ResizeToContents)
            breakpoint_table.horizontalHeader().setStretchLastSection(True)
            breakpoint_table.verticalHeader().setVisible(False)
            breakpoint_table.setEditTriggers(QtWidgets.QAbstractItemView.EditTrigger.NoEditTriggers)
            breakpoint_table.setSelectionMode(QtWidgets.QAbstractItemView.SelectionMode.NoSelection)
            breakpoint_table.setAlternatingRowColors(True)
            breakpoint_table.setShowGrid(False)
            for row_idx, breakpoint in enumerate(breakpoints):
                values = [
                    str(breakpoint.level),
                    breakpoint.outgoing_affinity,
                    breakpoint.incoming_affinity,
                    "--" if breakpoint.outgoing_metric is None else f"{breakpoint.outgoing_metric:.2f}",
                    "--" if breakpoint.incoming_metric is None else f"{breakpoint.incoming_metric:.2f}",
                ]
                for col_idx, value in enumerate(values):
                    breakpoint_table.setItem(
                        row_idx,
                        col_idx,
                        MainWindow._centered_table_item(self, value),
                    )
            breakpoint_layout.addWidget(breakpoint_table)
        else:
            label = QtWidgets.QLabel("No leadership changes within the selected horizon.")
            label.setProperty("role", "summaryBody")
            breakpoint_layout.addWidget(label)
        layout.addWidget(breakpoint_box, 1)


class OptimizeWorker(QtCore.QObject):
    progress = QtCore.pyqtSignal(int, object, object, object, float, object)
    finished = QtCore.pyqtSignal(int, object)
    failed = QtCore.pyqtSignal(int, str)

    def __init__(
        self,
        run_id: int,
        service: desktop_services.DesktopOptimizerService,
        session: desktop_models.GlobalSession,
    ) -> None:
        super().__init__()
        self.run_id = run_id
        self.service = service
        self.session = session
        self.cancel_requested = False

    @QtCore.pyqtSlot()
    def run(self) -> None:
        try:
            results = self.service.run_search(self.session, progress_cb=self._progress_cb)
            self.finished.emit(self.run_id, results)
        except Exception as exc:
            self.failed.emit(self.run_id, str(exc))

    @QtCore.pyqtSlot()
    def cancel(self) -> None:
        self.cancel_requested = True

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
        return not self.cancel_requested


class AffinityWatchWorker(QtCore.QObject):
    progress = QtCore.pyqtSignal(object, object, object, object)
    finished = QtCore.pyqtSignal(object, object)
    failed = QtCore.pyqtSignal(str)

    def __init__(
        self,
        service: desktop_services.DesktopOptimizerService,
        session: desktop_models.GlobalSession,
        solved: desktop_models.SolvedBuild,
        levels_ahead: int,
    ) -> None:
        super().__init__()
        self.service = service
        self.session = session
        self.solved = solved
        self.levels_ahead = levels_ahead
        self.cancel_requested = False

    @QtCore.pyqtSlot()
    def run(self) -> None:
        try:
            payload = self.service.build_affinity_watch(
                self.session,
                self.solved,
                self.levels_ahead,
                progress_cb=self._progress_cb,
            )
            self.finished.emit(list(payload.lines), list(payload.breakpoints))
        except Exception as exc:
            self.failed.emit(str(exc))

    @QtCore.pyqtSlot()
    def cancel(self) -> None:
        self.cancel_requested = True

    def _progress_cb(self, processed: int, total: int, affinity: str, level: int) -> bool:
        self.progress.emit(processed, total, affinity, level)
        return not self.cancel_requested


class PathPreviewWorker(QtCore.QObject):
    progress = QtCore.pyqtSignal(object, object, object, object)
    finished = QtCore.pyqtSignal(object, object)
    failed = QtCore.pyqtSignal(str)

    def __init__(
        self,
        service: desktop_services.DesktopOptimizerService,
        session: desktop_models.GlobalSession,
        configs: list[desktop_models.PathWeaponConfig],
        levels_ahead: int,
    ) -> None:
        super().__init__()
        self.service = service
        self.session = session
        self.configs = configs
        self.levels_ahead = levels_ahead
        self.cancel_requested = False

    @QtCore.pyqtSlot()
    def run(self) -> None:
        try:
            previews: list[desktop_models.PathPreview] = []
            total = max(len(self.configs), 1)
            for index, config in enumerate(self.configs, start=1):
                if self.cancel_requested:
                    raise RuntimeError("cancelled")
                preview = self.service.build_path_preview(
                    self.session,
                    config.solved,
                    self.levels_ahead,
                    config.title,
                )
                if self.cancel_requested:
                    raise RuntimeError("cancelled")
                previews.append(preview)
                final_level = self.session.build.derived_level + self.levels_ahead
                self.progress.emit(index, total, config.title, final_level)
            self.finished.emit(previews, self.levels_ahead)
        except Exception as exc:
            self.failed.emit(str(exc))

    @QtCore.pyqtSlot()
    def cancel(self) -> None:
        self.cancel_requested = True


class MainWindow(QtWidgets.QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("Tarnished's Arsenal")
        self.setWindowFlag(QtCore.Qt.WindowType.MSWindowsFixedSizeDialogHint, False)
        self.setWindowFlag(QtCore.Qt.WindowType.WindowMinMaxButtonsHint, True)
        self.setMinimumSize(1200, 720)
        self.resize(1600, 980)

        data_path = data_snapshot_dir()
        self.data = core.load_game_data(str(data_path))
        self.weapon_type_options = load_weapon_type_options(data_path)
        self.desktop_service = desktop_services.DesktopOptimizerService(self.data)
        self.run_id = 0
        self.active_run_id: int | None = None
        self.search_cancel_requested = False
        self.worker_thread: QtCore.QThread | None = None
        self.worker: OptimizeWorker | None = None
        self.path_thread: QtCore.QThread | None = None
        self.path_worker: PathPreviewWorker | None = None
        self.path_cancel_requested = False
        self.affinity_watch_thread: QtCore.QThread | None = None
        self.affinity_watch_worker: AffinityWatchWorker | None = None
        self.affinity_watch_cancel_requested = False
        self.affinity_watch_context: dict[str, Any] | None = None
        self.path_preview_signature: tuple[Any, ...] | None = None
        self.affinity_watch_signature: tuple[Any, ...] | None = None
        self.current_results: list[Any] = []
        self.results_signature: tuple[Any, ...] | None = None
        self.active_request_signature: tuple[Any, ...] | None = None
        self.discard_active_results = False
        self.locked_result_stats: desktop_models.LockedCombatStats | None = None
        self.all_weapon_names: list[str] = []
        self.all_affinities: list[str] = []
        self.stat_widgets: dict[str, QtWidgets.QSpinBox] = {}
        self.scaling_cache: dict[tuple[str, str], tuple[float, float, float, float, float]] = {}
        self.result_cards: list[dict[str, Any]] = []
        self.active_compare_selected: desktop_models.SolvedBuild | None = None
        self.active_compare_target: desktop_models.SolvedBuild | None = None
        self.compare_resolution_error: str | None = None
        self.selected_result_fingerprint: tuple[Any, ...] | None = None
        self.session: desktop_models.GlobalSession | None = None
        self.ui_state = desktop_widgets.UiState()
        self.workflow_buttons: dict[str, QtWidgets.QPushButton] = {}

        self._build_ui()
        self._populate_static_lists()
        self._wire_events()
        self._refresh_affinity_options()
        self._refresh_compare_weapon_options()
        self._sync_session_state()
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
        layout.setSpacing(10)

        left_panel = QtWidgets.QWidget()
        left_panel.setObjectName("LeftRail")
        left_panel.setMinimumWidth(280)
        left_panel.setMaximumWidth(340)
        left_outer = QtWidgets.QVBoxLayout(left_panel)
        left_outer.setContentsMargins(0, 0, 0, 0)
        left_outer.setSpacing(8)

        left_content = QtWidgets.QWidget()
        left_content.setObjectName("CommandRail")
        left_content_layout = QtWidgets.QVBoxLayout(left_content)
        left_content_layout.setContentsMargins(10, 10, 10, 10)
        left_content_layout.setSpacing(8)
        left_content_layout.addWidget(self._build_brand_panel())
        left_content_layout.addWidget(self._build_workflow_nav())
        left_content_layout.addWidget(self._build_character_group())
        left_content_layout.addWidget(self._build_weapon_group())
        left_content_layout.addWidget(self._build_options_group())
        left_content_layout.addWidget(self._build_advanced_drawer())
        left_content_layout.addStretch(1)

        left_scroll = QtWidgets.QScrollArea()
        left_scroll.setWidgetResizable(True)
        left_scroll.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        left_scroll.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        left_scroll.setWidget(left_content)
        left_outer.addWidget(left_scroll, 1)

        workspace_panel = QtWidgets.QWidget()
        workspace_panel.setObjectName("WorkspacePanel")
        workspace_layout = QtWidgets.QVBoxLayout(workspace_panel)
        workspace_layout.setContentsMargins(0, 0, 0, 0)
        workspace_layout.setSpacing(10)
        self._build_hero_header()
        self.main_tabs = QtWidgets.QTabWidget()
        self.main_tabs.setObjectName("WorkspaceTabs")
        self.main_tabs.setDocumentMode(True)
        self.main_tabs.addTab(self._build_results_group(), "RANKINGS")
        self.main_tabs.addTab(self._build_upgrade_group(), "COMPARE")
        self.main_tabs.addTab(self._build_paths_group(), "PATHS")
        self.main_tabs.addTab(self._build_affinity_watch_group(), "AFFINITY WATCH")
        self.main_tabs.tabBar().hide()
        workspace_layout.addWidget(self.hero_panel, 0)
        workspace_layout.addWidget(self.main_tabs, 1)

        splitter = QtWidgets.QSplitter()
        splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)
        splitter.setChildrenCollapsible(False)
        splitter.addWidget(left_panel)
        splitter.addWidget(workspace_panel)
        splitter.addWidget(self._build_inspector_panel())
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        splitter.setStretchFactor(2, 0)
        splitter.setSizes([306, 934, 320])
        layout.addWidget(splitter)

    def _build_brand_panel(self) -> QtWidgets.QWidget:
        frame = desktop_widgets.band_frame("ArmoryBrand")
        layout = QtWidgets.QVBoxLayout(frame)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(4)
        title = QtWidgets.QLabel("Tarnished's Arsenal")
        title.setProperty("role", "brandTitle")
        sub = QtWidgets.QLabel("TACTICAL BUILD PLANNER")
        sub.setProperty("role", "brandSub")
        layout.addWidget(title)
        layout.addWidget(sub)
        return frame

    def _build_workflow_nav(self) -> QtWidgets.QWidget:
        frame = desktop_widgets.band_frame("WorkflowNav")
        layout = QtWidgets.QVBoxLayout(frame)
        layout.setContentsMargins(10, 10, 10, 10)
        layout.setSpacing(6)
        entries = [
            ("rankings", "Rankings", "rankings"),
            ("compare", "Compare", "compare"),
            ("paths", "Paths", "path"),
            ("affinity_watch", "Affinity Watch", "affinity"),
        ]
        for workspace, text, icon_name in entries:
            button = QtWidgets.QPushButton(text.upper())
            button.setProperty("role", "navButton")
            button.setProperty("active", workspace == "rankings")
            button.setIcon(desktop_theme.icon(icon_name))
            self.workflow_buttons[workspace] = button
            layout.addWidget(button)
        return frame

    def _build_character_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("CHARACTER SETUP")
        group.setObjectName("BuildGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(8)

        self.class_combo = QtWidgets.QComboBox()
        self.level_spin = self._u16_spin(1, 713, 150)
        self.level_spin.setReadOnly(True)
        self.level_spin.setButtonSymbols(QtWidgets.QAbstractSpinBox.ButtonSymbols.NoButtons)
        self.level_spin.setFocusPolicy(QtCore.Qt.FocusPolicy.NoFocus)

        top_row = QtWidgets.QGridLayout()
        top_row.setHorizontalSpacing(10)
        top_row.addWidget(self._field_stack("Starting Class", self.class_combo), 0, 0)
        top_row.addWidget(self._field_stack("Derived Level", self.level_spin), 1, 0)
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
        stat_header = QtWidgets.QLabel("STAT")
        stat_header.setProperty("role", "gridHeader")
        current_label = QtWidgets.QLabel("CURRENT")
        current_label.setProperty("role", "gridHeader")
        current_label.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        grid.addWidget(stat_header, 0, 0)
        grid.addWidget(current_label, 0, 1)

        self._add_stat_row(grid, 1, "VIG", self.vig_spin, None, show_floor=False)
        self._add_stat_row(grid, 2, "MND", self.mnd_spin, None, show_floor=False)
        self._add_stat_row(grid, 3, "END", self.end_spin, None, show_floor=False)
        self._add_stat_row(grid, 4, "STR", self.str_spin, None, show_floor=False)
        self._add_stat_row(grid, 5, "DEX", self.dex_spin, None, show_floor=False)
        self._add_stat_row(grid, 6, "INT", self.int_spin, None, show_floor=False)
        self._add_stat_row(grid, 7, "FAI", self.fai_spin, None, show_floor=False)
        self._add_stat_row(grid, 8, "ARC", self.arc_spin, None, show_floor=False)
        layout.addLayout(grid)
        return group

    def _build_weapon_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("SEARCH SCOPE")
        group.setObjectName("ConstraintsGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(8)

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

        layout.addWidget(self._field_stack("Weapon", self.weapon_combo))
        two_col = QtWidgets.QGridLayout()
        two_col.setHorizontalSpacing(8)
        two_col.setVerticalSpacing(8)
        two_col.addWidget(self._field_stack("Affinity", self.affinity_combo), 0, 0)
        two_col.addWidget(self._field_stack("AoW", self.aow_combo), 0, 1)
        layout.addLayout(two_col)
        return group

    def _build_options_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("OBJECTIVE")
        group.setObjectName("SearchGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(8)

        self.lock_upgrade_exact = QtWidgets.QCheckBox("Lock Upgrade Exact")
        self.two_handing_check = QtWidgets.QCheckBox("Two Handing")
        self.lock_stats_checkbox = QtWidgets.QCheckBox("Use Locked Result Stats")

        self.objective_combo = QtWidgets.QComboBox()
        self.objective_combo.addItem("Max AR", "max_ar")
        self.objective_combo.addItem("Max AR + Bleed", "max_ar_plus_bleed")
        self.objective_combo.addItem("AoW First Hit (PvE)", "aow_first_hit")
        self.objective_combo.addItem("AoW Full Sequence (PvE)", "aow_full_sequence")

        objective_grid = QtWidgets.QGridLayout()
        objective_grid.setHorizontalSpacing(8)
        objective_grid.setVerticalSpacing(8)
        objective_grid.addWidget(self._field_stack("Objective", self.objective_combo), 0, 0)
        objective_grid.addWidget(self._field_stack("Max Upgrade", self.max_upgrade_spin), 0, 1)
        layout.addLayout(objective_grid)

        self.requirement_badge = self._chip_label("No weapon selected", "muted", "requirementBadge")
        self.free_points_label = QtWidgets.QLabel("Redistributable Combat Points: -")
        self.estimate_label = QtWidgets.QLabel("Search Space: -")
        self.requirement_label = QtWidgets.QLabel("Requirements: -")
        for label in (self.free_points_label, self.estimate_label, self.requirement_label):
            label.setProperty("role", "statusLine")

        self.search_button = QtWidgets.QPushButton("Search the Arsenal")
        self.search_button.setProperty("role", "ctaButton")
        self.search_button.setIcon(desktop_theme.icon("search"))
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

    def _build_advanced_drawer(self) -> desktop_widgets.AdvancedDrawer:
        self.advanced_drawer = desktop_widgets.AdvancedDrawer("Advanced")
        layout = self.advanced_drawer.body_layout

        layout.addWidget(self._field_stack("Weapon Type", self.weapon_type_combo))
        grid = QtWidgets.QGridLayout()
        grid.setHorizontalSpacing(8)
        grid.setVerticalSpacing(8)
        grid.addWidget(self._field_stack("Somber Filter", self.somber_combo), 0, 0)
        grid.addWidget(self._field_stack("Top Results", self.top_k_spin), 0, 1)
        layout.addLayout(grid)

        floor_grid = QtWidgets.QGridLayout()
        floor_grid.setHorizontalSpacing(8)
        floor_grid.setVerticalSpacing(6)
        floor_grid.addWidget(self._field_stack("Min STR", self.min_str_spin), 0, 0)
        floor_grid.addWidget(self._field_stack("Min DEX", self.min_dex_spin), 0, 1)
        floor_grid.addWidget(self._field_stack("Min INT", self.min_int_spin), 1, 0)
        floor_grid.addWidget(self._field_stack("Min FAI", self.min_fai_spin), 1, 1)
        floor_grid.addWidget(self._field_stack("Min ARC", self.min_arc_spin), 2, 0)
        layout.addLayout(floor_grid)

        self.lock_upgrade_exact.setToolTip("Require the optimizer to use the current max upgrade exactly.")
        self.two_handing_check.setToolTip("Apply two-handing strength behavior when eligible.")
        self.lock_stats_checkbox.setToolTip("Reuse the combat stats captured from Use As Locks.")
        layout.addWidget(self.lock_upgrade_exact)
        layout.addWidget(self.two_handing_check)
        layout.addWidget(self.lock_stats_checkbox)
        self.advanced_drawer.toggled.connect(self._on_advanced_drawer_toggled)
        return self.advanced_drawer

    def _build_inspector_panel(self) -> QtWidgets.QWidget:
        panel = QtWidgets.QWidget()
        panel.setObjectName("InspectorPanel")
        panel.setMinimumWidth(260)
        panel.setMaximumWidth(350)
        outer = QtWidgets.QVBoxLayout(panel)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(0)

        content = QtWidgets.QWidget()
        layout = QtWidgets.QVBoxLayout(content)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)

        selected_block, selected_layout = desktop_widgets.inspector_block("Selected Result")
        self.inspector_title = desktop_widgets.text_label("No result selected", "inspectorTitle", True)
        self.inspector_detail = desktop_widgets.text_label("Run Rankings, then select a build row.", "summaryBody", True)
        self.inspector_stats = desktop_widgets.text_label("STR --  DEX --  INT --  FAI --  ARC --", "inspectorMetric", True)
        self.inspector_metrics = desktop_widgets.text_label("AR --   Bleed --   AoW --", "inspectorMetric", True)
        selected_layout.addWidget(self.inspector_title)
        selected_layout.addWidget(self.inspector_detail)
        selected_layout.addWidget(self.inspector_stats)
        selected_layout.addWidget(self.inspector_metrics)
        layout.addWidget(selected_block)

        budget_block, budget_layout = desktop_widgets.inspector_block("Stat Budget")
        self.inspector_level_label = desktop_widgets.text_label("Level --", "statusLine")
        self.inspector_budget_label = desktop_widgets.text_label("Budget --", "statusLine")
        self.inspector_requirement_label = desktop_widgets.text_label("Requirements: -", "statusLine", True)
        budget_layout.addWidget(self.inspector_level_label)
        budget_layout.addWidget(self.inspector_budget_label)
        budget_layout.addWidget(self.inspector_requirement_label)
        layout.addWidget(budget_block)

        locks_block, locks_layout = desktop_widgets.inspector_block("Locks")
        self.inspector_lock_state = desktop_widgets.text_label("Open Search", "summaryBody", True)
        self.inspector_lock_button = QtWidgets.QPushButton("Use As Locks")
        self.inspector_lock_button.setProperty("role", "inlineButton")
        self.inspector_lock_button.setIcon(desktop_theme.icon("lock"))
        self.inspector_lock_button.setEnabled(False)
        locks_layout.addWidget(self.inspector_lock_state)
        locks_layout.addWidget(self.inspector_lock_button)
        layout.addWidget(locks_block)

        actions_block, actions_layout = desktop_widgets.inspector_block("Next Actions")
        self.inspector_compare_button = QtWidgets.QPushButton("Open Compare")
        self.inspector_compare_button.setProperty("role", "inlineButton")
        self.inspector_compare_button.setIcon(desktop_theme.icon("compare"))
        self.inspector_path_button = QtWidgets.QPushButton("Run Paths")
        self.inspector_path_button.setProperty("role", "inlineButton")
        self.inspector_path_button.setIcon(desktop_theme.icon("path"))
        self.inspector_affinity_button = QtWidgets.QPushButton("Run Affinity Watch")
        self.inspector_affinity_button.setProperty("role", "inlineButton")
        self.inspector_affinity_button.setIcon(desktop_theme.icon("affinity"))
        for button in (
            self.inspector_compare_button,
            self.inspector_path_button,
            self.inspector_affinity_button,
        ):
            button.setEnabled(False)
            actions_layout.addWidget(button)
        layout.addWidget(actions_block)
        layout.addStretch(1)

        scroll = QtWidgets.QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        scroll.setWidget(content)
        outer.addWidget(scroll)
        return panel

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
        level_card, self.hero_level_value = self._metric_card("Level")
        budget_card, self.hero_budget_value = self._metric_card("Budget")
        free_card, self.hero_free_value = self._metric_card("Free")
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

    def _centered_table_item(self, value: str) -> QtWidgets.QTableWidgetItem:
        item = QtWidgets.QTableWidgetItem(value)
        item.setTextAlignment(
            int(
                QtCore.Qt.AlignmentFlag.AlignCenter
                | QtCore.Qt.AlignmentFlag.AlignVCenter
            )
        )
        return item

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
        show_floor: bool = True,
    ) -> None:
        name_label = QtWidgets.QLabel(name)
        name_label.setProperty("role", "statName")
        grid.addWidget(name_label, row, 0)
        grid.addWidget(current_spin, row, 1)
        if not show_floor:
            return
        if min_spin is None:
            dash = QtWidgets.QLabel("--")
            dash.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
            dash.setProperty("role", "statDash")
            grid.addWidget(dash, row, 2)
        else:
            grid.addWidget(min_spin, row, 2)

    def _build_results_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("RANKED BUILD BOARD")
        group.setObjectName("ResultsGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(10)
        layout.addWidget(self._helper_label("Best lanes first. Precision table below."))

        self.result_cards_container = QtWidgets.QWidget()
        cards_layout = QtWidgets.QVBoxLayout(self.result_cards_container)
        cards_layout.setContentsMargins(0, 0, 0, 0)
        cards_layout.setSpacing(8)
        for card_idx in range(3):
            card = self._build_result_card(card_idx)
            self.result_cards.append(card)
            cards_layout.addWidget(card["frame"], 0)
        layout.addWidget(self.result_cards_container)

        self.results_table = QtWidgets.QTableWidget(0, 18)
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
                "Split",
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
        group = QtWidgets.QGroupBox("COMPARE LANES")
        group.setObjectName("UpgradeGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(10)
        layout.addWidget(self._helper_label("Selected lane versus target lane."))

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
        self.affinity_watch_button = QtWidgets.QPushButton("Affinity Watcher")
        self.affinity_watch_button.setProperty("role", "inlineButton")
        self.affinity_watch_button.setEnabled(False)
        path_toolbar.addWidget(self._field_stack("Current + N", self.level_path_horizon_spin), 0)
        path_toolbar.addWidget(self.level_path_button, 0)
        path_toolbar.addWidget(self.affinity_watch_button, 0)
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

    def _build_paths_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("PATH LANES")
        group.setObjectName("PathsGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(10)
        layout.addWidget(self._helper_label("Current + N stat route."))

        self.path_workspace_summary = QtWidgets.QLabel("No selected path lane yet.")
        self.path_workspace_summary.setProperty("role", "summaryBody")
        self.path_workspace_summary.setWordWrap(True)
        layout.addWidget(self.path_workspace_summary)

        self.path_workspace_detail = QtWidgets.QLabel(
            "Pick a selected result and a comparison target to trace the exact Current + N stat route."
        )
        self.path_workspace_detail.setProperty("role", "statusLine")
        self.path_workspace_detail.setWordWrap(True)
        layout.addWidget(self.path_workspace_detail)

        self.path_tab_open_button = QtWidgets.QPushButton("Refresh Paths")
        self.path_tab_open_button.setProperty("role", "inlineButton")
        self.path_tab_open_button.setEnabled(False)
        layout.addWidget(self.path_tab_open_button, 0, QtCore.Qt.AlignmentFlag.AlignLeft)

        self.path_progress_label = QtWidgets.QLabel("Idle")
        self.path_progress_label.setProperty("role", "progressLabel")
        self.path_progress_bar = QtWidgets.QProgressBar()
        self.path_progress_bar.setTextVisible(True)
        self.path_progress_bar.setRange(0, 1)
        self.path_progress_bar.setValue(0)
        layout.addWidget(self.path_progress_label)
        layout.addWidget(self.path_progress_bar)

        self.path_chart_widget = PathChartWidget()
        layout.addWidget(self.path_chart_widget)

        self.path_tables_splitter = QtWidgets.QSplitter(QtCore.Qt.Orientation.Horizontal)
        self.path_tables_splitter.setChildrenCollapsible(False)
        layout.addWidget(self.path_tables_splitter, 1)
        self._populate_path_panels([])
        layout.addStretch(1)
        return group

    def _build_affinity_watch_group(self) -> QtWidgets.QGroupBox:
        group = QtWidgets.QGroupBox("AFFINITY WATCH")
        group.setObjectName("AffinityWatchGroup")
        layout = QtWidgets.QVBoxLayout(group)
        layout.setSpacing(10)
        layout.addWidget(self._helper_label("Final ranking, crossovers, and trace."))

        self.affinity_workspace_summary = QtWidgets.QLabel("No selected affinity lane yet.")
        self.affinity_workspace_summary.setProperty("role", "summaryBody")
        self.affinity_workspace_summary.setWordWrap(True)
        layout.addWidget(self.affinity_workspace_summary)

        self.affinity_workspace_detail = QtWidgets.QLabel(
            "Pick a selected result row to compare legal affinities from Current to Current + N."
        )
        self.affinity_workspace_detail.setProperty("role", "statusLine")
        self.affinity_workspace_detail.setWordWrap(True)
        layout.addWidget(self.affinity_workspace_detail)

        self.affinity_tab_open_button = QtWidgets.QPushButton("Refresh Affinity Watch")
        self.affinity_tab_open_button.setProperty("role", "inlineButton")
        self.affinity_tab_open_button.setEnabled(False)
        layout.addWidget(self.affinity_tab_open_button, 0, QtCore.Qt.AlignmentFlag.AlignLeft)

        self.affinity_progress_label = QtWidgets.QLabel("Idle")
        self.affinity_progress_label.setProperty("role", "progressLabel")
        self.affinity_progress_bar = QtWidgets.QProgressBar()
        self.affinity_progress_bar.setTextVisible(True)
        self.affinity_progress_bar.setRange(0, 1)
        self.affinity_progress_bar.setValue(0)
        layout.addWidget(self.affinity_progress_label)
        layout.addWidget(self.affinity_progress_bar)

        self.affinity_chart_widget = AffinityWatchChartWidget()
        layout.addWidget(self.affinity_chart_widget)

        self.affinity_summary_table = QtWidgets.QTableWidget(0, 4)
        self.affinity_summary_table.setHorizontalHeaderLabels(["Affinity", "Start", "End", "Final Stats"])
        self.affinity_summary_table.horizontalHeader().setSectionResizeMode(QtWidgets.QHeaderView.ResizeMode.ResizeToContents)
        self.affinity_summary_table.horizontalHeader().setStretchLastSection(True)
        self.affinity_summary_table.verticalHeader().setVisible(False)
        self.affinity_summary_table.setEditTriggers(QtWidgets.QAbstractItemView.EditTrigger.NoEditTriggers)
        self.affinity_summary_table.setSelectionMode(QtWidgets.QAbstractItemView.SelectionMode.NoSelection)
        self.affinity_summary_table.setAlternatingRowColors(True)
        self.affinity_summary_table.setShowGrid(False)
        layout.addWidget(self.affinity_summary_table, 1)

        self.affinity_breakpoint_table = QtWidgets.QTableWidget(0, 5)
        self.affinity_breakpoint_table.setHorizontalHeaderLabels(["Level", "From", "To", "Old Metric", "New Metric"])
        self.affinity_breakpoint_table.horizontalHeader().setSectionResizeMode(QtWidgets.QHeaderView.ResizeMode.ResizeToContents)
        self.affinity_breakpoint_table.horizontalHeader().setStretchLastSection(True)
        self.affinity_breakpoint_table.verticalHeader().setVisible(False)
        self.affinity_breakpoint_table.setEditTriggers(QtWidgets.QAbstractItemView.EditTrigger.NoEditTriggers)
        self.affinity_breakpoint_table.setSelectionMode(QtWidgets.QAbstractItemView.SelectionMode.NoSelection)
        self.affinity_breakpoint_table.setAlternatingRowColors(True)
        self.affinity_breakpoint_table.setShowGrid(False)
        layout.addWidget(self.affinity_breakpoint_table, 1)
        layout.addStretch(1)
        return group

    def _build_result_card(self, card_idx: int) -> dict[str, Any]:
        frame = QtWidgets.QFrame()
        frame.setProperty("role", "resultCard")
        frame.setProperty("cardState", "empty")
        frame.setMinimumHeight(116)
        layout = QtWidgets.QVBoxLayout(frame)
        layout.setContentsMargins(12, 10, 12, 10)
        layout.setSpacing(4)

        rank_chip = self._chip_label(f"#{card_idx + 1}", "muted")
        title = QtWidgets.QLabel("No result yet")
        title.setProperty("role", "cardTitle")
        detail = QtWidgets.QLabel("Run a search to surface ranked weapon lines.")
        detail.setProperty("role", "cardDetail")
        detail.setWordWrap(True)
        stats = QtWidgets.QLabel("STR --  DEX --  INT --  FAI --  ARC --")
        stats.setProperty("role", "cardStats")
        stats.setWordWrap(True)
        metrics = QtWidgets.QLabel("AR --   Bleed --   1st --   Full --   Score --")
        metrics.setProperty("role", "cardMetric")
        metrics.setWordWrap(True)

        chip_row = QtWidgets.QHBoxLayout()
        chip_row.addWidget(rank_chip, 0)
        chip_row.addStretch(1)

        button_row = QtWidgets.QHBoxLayout()
        button_row.setSpacing(6)
        focus_button = QtWidgets.QPushButton("Focus")
        focus_button.setProperty("role", "inlineButton")
        focus_button.setIcon(desktop_theme.icon("rankings"))
        focus_button.clicked.connect(lambda _checked=False, idx=card_idx: self._focus_result_row(idx))
        lock_button = QtWidgets.QPushButton("Lock")
        lock_button.setProperty("role", "inlineButton")
        lock_button.setIcon(desktop_theme.icon("lock"))
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
        self.search_button.clicked.connect(self._handle_search_button)
        self.inspector_lock_button.clicked.connect(self._lock_selected_result)
        self.inspector_compare_button.clicked.connect(lambda: self.main_tabs.setCurrentIndex(1))
        self.inspector_path_button.clicked.connect(self._handle_level_path_button)
        self.inspector_affinity_button.clicked.connect(self._handle_affinity_watch_button)
        for workspace, button in self.workflow_buttons.items():
            button.clicked.connect(lambda _checked=False, name=workspace: self._activate_workspace(name))
        self.class_combo.currentIndexChanged.connect(self._on_class_changed)
        self.weapon_combo.currentIndexChanged.connect(self._refresh_affinity_options)
        self.weapon_type_combo.currentIndexChanged.connect(self._refresh_weapon_options)
        self.affinity_combo.currentIndexChanged.connect(self._refresh_aow_options)
        self.compare_weapon_type_combo.currentIndexChanged.connect(self._refresh_compare_weapon_options)
        self.compare_weapon_combo.currentIndexChanged.connect(self._refresh_compare_affinity_options)
        self.compare_affinity_combo.currentIndexChanged.connect(self._refresh_compare_aow_options)
        self.results_table.itemSelectionChanged.connect(self._rebuild_upgrade_table)
        self.results_table.itemSelectionChanged.connect(self._refresh_result_cards)
        self.level_path_button.clicked.connect(self._handle_level_path_button)
        self.affinity_watch_button.clicked.connect(self._handle_affinity_watch_button)
        self.path_tab_open_button.clicked.connect(self._handle_level_path_button)
        self.affinity_tab_open_button.clicked.connect(self._handle_affinity_watch_button)
        self.level_path_horizon_spin.valueChanged.connect(self._refresh_analysis_workspace_labels)
        self.main_tabs.currentChanged.connect(self._on_workspace_changed)
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
        type_options = self.weapon_type_options or [(key, key) for key in self.data.weapon_type_keys()]
        for label, key in type_options:
            self.weapon_type_combo.addItem(label, key)

        self.compare_weapon_type_combo.addItem(ALL_OPTION, None)
        for label, key in type_options:
            self.compare_weapon_type_combo.addItem(label, key)

        self._refresh_weapon_options()
        self._refresh_compare_weapon_options()
        self._enable_searchable_dropdowns()
        self._apply_class_baselines()
        self._refresh_estimate()

    def _refresh_weapon_options(self) -> None:
        selected_type = self._combo_value(self.weapon_type_combo)
        previous_weapon = self._combo_value(self.weapon_combo)

        self.weapon_combo.blockSignals(True)
        self.weapon_combo.clear()
        self.weapon_combo.addItem(OPEN_OPTION, None)

        if selected_type is None:
            weapon_names = self.all_weapon_names
        else:
            weapon_names = self.data.weapon_names_for_type(selected_type)

        for weapon_name in weapon_names:
            self.weapon_combo.addItem(weapon_name, weapon_name)

        if previous_weapon is not None:
            idx = self.weapon_combo.findData(previous_weapon)
            if idx >= 0:
                self.weapon_combo.setCurrentIndex(idx)
            else:
                self.weapon_combo.setCurrentIndex(0)
        else:
            self.weapon_combo.setCurrentIndex(0)

        self.weapon_combo.blockSignals(False)
        self._refresh_affinity_options()

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

    def _on_advanced_drawer_toggled(self, open_: bool) -> None:
        self.ui_state.advanced_open = open_

    def _activate_workspace(self, workspace: str) -> None:
        index_by_workspace = {
            "rankings": 0,
            "compare": 1,
            "paths": 2,
            "affinity_watch": 3,
        }
        index = index_by_workspace.get(workspace, 0)
        self.main_tabs.setCurrentIndex(index)

    def _on_workspace_changed(self, _index: int) -> None:
        self.ui_state.active_workspace = self._active_workspace()
        self._refresh_workflow_nav()
        self._sync_session_state()

    def _refresh_workflow_nav(self) -> None:
        active = self._active_workspace()
        for workspace, button in self.workflow_buttons.items():
            button.setProperty("active", workspace == active)
            self._restyle_widget(button)

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

    def _active_workspace(self) -> desktop_models.WorkspaceTab:
        match self.main_tabs.currentIndex():
            case 1:
                return "compare"
            case 2:
                return "paths"
            case 3:
                return "affinity_watch"
            case _:
                return "rankings"

    def _resolved_compare_aow_state(self) -> tuple[str | None, bool]:
        compare_aow_value = self._combo_value(self.compare_aow_combo)
        if compare_aow_value == "__match_selected__":
            return None, True
        return compare_aow_value, False

    def _current_build_session(self) -> desktop_models.BuildSession:
        self._sync_derived_level()
        return desktop_models.BuildSession(
            class_name=self._resolved_class_name(),
            vig=self.vig_spin.value(),
            mnd=self.mnd_spin.value(),
            end=self.end_spin.value(),
            str_stat=self.str_spin.value(),
            dex=self.dex_spin.value(),
            int_stat=self.int_spin.value(),
            fai=self.fai_spin.value(),
            arc=self.arc_spin.value(),
            min_str=self.min_str_spin.value(),
            min_dex=self.min_dex_spin.value(),
            min_int=self.min_int_spin.value(),
            min_fai=self.min_fai_spin.value(),
            min_arc=self.min_arc_spin.value(),
            two_handing=self.two_handing_check.isChecked(),
        )

    def _current_search_scope(self) -> desktop_models.SearchScope:
        return desktop_models.SearchScope(
            weapon_type_key=self._combo_value(self.weapon_type_combo),
            weapon_name=self._combo_value(self.weapon_combo),
            affinity=self._combo_value(self.affinity_combo),
            aow_name=self._combo_value(self.aow_combo),
            somber_filter=self.somber_combo.currentData(),
            max_upgrade=self.max_upgrade_spin.value(),
            exact_upgrade=self.lock_upgrade_exact.isChecked(),
            top_k=self.top_k_spin.value(),
        )

    def _current_analysis_state(self) -> desktop_models.AnalysisState:
        compare_aow_name, compare_match_selected_aow = self._resolved_compare_aow_state()
        return desktop_models.AnalysisState(
            selected_fingerprint=self.selected_result_fingerprint,
            compare_weapon_type_key=self._combo_value(self.compare_weapon_type_combo),
            compare_weapon_name=self._combo_value(self.compare_weapon_combo),
            compare_affinity=self._combo_value(self.compare_affinity_combo),
            compare_aow_name=compare_aow_name,
            compare_match_selected_aow=compare_match_selected_aow,
            levels_ahead=self.level_path_horizon_spin.value(),
            active_workspace=self._active_workspace(),
        )

    def _current_session(self) -> desktop_models.GlobalSession:
        return desktop_models.GlobalSession(
            build=self._current_build_session(),
            scope=self._current_search_scope(),
            objective_id=self.objective_combo.currentData(),
            locked_combat_stats=self.locked_result_stats,
            use_locked_stats=self.lock_stats_checkbox.isChecked(),
            analysis=self._current_analysis_state(),
        )

    def _sync_session_state(self) -> None:
        self.session = self._current_session()

    def _build_request_kwargs(self, include_progress: bool) -> dict[str, Any]:
        self._sync_session_state()
        return self.desktop_service.build_optimize_request(
            self.session,
            include_progress=include_progress,
        )

    def _refresh_estimate(self) -> None:
        self._sync_derived_level()
        self._sync_session_state()
        self.desktop_service.clear_caches()
        request_signature = self._search_request_signature()
        if self.results_signature is not None and request_signature != self.results_signature:
            self._clear_results_state()
        if self.active_run_id is not None:
            self.discard_active_results = request_signature != self.active_request_signature
        try:
            estimate = self.desktop_service.estimate_search_space(self.session)
            self.estimate_label.setText(
                f"Search Space: {estimate.combinations:,} "
                f"({self._weapon_candidate_summary(estimate.weapon_candidates)} x "
                f"{estimate.stat_candidates:,} stat states)"
            )
            self.free_points_label.setText(self._compute_free_points_text())
            self._update_requirement_highlights()
            self._refresh_hero_summary()
            self._refresh_analysis_workspace_labels()
            self._refresh_inspector()
        except Exception as exc:
            self.estimate_label.setText(f"Search Space: invalid ({exc})")
            self.free_points_label.setText("Redistributable Combat Points: invalid")
            self._update_requirement_highlights()
            self._refresh_hero_summary()
            self._refresh_analysis_workspace_labels()
            self._refresh_inspector()

    def _compute_free_points_text(self) -> str:
        snapshot = self._budget_snapshot()
        return (
            f"Redistributable Combat Points: {snapshot['redistributable']} "
            f"(Level {snapshot['level']}, Budget {snapshot['total']})"
        )

    def _weapon_candidate_summary(self, weapon_candidates: int) -> str:
        selected_weapon = self._combo_value(self.weapon_combo)
        if selected_weapon is not None:
            return f"{weapon_candidates} weapon lines"
        visible_weapon_names = max(0, self.weapon_combo.count() - 1)
        if visible_weapon_names > 0 and visible_weapon_names != weapon_candidates:
            return f"{weapon_candidates} weapon lines / {visible_weapon_names} weapons"
        return f"{weapon_candidates} weapon lines"

    def _budget_snapshot(self) -> dict[str, int]:
        self._sync_session_state()
        return self.session.build.budget_snapshot()

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
            "Locked" if self._has_locked_filters() else "Open",
            "info" if self._has_locked_filters() else "muted",
        )
        self._set_toned_label(self.hero_somber_chip, self._somber_chip_text(), "muted")
        self._set_toned_label(
            self.hero_handing_chip,
            "2H" if self.two_handing_check.isChecked() else "1H",
            "accent" if self.two_handing_check.isChecked() else "muted",
        )
        self._set_toned_label(
            self.hero_upgrade_chip,
            "Exact +" if self.lock_upgrade_exact.isChecked() else "+Range",
            "info" if self.lock_upgrade_exact.isChecked() else "muted",
        )
        exact_stats = self.lock_stats_checkbox.isChecked() and self.locked_result_stats is not None
        self._set_toned_label(
            self.hero_stats_chip,
            "Exact Stats" if exact_stats else "Stats",
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
            return "Std"
        if value == "somber_only":
            return "Somber"
        return "All"

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
        self._sync_session_state()
        return self.desktop_service.search_request_signature(self.session)

    def _clear_results_state(self) -> None:
        self.current_results = []
        self.results_signature = None
        self.active_compare_selected = None
        self.active_compare_target = None
        self.selected_result_fingerprint = None
        self.results_table.clearContents()
        self.results_table.setRowCount(0)
        self.upgrade_table.clearContents()
        self.upgrade_table.setRowCount(0)
        self.upgrade_table.setColumnCount(0)
        self.level_path_button.setEnabled(False)
        self.path_tab_open_button.setEnabled(False)
        self.path_preview_signature = None
        self.affinity_watch_signature = None
        self._refresh_affinity_watch_button_state()
        self._refresh_path_button_state()
        self._refresh_result_cards()
        self._refresh_compare_summary(None, None, None)
        self._refresh_inspector()

    @staticmethod
    def _restyle_widget(widget: QtWidgets.QWidget) -> None:
        widget.style().unpolish(widget)
        widget.style().polish(widget)

    def _refresh_search_button_state(self) -> None:
        if self.active_run_id is None:
            self.search_button.setEnabled(True)
            self.search_button.setText("Search the Arsenal")
            self.search_button.setIcon(desktop_theme.icon("search"))
            self.ui_state.busy_search = False
            return
        if self.search_cancel_requested:
            self.search_button.setEnabled(False)
            self.search_button.setText("Stopping...")
            self.search_button.setIcon(desktop_theme.icon("stop"))
            self.ui_state.busy_search = True
            return
        self.search_button.setEnabled(True)
        self.search_button.setText("Stop Search")
        self.search_button.setIcon(desktop_theme.icon("stop"))
        self.ui_state.busy_search = True

    def _refresh_affinity_watch_button_state(self) -> None:
        if self.affinity_watch_thread is None:
            enabled = self.active_compare_selected is not None
            self.affinity_watch_button.setEnabled(enabled)
            self.affinity_watch_button.setText("Affinity Watcher")
            self.affinity_tab_open_button.setEnabled(enabled)
            self.affinity_tab_open_button.setText("Refresh Affinity Watch")
            self.ui_state.busy_affinity = False
            self._refresh_inspector()
            return
        if self.affinity_watch_cancel_requested:
            self.affinity_watch_button.setEnabled(False)
            self.affinity_watch_button.setText("Stopping...")
            self.affinity_tab_open_button.setEnabled(False)
            self.affinity_tab_open_button.setText("Stopping...")
            self.ui_state.busy_affinity = True
            self._refresh_inspector()
            return
        self.affinity_watch_button.setEnabled(True)
        self.affinity_watch_button.setText("Stop Affinity Watch")
        self.affinity_tab_open_button.setEnabled(True)
        self.affinity_tab_open_button.setText("Stop Affinity Watch")
        self.ui_state.busy_affinity = True
        self._refresh_inspector()

    def _refresh_path_button_state(self) -> None:
        if self.path_thread is None:
            enabled = self.active_compare_selected is not None and self.active_compare_target is not None
            self.level_path_button.setEnabled(enabled)
            self.level_path_button.setText("Path Graphs")
            self.path_tab_open_button.setEnabled(enabled)
            self.path_tab_open_button.setText("Refresh Paths")
            self.ui_state.busy_paths = False
            self._refresh_inspector()
            return
        if self.path_cancel_requested:
            self.level_path_button.setEnabled(False)
            self.level_path_button.setText("Stopping...")
            self.path_tab_open_button.setEnabled(False)
            self.path_tab_open_button.setText("Stopping...")
            self.ui_state.busy_paths = True
            self._refresh_inspector()
            return
        self.level_path_button.setEnabled(True)
        self.level_path_button.setText("Stop Paths")
        self.path_tab_open_button.setEnabled(True)
        self.path_tab_open_button.setText("Stop Paths")
        self.ui_state.busy_paths = True
        self._refresh_inspector()

    def _handle_search_button(self) -> None:
        if self.active_run_id is not None:
            self._cancel_search()
            return
        self._start_search()

    def _cancel_search(self) -> None:
        if self.active_run_id is None or self.worker is None or self.search_cancel_requested:
            return
        self.search_cancel_requested = True
        self.progress_label.setText("Stopping search...")
        self.worker.cancel()
        self._refresh_search_button_state()

    def _handle_affinity_watch_button(self) -> None:
        if self.affinity_watch_thread is not None:
            self._cancel_affinity_watch()
            return
        self._open_affinity_watch_dialog()

    def _handle_level_path_button(self) -> None:
        if self.path_thread is not None:
            self._cancel_level_path()
            return
        self._open_level_path_dialog()

    def _cancel_affinity_watch(self) -> None:
        if (
            self.affinity_watch_thread is None
            or self.affinity_watch_worker is None
            or self.affinity_watch_cancel_requested
        ):
            return
        self.affinity_watch_cancel_requested = True
        self.affinity_progress_label.setText("Stopping affinity watch...")
        self.affinity_watch_worker.cancel()
        self._refresh_affinity_watch_button_state()

    def _cancel_level_path(self) -> None:
        if self.path_thread is None or self.path_worker is None or self.path_cancel_requested:
            return
        self.path_cancel_requested = True
        self.path_progress_label.setText("Stopping path preview...")
        self.path_worker.cancel()
        self._refresh_path_button_state()

    def _start_search(self) -> None:
        if self.active_run_id is not None:
            return

        try:
            self._sync_session_state()
            estimate = self.desktop_service.estimate_search_space(self.session)
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
        self.search_cancel_requested = False
        self.active_request_signature = self._search_request_signature()
        self.discard_active_results = False
        self._clear_results_state()
        self._set_search_progress_bar(0, total)
        self.progress_label.setText(f"Searching 0 / {total:,}...")
        self._refresh_search_button_state()
        self._refresh_hero_summary()

        worker = OptimizeWorker(
            run_id=run_id,
            service=self.desktop_service,
            session=self.session,
        )
        worker.progress.connect(self._on_progress)
        worker.finished.connect(self._on_finished)
        worker.failed.connect(self._on_failed)
        worker.finished.connect(self._teardown_worker)
        worker.failed.connect(self._teardown_worker)
        self._launch_worker_thread("worker_thread", "worker", worker)

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
        self.search_cancel_requested = False
        self._refresh_search_button_state()
        if self.discard_active_results or self.active_request_signature != self._search_request_signature():
            self.active_run_id = None
            self.active_request_signature = None
            self.discard_active_results = False
            self._clear_results_state()
            self._set_idle_progress("Inputs changed during search. Rerun search.")
            return
        self.active_run_id = None
        self.current_results = [self.desktop_service.normalize_result(result) for result in results]
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
        was_cancelled = message == "cancelled"
        self.search_cancel_requested = False
        self.active_run_id = None
        self.active_request_signature = None
        self.discard_active_results = False
        self._refresh_search_button_state()
        if was_cancelled:
            self._set_idle_progress("Search stopped.")
            return
        self._set_idle_progress(f"Failed: {message}")

    def _launch_worker_thread(
        self,
        thread_attr: str,
        worker_attr: str,
        worker: QtCore.QObject,
    ) -> None:
        thread = QtCore.QThread(self)
        setattr(self, thread_attr, thread)
        setattr(self, worker_attr, worker)
        worker.moveToThread(thread)
        thread.started.connect(worker.run)
        thread.start()

    def _teardown_named_worker(self, thread_attr: str, worker_attr: str) -> None:
        thread = getattr(self, thread_attr)
        if thread is not None:
            thread.quit()
            thread.wait(1000)
        setattr(self, worker_attr, None)
        setattr(self, thread_attr, None)

    @QtCore.pyqtSlot()
    def _teardown_worker(self) -> None:
        self._teardown_named_worker("worker_thread", "worker")

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
                self._damage_split_text(result),
                f"{result.ar_total:.2f}",
                f"{result.bleed_buildup:.2f}",
                f"{result.aow_first_hit_damage:.2f}",
                f"{result.aow_full_sequence_damage:.2f}",
                f"{result.score:.2f}",
            ]
            for col_idx, value in enumerate(values):
                self.results_table.setItem(row_idx, col_idx, self._centered_table_item(value))

            lock_button = QtWidgets.QPushButton("Use As Locks")
            lock_button.setProperty("role", "inlineButton")
            lock_button.clicked.connect(lambda _checked=False, idx=row_idx: self._lock_from_result(idx))
            self.results_table.setCellWidget(row_idx, 17, lock_button)

        if self.current_results:
            selected_idx = 0
            if self.selected_result_fingerprint is not None:
                for idx, result in enumerate(self.current_results):
                    if result.fingerprint == self.selected_result_fingerprint:
                        selected_idx = idx
                        break
            self.results_table.selectRow(selected_idx)
            self.selected_result_fingerprint = self.current_results[selected_idx].fingerprint
        self._refresh_result_cards()
        self._refresh_hero_summary()

    def _focus_result_row(self, row_idx: int) -> None:
        if row_idx >= len(self.current_results):
            return
        self.results_table.selectRow(row_idx)

    def _lock_selected_result(self) -> None:
        selected_idx = self._selected_result_index()
        if selected_idx is None:
            return
        self._lock_from_result(selected_idx)

    def _selected_result_index(self) -> int | None:
        selected = self.results_table.selectionModel().selectedRows()
        if not selected:
            return None
        idx = selected[0].row()
        if idx >= len(self.current_results):
            return None
        self.selected_result_fingerprint = self.current_results[idx].fingerprint
        self._sync_session_state()
        return idx

    def _refresh_inspector(self) -> None:
        snapshot = self._budget_snapshot()
        self.inspector_level_label.setText(f"Level {snapshot['level']}")
        self.inspector_budget_label.setText(
            f"Budget {snapshot['total']} | Redistributable {snapshot['redistributable']}"
        )
        self.inspector_requirement_label.setText(self.requirement_label.text())
        self.inspector_lock_state.setText(
            "Exact upgrade and stat locks active"
            if self.lock_upgrade_exact.isChecked()
            and self.lock_stats_checkbox.isChecked()
            and self.locked_result_stats is not None
            else "Open or partial locks"
        )

        selected_idx = self._selected_result_index()
        selected_result = self.current_results[selected_idx] if selected_idx is not None else None
        has_result = selected_result is not None
        if selected_result is None:
            self.ui_state.inspector_mode = "empty"
            self.inspector_title.setText("No result selected")
            self.inspector_detail.setText("Run Rankings, then select a build row.")
            self.inspector_stats.setText("STR --  DEX --  INT --  FAI --  ARC --")
            self.inspector_metrics.setText("AR --   Bleed --   AoW --")
        else:
            self.ui_state.inspector_mode = "result"
            self.inspector_title.setText(
                f"#{selected_idx + 1} {selected_result.weapon_name} | {selected_result.affinity}"
            )
            self.inspector_detail.setText(
                f"AoW {selected_result.aow_name or '-'} | +{selected_result.upgrade} | "
                f"{self._scaling_summary(selected_result.weapon_name, selected_result.affinity, int(selected_result.upgrade))}"
            )
            self.inspector_stats.setText(
                f"STR {selected_result.str_stat}  DEX {selected_result.dex}  "
                f"INT {selected_result.int_stat}  FAI {selected_result.fai}  ARC {selected_result.arc}"
            )
            self.inspector_metrics.setText(self._result_metrics_text(selected_result))

        self.inspector_lock_button.setEnabled(has_result)
        self.inspector_compare_button.setEnabled(has_result)
        self.inspector_path_button.setEnabled(
            self.active_compare_selected is not None and self.active_compare_target is not None
        )
        self.inspector_affinity_button.setEnabled(self.active_compare_selected is not None)

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
        self._refresh_inspector()

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
        self.locked_result_stats = desktop_models.LockedCombatStats(
            str_stat=int(result.str_stat),
            dex=int(result.dex),
            int_stat=int(result.int_stat),
            fai=int(result.fai),
            arc=int(result.arc),
        )
        self.lock_stats_checkbox.setChecked(True)
        self._refresh_estimate()
        self._start_search()

    def _rebuild_upgrade_table(self) -> None:
        if not self.current_results:
            self.upgrade_table.setRowCount(0)
            self.upgrade_table.setColumnCount(0)
            self.active_compare_selected = None
            self.active_compare_target = None
            self.compare_resolution_error = None
            self._refresh_path_button_state()
            self._refresh_affinity_watch_button_state()
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
        selected_best = self._row_config_from_result(selected_result)
        compare_summary_row: desktop_models.SolvedBuild | None = None

        try:
            resolved_selected = self._best_row_config(
                selected_result.weapon_name,
                selected_result.affinity,
                selected_result.aow_name,
            )
            if resolved_selected is not None:
                selected_best = resolved_selected

            rows_to_render: list[tuple[str, Any]] = [
                (
                    f"Selected: {selected_best.weapon_name} | {selected_best.affinity} | "
                    f"AoW {selected_best.aow_name or '-'} | {self._format_best_stats(selected_best)}",
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
                    if compare_summary_row is None:
                        compare_summary_row = row_best
                    rows_to_render.append(
                        (
                            f"Top #{row_idx + 1}: {row_best.weapon_name} | {row_best.affinity} | "
                            f"AoW {row_best.aow_name or '-'} | {self._format_best_stats(row_best)}",
                            row_best,
                        )
                    )
            else:
                compare_affinity = self._combo_value(self.compare_affinity_combo)
                compare_aow_value = self._combo_value(self.compare_aow_combo)
                if compare_aow_value == "__match_selected__":
                    compare_aow = selected_best.aow_name
                else:
                    compare_aow = compare_aow_value
                compare_best = self._best_row_config(
                    compare_weapon,
                    compare_affinity,
                    compare_aow,
                )
                requested_affinity = compare_affinity or OPEN_OPTION
                if compare_best is not None:
                    compare_label = (
                        f"Compare: {compare_best.weapon_name} | {compare_best.affinity} | "
                        f"AoW {compare_best.aow_name or '-'} | {self._format_best_stats(compare_best)}"
                    )
                else:
                    compare_label = (
                        f"Compare: {compare_weapon} | {requested_affinity} | AoW {compare_aow or '-'} | "
                        "No valid build"
                    )
                rows_to_render.append((compare_label, compare_best))
                compare_summary_row = compare_best

            self.upgrade_table.setRowCount(len(rows_to_render))
            for row_idx, (label, row_data) in enumerate(rows_to_render):
                self.upgrade_table.setItem(row_idx, 0, self._centered_table_item(label))
                metric_series = self._locked_metric_series_for_config(row_data, max_upgrade)
                for lv in range(0, max_upgrade + 1):
                    col = lv + 1
                    metric = metric_series.get(lv)
                    text = "-" if metric is None else f"{metric:.2f}"
                    self.upgrade_table.setItem(row_idx, col, self._centered_table_item(text))
            self.compare_resolution_error = None
        except Exception as exc:
            self.compare_resolution_error = str(exc)
            self.upgrade_table.setRowCount(1)
            self.upgrade_table.setItem(
                0,
                0,
                self._centered_table_item(
                    f"Selected: {selected_best.weapon_name} | {selected_best.affinity} | "
                    f"AoW {selected_best.aow_name or '-'} | Comparison failed"
                ),
            )
            for lv in range(0, max_upgrade + 1):
                self.upgrade_table.setItem(0, lv + 1, self._centered_table_item("-"))
            self._set_idle_progress(f"Compare failed: {exc}")

        self.active_compare_selected = selected_best
        self.active_compare_target = compare_summary_row
        self._refresh_path_button_state()
        self._refresh_affinity_watch_button_state()
        self._refresh_compare_summary(selected_best, compare_summary_row, compare_weapon)

    def _locked_metric_series_for_config(
        self,
        row_data: desktop_models.SolvedBuild | None,
        max_upgrade: int,
    ) -> dict[int, float]:
        if row_data is None:
            return {}
        self._sync_session_state()
        return self.desktop_service.build_upgrade_series(self.session, row_data, max_upgrade)

    def _result_series_value(self, row: Any) -> float:
        return self._row_config_from_result(row).metric_for_objective(
            self.objective_combo.currentData()
        )

    def _best_row_config(
        self,
        weapon_name: str,
        affinity: str | None,
        aow_name: Any,
    ) -> desktop_models.SolvedBuild | None:
        self._sync_session_state()
        return self.desktop_service.solve_build(self.session, weapon_name, affinity, aow_name)

    def _optimizer_context_key(self) -> tuple[Any, ...]:
        self._sync_session_state()
        return self.desktop_service.optimizer_context_key(self.session)

    def _row_config_from_result(self, result: Any) -> desktop_models.SolvedBuild:
        if isinstance(result, desktop_models.SolvedBuild):
            return result
        if isinstance(result, dict):
            return desktop_models.SolvedBuild(
                weapon_id=int(result.get("weapon_id", 0)),
                weapon_name=str(result["weapon_name"]),
                affinity=str(result["affinity"]),
                aow_name=result.get("aow_name"),
                upgrade=int(result.get("best_upgrade", result.get("upgrade", 0))),
                str_stat=int(result["str_stat"]),
                dex=int(result["dex"]),
                int_stat=int(result["int_stat"]),
                fai=int(result["fai"]),
                arc=int(result["arc"]),
                ar_total=float(result.get("best_ar_total", result.get("ar_total", 0.0))),
                ar_physical=float(result.get("ar_physical", 0.0)),
                ar_magic=float(result.get("ar_magic", 0.0)),
                ar_fire=float(result.get("ar_fire", 0.0)),
                ar_lightning=float(result.get("ar_lightning", 0.0)),
                ar_holy=float(result.get("ar_holy", 0.0)),
                score=float(result.get("score", 0.0)),
                bleed_buildup=float(result.get("bleed_buildup", 0.0)),
                bleed_buildup_add=float(result.get("bleed_buildup_add", 0.0)),
                frost_buildup=float(result.get("frost_buildup", 0.0)),
                poison_buildup=float(result.get("poison_buildup", 0.0)),
                scarlet_rot_buildup=float(result.get("scarlet_rot_buildup", 0.0)),
                aow_first_hit_damage=float(result.get("aow_first_hit_damage", 0.0)),
                aow_full_sequence_damage=float(result.get("aow_full_sequence_damage", 0.0)),
            )
        return self.desktop_service.normalize_result(result)

    def _format_best_stats(self, row_data: desktop_models.SolvedBuild) -> str:
        return (
            f"Best +{row_data.upgrade} "
            f"STR {row_data.str_stat} DEX {row_data.dex} "
            f"INT {row_data.int_stat} FAI {row_data.fai} ARC {row_data.arc} "
            f"AR {row_data.ar_total:.2f} BLEED {row_data.bleed_buildup:.2f} "
            f"1ST {row_data.aow_first_hit_damage:.2f} FULL {row_data.aow_full_sequence_damage:.2f}"
        )

    def _result_metrics_text(self, result: Any) -> str:
        return (
            f"AR {float(result.ar_total):.2f}   "
            f"Split {self._damage_split_text(result)}   "
            f"Bleed {float(result.bleed_buildup):.2f}   "
            f"1st {float(result.aow_first_hit_damage):.2f}   "
            f"Full {float(result.aow_full_sequence_damage):.2f}   "
            f"Score {float(result.score):.2f}"
        )

    def _damage_split_text(self, result: Any) -> str:
        return (
            f"P {float(getattr(result, 'ar_physical', 0.0)):.2f} | "
            f"M {float(getattr(result, 'ar_magic', 0.0)):.2f} | "
            f"F {float(getattr(result, 'ar_fire', 0.0)):.2f} | "
            f"L {float(getattr(result, 'ar_lightning', 0.0)):.2f} | "
            f"H {float(getattr(result, 'ar_holy', 0.0)):.2f}"
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

    def _objective_metric_label(self) -> str:
        objective = self.objective_combo.currentData()
        if objective == "aow_first_hit":
            return "AoW First Hit (PvE)"
        if objective == "aow_full_sequence":
            return "AoW Full Sequence (PvE)"
        if objective == "max_ar_plus_bleed":
            return "AR + Bleed"
        return "AR"

    def _affinity_watch_affinities(
        self,
        weapon_name: str,
        aow_name: str | None,
        preferred_affinity: str | None,
    ) -> list[str]:
        solved = desktop_models.SolvedBuild(
            weapon_id=0,
            weapon_name=weapon_name,
            affinity=preferred_affinity or "Standard",
            aow_name=aow_name,
            upgrade=self.max_upgrade_spin.value(),
            str_stat=0,
            dex=0,
            int_stat=0,
            fai=0,
            arc=0,
            ar_total=0.0,
            ar_physical=0.0,
            ar_magic=0.0,
            ar_fire=0.0,
            ar_lightning=0.0,
            ar_holy=0.0,
            score=0.0,
            bleed_buildup=0.0,
            bleed_buildup_add=0.0,
            frost_buildup=0.0,
            poison_buildup=0.0,
            scarlet_rot_buildup=0.0,
            aow_first_hit_damage=0.0,
            aow_full_sequence_damage=0.0,
        )
        return self.desktop_service.affinity_watch_affinities(solved)

    def _build_affinity_watch_data(
        self,
        row_data: desktop_models.SolvedBuild | dict[str, Any],
        levels_ahead: int,
    ) -> tuple[list[AffinityWatchLine], list[AffinityBreakpoint]]:
        row = self._row_config_from_result(row_data)
        self._sync_session_state()
        payload = self.desktop_service.build_affinity_watch(self.session, row, levels_ahead)
        return list(payload.lines), list(payload.breakpoints)

    def _open_affinity_watch_dialog(self) -> None:
        if self.active_compare_selected is None:
            QtWidgets.QMessageBox.information(
                self,
                "Affinity Watcher",
                "Pick a result row first.",
            )
            return
        if self.affinity_watch_thread is not None:
            return

        requested_horizon = self.level_path_horizon_spin.value()
        levels_ahead = min(requested_horizon, self._remaining_path_levels())
        if levels_ahead <= 0:
            QtWidgets.QMessageBox.information(
                self,
                "Affinity Watcher",
                "Combat stats are already capped. There is no forward horizon to inspect.",
            )
            return

        row_data = self.active_compare_selected
        weapon_name = row_data.weapon_name
        affinities = self._affinity_watch_affinities(
            weapon_name,
            row_data.aow_name,
            row_data.affinity,
        )
        if not affinities:
            QtWidgets.QMessageBox.information(
                self,
                "Affinity Watcher",
                "No legal affinities are available for the selected weapon setup.",
            )
            return

        self.main_tabs.setCurrentIndex(3)
        self._sync_session_state()
        self.affinity_watch_context = {
            "weapon_name": weapon_name,
            "aow_name": row_data.aow_name,
            "upgrade": int(row_data.upgrade),
            "start_level": self._derived_level(),
            "levels_ahead": levels_ahead,
            "metric_label": self._objective_metric_label(),
            "selected_affinity": row_data.affinity,
        }
        self.affinity_watch_cancel_requested = False
        total = max(len(affinities) * (levels_ahead + 1), 1)
        self.affinity_progress_bar.setRange(0, total)
        self.affinity_progress_bar.setValue(0)
        self.affinity_progress_label.setText("Tracing affinity crossover lines...")
        self._refresh_affinity_watch_button_state()

        worker = AffinityWatchWorker(
            service=self.desktop_service,
            session=self.session,
            solved=row_data,
            levels_ahead=levels_ahead,
        )
        worker.progress.connect(self._on_affinity_watch_progress)
        worker.finished.connect(self._on_affinity_watch_finished)
        worker.failed.connect(self._on_affinity_watch_failed)
        worker.finished.connect(self._teardown_affinity_watch_worker)
        worker.failed.connect(self._teardown_affinity_watch_worker)
        self._launch_worker_thread("affinity_watch_thread", "affinity_watch_worker", worker)
        self._refresh_affinity_watch_button_state()

    @QtCore.pyqtSlot(object, object, object, object)
    def _on_affinity_watch_progress(
        self,
        processed: object,
        total: object,
        affinity: object,
        level: object,
    ) -> None:
        current = int(processed)
        maximum = int(total)
        self.affinity_progress_bar.setMaximum(max(maximum, 1))
        self.affinity_progress_bar.setValue(min(current, max(maximum, 1)))
        self.affinity_progress_label.setText(
            f"Tracing {affinity} at level {int(level)} ({current:,}/{maximum:,})..."
        )

    @QtCore.pyqtSlot(object, object)
    def _on_affinity_watch_finished(self, lines: object, breakpoints: object) -> None:
        context = self.affinity_watch_context
        if context is None:
            return
        self.affinity_watch_cancel_requested = False
        typed_lines = list(lines)
        typed_breakpoints = list(breakpoints)
        self.affinity_chart_widget.set_payload(typed_lines, str(context["metric_label"]))
        self._populate_affinity_tables(
            typed_lines,
            typed_breakpoints,
            int(context["start_level"]),
            int(context["levels_ahead"]),
        )
        self._set_affinity_progress_idle(
            f"Ready: {len(typed_lines)} affinity lines across +{int(context['levels_ahead'])}."
        )
        self.affinity_watch_signature = self._affinity_workspace_signature(self.active_compare_selected)

    @QtCore.pyqtSlot(str)
    def _on_affinity_watch_failed(self, message: str) -> None:
        was_cancelled = message == "cancelled"
        self.affinity_watch_cancel_requested = False
        if was_cancelled:
            self._set_affinity_progress_idle("Affinity watch stopped.")
            return
        self._set_affinity_progress_idle(f"Failed: {message}")
        QtWidgets.QMessageBox.warning(self, "Affinity Watcher", f"Failed to build watcher: {message}")

    @QtCore.pyqtSlot()
    def _teardown_affinity_watch_worker(self) -> None:
        self._teardown_named_worker("affinity_watch_thread", "affinity_watch_worker")
        self.affinity_watch_context = None
        self.affinity_watch_cancel_requested = False
        self._refresh_affinity_watch_button_state()

    @staticmethod
    def _path_config_from_row(title: str, row_data: desktop_models.SolvedBuild) -> PathWeaponConfig:
        return desktop_models.PathWeaponConfig(
            title=title,
            solved=row_data,
            start_state=row_data.combat_state,
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

        if self.path_thread is not None:
            return

        self.main_tabs.setCurrentIndex(2)
        self.path_cancel_requested = False
        self.path_progress_bar.setRange(0, max(len(configs), 1))
        self.path_progress_bar.setValue(0)
        self.path_progress_label.setText("Tracing level paths...")
        worker = PathPreviewWorker(
            service=self.desktop_service,
            session=self._current_session(),
            configs=configs,
            levels_ahead=levels_ahead,
        )
        worker.progress.connect(self._on_path_progress)
        worker.finished.connect(self._on_path_finished)
        worker.failed.connect(self._on_path_failed)
        worker.finished.connect(self._teardown_path_worker)
        worker.failed.connect(self._teardown_path_worker)
        self._launch_worker_thread("path_thread", "path_worker", worker)
        self._refresh_path_button_state()

    @QtCore.pyqtSlot(object, object, object, object)
    def _on_path_progress(
        self,
        processed: object,
        total: object,
        title: object,
        level: object,
    ) -> None:
        current = int(processed)
        maximum = int(total)
        self.path_progress_bar.setMaximum(max(maximum, 1))
        self.path_progress_bar.setValue(min(current, max(maximum, 1)))
        self.path_progress_label.setText(
            f"Tracing {str(title).lower()} path through level {int(level)} ({current}/{maximum})..."
        )

    @QtCore.pyqtSlot(object, object)
    def _on_path_finished(self, previews: object, levels_ahead: object) -> None:
        typed_previews = list(previews)
        self.path_chart_widget.set_previews(typed_previews)
        self._populate_path_panels(typed_previews)
        self._set_path_progress_idle(
            f"Ready: {len(typed_previews)} path lanes across +{int(levels_ahead)}."
        )
        self.path_preview_signature = self._path_workspace_signature(
            self.active_compare_selected,
            self.active_compare_target,
        )

    @QtCore.pyqtSlot(str)
    def _on_path_failed(self, message: str) -> None:
        was_cancelled = message == "cancelled"
        self.path_cancel_requested = False
        if was_cancelled:
            self._set_path_progress_idle("Path preview stopped.")
            return
        self._set_path_progress_idle(f"Failed: {message}")
        QtWidgets.QMessageBox.warning(self, "Path Graphs", f"Failed to build paths: {message}")

    @QtCore.pyqtSlot()
    def _teardown_path_worker(self) -> None:
        self._teardown_named_worker("path_thread", "path_worker")
        self.path_cancel_requested = False
        self._refresh_path_button_state()

    @staticmethod
    def _combat_state_from_row(row_data: desktop_models.SolvedBuild) -> CombatState:
        return row_data.combat_state

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

        effective_str = self._effective_str_for_weapon(
            config.weapon_name,
            config.affinity,
            state.str_stat,
        )
        return (
            max(req_str - effective_str, 0)
            + max(req_dex - state.dex, 0)
            + max(req_int - state.int_stat, 0)
            + max(req_fai - state.fai, 0)
            + max(req_arc - state.arc, 0)
        )

    def _effective_str_for_weapon(
        self,
        weapon_name: str | None,
        affinity: str | None,
        str_stat: int,
    ) -> int:
        if not self.two_handing_check.isChecked() or weapon_name is None:
            return str_stat
        try:
            disable_bonus = self.data.weapon_disables_two_hand_bonus(weapon_name, affinity)
        except Exception:
            disable_bonus = False
        if disable_bonus:
            return str_stat
        return min(99, int(str_stat * 1.5))

    def _refresh_compare_summary(
        self,
        selected_best: desktop_models.SolvedBuild | None,
        compare_best: desktop_models.SolvedBuild | None,
        compare_weapon: str | None,
    ) -> None:
        selected_fallback = "Pick a result row to inspect its optimized line."
        if self.compare_resolution_error is not None and selected_best is None:
            selected_fallback = f"Failed to resolve selected line: {self.compare_resolution_error}"
        self._set_compare_panel(
            self.selected_compare_panel,
            "Selected Build",
            selected_best,
            selected_fallback,
        )
        if self.compare_resolution_error is not None:
            fallback = f"Comparison failed: {self.compare_resolution_error}"
        elif compare_best is not None:
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
        self._refresh_path_workspace(selected_best, compare_best)
        self._refresh_affinity_workspace(selected_best)

    def _refresh_path_workspace(
        self,
        selected_best: desktop_models.SolvedBuild | None,
        compare_best: desktop_models.SolvedBuild | None,
    ) -> None:
        if selected_best is None:
            self.path_workspace_summary.setText("No selected path lane yet.")
            self.path_workspace_detail.setText(
                "Run a search and pick a selected result before opening path analysis."
            )
            self.path_preview_signature = None
            self.path_chart_widget.set_previews([])
            self._populate_path_panels([])
            self._set_path_progress_idle("Idle")
            self._refresh_path_button_state()
            return
        signature = self._path_workspace_signature(selected_best, compare_best)
        if self.path_thread is None and signature != self.path_preview_signature:
            self.path_preview_signature = None
            self.path_chart_widget.set_previews([])
            self._populate_path_panels([])
            self._set_path_progress_idle("Idle")
        self.path_workspace_summary.setText(
            f"Selected: {selected_best.weapon_name} | {selected_best.affinity} | "
            f"AoW {selected_best.aow_name or '-'} | +{selected_best.upgrade}"
        )
        if compare_best is None:
            self.path_workspace_detail.setText(
                "Pick a comparison target to trace both optimized Current + N routes."
            )
            self.path_preview_signature = None
            self.path_chart_widget.set_previews([])
            self._populate_path_panels([])
            self._set_path_progress_idle("Idle")
            self._refresh_path_button_state()
            return
        self.path_workspace_detail.setText(
            f"Compare: {compare_best.weapon_name} | {compare_best.affinity} | "
            f"AoW {compare_best.aow_name or '-'} | Horizon +{self.level_path_horizon_spin.value()}"
        )
        self._refresh_path_button_state()

    def _refresh_affinity_workspace(
        self,
        selected_best: desktop_models.SolvedBuild | None,
    ) -> None:
        if selected_best is None:
            self.affinity_workspace_summary.setText("No selected affinity lane yet.")
            self.affinity_workspace_detail.setText(
                "Run a search and pick a selected result row to analyze legal affinity crossovers."
            )
            self.affinity_watch_signature = None
            self.affinity_chart_widget.set_payload([], self._objective_metric_label())
            self._populate_affinity_tables([], [], self._derived_level(), self.level_path_horizon_spin.value())
            self._set_affinity_progress_idle("Idle")
            self._refresh_affinity_watch_button_state()
            return
        signature = self._affinity_workspace_signature(selected_best)
        if self.affinity_watch_thread is None and signature != self.affinity_watch_signature:
            self.affinity_watch_signature = None
            self.affinity_chart_widget.set_payload([], self._objective_metric_label())
            self._populate_affinity_tables([], [], self._derived_level(), self.level_path_horizon_spin.value())
            self._set_affinity_progress_idle("Idle")
        legal_affinities = self.desktop_service.affinity_watch_affinities(selected_best)
        self.affinity_workspace_summary.setText(
            f"{selected_best.weapon_name} | AoW {selected_best.aow_name or '-'} | +{selected_best.upgrade}"
        )
        self.affinity_workspace_detail.setText(
            f"{len(legal_affinities)} legal affinities across Current +{self.level_path_horizon_spin.value()}."
        )
        self._refresh_affinity_watch_button_state()

    def _refresh_analysis_workspace_labels(self) -> None:
        self._refresh_path_workspace(self.active_compare_selected, self.active_compare_target)
        self._refresh_affinity_workspace(self.active_compare_selected)

    def _path_workspace_signature(
        self,
        selected_best: desktop_models.SolvedBuild | None,
        compare_best: desktop_models.SolvedBuild | None,
    ) -> tuple[Any, ...] | None:
        if selected_best is None or compare_best is None:
            return None
        return (
            selected_best.fingerprint,
            compare_best.fingerprint,
            self.objective_combo.currentData(),
            self._derived_level(),
            self.level_path_horizon_spin.value(),
        )

    def _affinity_workspace_signature(
        self,
        selected_best: desktop_models.SolvedBuild | None,
    ) -> tuple[Any, ...] | None:
        if selected_best is None:
            return None
        return (
            selected_best.fingerprint,
            self.objective_combo.currentData(),
            self._derived_level(),
            self.level_path_horizon_spin.value(),
        )

    def _set_path_progress_idle(self, text: str) -> None:
        self.path_progress_label.setText(text)
        self.path_progress_bar.setRange(0, 1)
        self.path_progress_bar.setValue(0)

    def _set_affinity_progress_idle(self, text: str) -> None:
        self.affinity_progress_label.setText(text)
        self.affinity_progress_bar.setRange(0, 1)
        self.affinity_progress_bar.setValue(0)
        self._refresh_affinity_watch_button_state()

    def _clear_splitter(self, splitter: QtWidgets.QSplitter) -> None:
        while splitter.count():
            widget = splitter.widget(0)
            splitter.widget(0).setParent(None)
            if widget is not None:
                widget.deleteLater()

    def _build_path_table_panel(self, preview: PathPreview) -> QtWidgets.QWidget:
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
        last_metric: float | None = None
        for row_idx, step in enumerate(preview.steps):
            gain_text = "--"
            if step.ar is not None and last_metric is not None:
                gain_text = f"{step.ar - last_metric:+.2f}"
            metric_text = "-" if step.ar is None else f"{step.ar:.2f}"
            added_text = step.added_stat.upper() if step.added_stat is not None else "START"
            if step.ar is None and step.requirement_gap > 0:
                added_text = f"{added_text} (gap {step.requirement_gap})"
            values = [str(step.level), metric_text, gain_text, added_text, step.stats.summary()]
            for col_idx, value in enumerate(values):
                table.setItem(row_idx, col_idx, self._centered_table_item(value))
            if step.ar is not None:
                last_metric = step.ar
        layout.addWidget(table, 1)
        return shell

    def _populate_path_panels(self, previews: list[PathPreview]) -> None:
        self._clear_splitter(self.path_tables_splitter)
        if not previews:
            placeholder = QtWidgets.QLabel("No path previews loaded.")
            placeholder.setProperty("role", "summaryBody")
            placeholder.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
            self.path_tables_splitter.addWidget(placeholder)
            return
        for preview in previews:
            self.path_tables_splitter.addWidget(self._build_path_table_panel(preview))

    def _populate_affinity_tables(
        self,
        lines: list[AffinityWatchLine],
        breakpoints: list[AffinityBreakpoint],
        start_level: int,
        levels_ahead: int,
    ) -> None:
        self.affinity_summary_table.setRowCount(len(lines))
        self.affinity_summary_table.setHorizontalHeaderLabels(
            ["Affinity", f"Lv {start_level}", f"Lv {start_level + levels_ahead}", "Final Stats"]
        )
        for row_idx, line in enumerate(lines):
            final_stats = "--"
            if getattr(line, "final_build", None) is not None:
                final_stats = line.final_build.combat_state.summary()
            values = [
                line.affinity,
                "--" if line.start_metric is None else f"{line.start_metric:.2f}",
                "--" if line.end_metric is None else f"{line.end_metric:.2f}",
                final_stats,
            ]
            for col_idx, value in enumerate(values):
                self.affinity_summary_table.setItem(row_idx, col_idx, self._centered_table_item(value))
        self.affinity_breakpoint_table.setRowCount(len(breakpoints))
        for row_idx, breakpoint in enumerate(breakpoints):
            values = [
                str(breakpoint.level),
                breakpoint.outgoing_affinity,
                breakpoint.incoming_affinity,
                "--" if breakpoint.outgoing_metric is None else f"{breakpoint.outgoing_metric:.2f}",
                "--" if breakpoint.incoming_metric is None else f"{breakpoint.incoming_metric:.2f}",
            ]
            for col_idx, value in enumerate(values):
                self.affinity_breakpoint_table.setItem(
                    row_idx,
                    col_idx,
                    self._centered_table_item(value),
                )

    def _set_compare_panel(
        self,
        panel: dict[str, Any],
        heading: str,
        row_data: desktop_models.SolvedBuild | None,
        fallback: str,
    ) -> None:
        panel["heading"].setText(heading.upper())
        if row_data is None:
            panel["title"].setText("Waiting on a valid line")
            panel["body"].setText(fallback)
            panel["stats"].setText("STR --  DEX --  INT --  FAI --  ARC --")
            panel["metrics"].setText("Best +--   AR --   Bleed --   1st --   Full --")
            return
        panel["title"].setText(f"{row_data.weapon_name} | {row_data.affinity}")
        panel["body"].setText(f"AoW {row_data.aow_name or '-'}")
        panel["stats"].setText(
            f"STR {row_data.str_stat}  DEX {row_data.dex}  INT {row_data.int_stat}  "
            f"FAI {row_data.fai}  ARC {row_data.arc}"
        )
        panel["metrics"].setText(
            f"Best +{row_data.upgrade}   AR {row_data.ar_total:.2f}   "
            f"Split {self._damage_split_text(row_data)}   "
            f"Bleed {row_data.bleed_buildup:.2f}   "
            f"1st {row_data.aow_first_hit_damage:.2f}   Full {row_data.aow_full_sequence_damage:.2f}"
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

        effective_str = self._effective_str_for_weapon(
            selected_weapon,
            selected_affinity,
            self.str_spin.value(),
        )

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
        self._refresh_search_button_state()
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
            if self._combo_has_open_option(combo):
                combo.setCurrentIndex(0)
                return
            current_idx = combo.currentIndex()
            if current_idx >= 0:
                combo.setEditText(combo.itemText(current_idx))
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
                if MainWindow._combo_has_open_option(combo):
                    combo.setCurrentIndex(0)
                    return combo.itemData(0)
                return combo.currentData()
            idx = MainWindow._find_index_by_text(combo, text)
            if idx >= 0:
                if idx != combo.currentIndex():
                    combo.setCurrentIndex(idx)
                return combo.itemData(idx)
            if MainWindow._combo_has_open_option(combo):
                combo.setCurrentIndex(0)
                return combo.itemData(0)
            current_idx = combo.currentIndex()
            if current_idx >= 0:
                combo.setEditText(combo.itemText(current_idx))
            return combo.currentData()
        return combo.currentData()

    @staticmethod
    def _combo_has_open_option(combo: QtWidgets.QComboBox) -> bool:
        return combo.count() > 0 and combo.itemData(0) is None

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
    desktop_theme.apply_dark_theme(app)


def main() -> int:
    app = QtWidgets.QApplication(sys.argv)
    apply_dark_theme(app)
    window = MainWindow()
    window.show()
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
