from __future__ import annotations

from PyQt6 import QtCore, QtGui, QtWidgets


FONT_FAMILY = "Tahoma"

THEME = {
    "bg": "#070809",
    "rail": "#0d0f12",
    "panel": "#111419",
    "panel_alt": "#171b21",
    "panel_soft": "#1d2229",
    "input": "#0e1115",
    "border": "#303842",
    "border_soft": "#222932",
    "border_bright": "#c6a15a",
    "text": "#d8cfbd",
    "text_soft": "#9a927f",
    "text_bright": "#f3e4bd",
    "accent": "#c6a15a",
    "accent_deep": "#8f6725",
    "accent_dark": "#2a2114",
    "success_bg": "#142017",
    "success_border": "#7c9b58",
    "danger_bg": "#251114",
    "danger_border": "#c95d63",
    "info_bg": "#131b24",
    "info_border": "#57718a",
    "muted_bg": "#0b0d10",
    "row_alt": "#11161c",
}

ICON_KEYS = {
    "search",
    "lock",
    "compare",
    "path",
    "affinity",
    "warning",
    "stop",
    "rankings",
}


def icon(name: str, color: str | None = None, size: int = 18) -> QtGui.QIcon:
    if name not in ICON_KEYS:
        name = "search"
    stroke = QtGui.QColor(color or THEME["text_bright"])
    muted = QtGui.QColor(THEME["text_soft"])
    accent = QtGui.QColor(THEME["accent"])
    pixmap = QtGui.QPixmap(size, size)
    pixmap.fill(QtCore.Qt.GlobalColor.transparent)
    painter = QtGui.QPainter(pixmap)
    painter.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
    pen = QtGui.QPen(stroke, max(1.5, size / 11))
    pen.setCapStyle(QtCore.Qt.PenCapStyle.RoundCap)
    pen.setJoinStyle(QtCore.Qt.PenJoinStyle.RoundJoin)
    painter.setPen(pen)
    painter.setBrush(QtCore.Qt.BrushStyle.NoBrush)

    s = float(size)
    if name == "search":
        painter.drawEllipse(QtCore.QRectF(s * 0.17, s * 0.17, s * 0.48, s * 0.48))
        painter.drawLine(QtCore.QPointF(s * 0.58, s * 0.58), QtCore.QPointF(s * 0.82, s * 0.82))
    elif name == "lock":
        painter.drawRoundedRect(QtCore.QRectF(s * 0.22, s * 0.43, s * 0.56, s * 0.4), 2, 2)
        painter.drawArc(QtCore.QRectF(s * 0.32, s * 0.17, s * 0.36, s * 0.48), 0, 180 * 16)
    elif name == "compare":
        painter.drawLine(QtCore.QPointF(s * 0.22, s * 0.34), QtCore.QPointF(s * 0.78, s * 0.34))
        painter.drawLine(QtCore.QPointF(s * 0.22, s * 0.66), QtCore.QPointF(s * 0.78, s * 0.66))
        painter.drawLine(QtCore.QPointF(s * 0.64, s * 0.22), QtCore.QPointF(s * 0.78, s * 0.34))
        painter.drawLine(QtCore.QPointF(s * 0.64, s * 0.46), QtCore.QPointF(s * 0.78, s * 0.34))
        painter.drawLine(QtCore.QPointF(s * 0.36, s * 0.54), QtCore.QPointF(s * 0.22, s * 0.66))
        painter.drawLine(QtCore.QPointF(s * 0.36, s * 0.78), QtCore.QPointF(s * 0.22, s * 0.66))
    elif name == "path":
        painter.drawEllipse(QtCore.QRectF(s * 0.17, s * 0.62, s * 0.17, s * 0.17))
        painter.drawEllipse(QtCore.QRectF(s * 0.66, s * 0.18, s * 0.17, s * 0.17))
        path = QtGui.QPainterPath(QtCore.QPointF(s * 0.29, s * 0.68))
        path.cubicTo(s * 0.45, s * 0.48, s * 0.45, s * 0.34, s * 0.7, s * 0.27)
        painter.drawPath(path)
    elif name == "affinity":
        painter.setPen(QtGui.QPen(accent, max(1.5, size / 12)))
        painter.drawEllipse(QtCore.QRectF(s * 0.2, s * 0.2, s * 0.26, s * 0.26))
        painter.drawEllipse(QtCore.QRectF(s * 0.54, s * 0.2, s * 0.26, s * 0.26))
        painter.drawEllipse(QtCore.QRectF(s * 0.37, s * 0.58, s * 0.26, s * 0.26))
        painter.drawLine(QtCore.QPointF(s * 0.42, s * 0.42), QtCore.QPointF(s * 0.47, s * 0.62))
        painter.drawLine(QtCore.QPointF(s * 0.59, s * 0.42), QtCore.QPointF(s * 0.54, s * 0.62))
    elif name == "warning":
        painter.setPen(QtGui.QPen(QtGui.QColor(THEME["danger_border"]), max(1.5, size / 12)))
        points = [
            QtCore.QPointF(s * 0.5, s * 0.16),
            QtCore.QPointF(s * 0.84, s * 0.8),
            QtCore.QPointF(s * 0.16, s * 0.8),
        ]
        painter.drawPolygon(QtGui.QPolygonF(points))
        painter.drawLine(QtCore.QPointF(s * 0.5, s * 0.36), QtCore.QPointF(s * 0.5, s * 0.58))
        painter.drawPoint(QtCore.QPointF(s * 0.5, s * 0.68))
    elif name == "stop":
        painter.setBrush(QtGui.QColor(THEME["danger_border"]))
        painter.setPen(QtCore.Qt.PenStyle.NoPen)
        painter.drawRoundedRect(QtCore.QRectF(s * 0.25, s * 0.25, s * 0.5, s * 0.5), 2, 2)
    elif name == "rankings":
        painter.setPen(QtGui.QPen(muted, max(1.5, size / 12)))
        for idx, height in enumerate((0.5, 0.68, 0.35)):
            x = s * (0.22 + idx * 0.22)
            painter.drawLine(QtCore.QPointF(x, s * 0.8), QtCore.QPointF(x, s * (0.8 - height)))

    painter.end()
    return QtGui.QIcon(pixmap)


def apply_dark_theme(app: QtWidgets.QApplication) -> None:
    c = THEME
    app.setStyle("Fusion")
    app.setFont(QtGui.QFont(FONT_FAMILY, 9))
    app.setStyleSheet(
        f"""
        QWidget {{
            background: {c["bg"]};
            color: {c["text"]};
        }}
        QWidget#RootShell, QWidget#RightStage, QWidget#LeftRail, QWidget#InspectorPanel {{
            background: {c["bg"]};
        }}
        QWidget#CommandRail {{
            background: {c["rail"]};
            border: 1px solid {c["border_soft"]};
            border-radius: 6px;
        }}
        QWidget#WorkspacePanel {{
            background: {c["bg"]};
        }}
        QScrollArea {{
            background: transparent;
            border: none;
        }}
        QGroupBox {{
            border: 1px solid {c["border_soft"]};
            border-radius: 6px;
            margin-top: 12px;
            padding-top: 12px;
            background: {c["panel"]};
        }}
        QGroupBox#SearchGroup {{
            background: {c["panel_alt"]};
            border: 1px solid {c["border_bright"]};
        }}
        QGroupBox::title {{
            subcontrol-origin: margin;
            left: 10px;
            padding: 0 8px;
            color: {c["accent"]};
            font-weight: 700;
            letter-spacing: 1px;
        }}
        QFrame#HeroPanel {{
            background: {c["panel"]};
            border-left: 3px solid {c["border_bright"]};
            border-top: 1px solid {c["border_soft"]};
            border-right: 1px solid {c["border_soft"]};
            border-bottom: 1px solid {c["border_soft"]};
            border-radius: 4px;
        }}
        QFrame[role="band"], QFrame[role="advancedDrawer"] {{
            background: {c["panel"]};
            border: 1px solid {c["border_soft"]};
            border-radius: 6px;
        }}
        QFrame[role="metricCard"] {{
            background: {c["panel_alt"]};
            border: 1px solid {c["border_soft"]};
            border-radius: 4px;
        }}
        QFrame[role="summaryPanel"], QFrame[role="resultCard"], QFrame[role="inspectorBlock"] {{
            background: {c["panel_alt"]};
            border: 1px solid {c["border_soft"]};
            border-radius: 5px;
        }}
        QFrame[role="resultCard"][cardState="best"] {{
            border: 1px solid {c["border_bright"]};
            background: {c["accent_dark"]};
        }}
        QFrame[role="resultCard"][cardState="selected"] {{
            border: 1px solid {c["text_bright"]};
            background: {c["panel_soft"]};
        }}
        QFrame[role="resultCard"][cardState="empty"] {{
            background: {c["muted_bg"]};
        }}
        QLabel[role="brandTitle"] {{
            color: {c["text_bright"]};
            font-size: 18px;
            font-weight: 800;
            letter-spacing: 1px;
        }}
        QLabel[role="brandSub"], QLabel[role="sectionHint"], QLabel[role="cardDetail"], QLabel[role="summaryBody"], QLabel[role="statusLine"] {{
            color: {c["text_soft"]};
        }}
        QLabel[role="heroTitle"] {{
            color: {c["text_bright"]};
            font-size: 22px;
            font-weight: 800;
            letter-spacing: 1px;
        }}
        QLabel[role="heroSubtitle"] {{
            color: {c["text"]};
            font-size: 12px;
        }}
        QLabel[role="metricTitle"], QLabel[role="fieldLabel"], QLabel[role="gridHeader"], QLabel[role="summaryHeading"], QLabel[role="inspectorHeading"] {{
            color: {c["text_soft"]};
            font-size: 10px;
            font-weight: 800;
            letter-spacing: 1px;
        }}
        QLabel[role="metricValue"] {{
            color: {c["text_bright"]};
            font-size: 21px;
            font-weight: 800;
        }}
        QLabel[role="cardTitle"], QLabel[role="summaryTitle"], QLabel[role="inspectorTitle"] {{
            color: {c["text_bright"]};
            font-size: 14px;
            font-weight: 800;
        }}
        QLabel[role="cardStats"], QLabel[role="summaryStats"], QLabel[role="cardMetric"], QLabel[role="summaryMetric"], QLabel[role="inspectorMetric"] {{
            color: {c["text"]};
        }}
        QLabel[role="statName"] {{
            color: {c["text_bright"]};
            font-weight: 800;
        }}
        QLabel[role="statDash"] {{
            color: {c["text_soft"]};
        }}
        QLabel[role="chip"], QLabel[role="requirementBadge"] {{
            border: 1px solid {c["border"]};
            border-radius: 8px;
            padding: 3px 8px;
            font-size: 10px;
            font-weight: 800;
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
            border-color: {c["info_border"]};
            color: {c["text_bright"]};
        }}
        QLineEdit, QSpinBox, QComboBox, QTableWidget {{
            background: {c["input"]};
            border: 1px solid {c["border"]};
            border-radius: 4px;
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
        QComboBox QAbstractItemView, QTableWidget {{
            background: {c["input"]};
            color: {c["text"]};
            selection-background-color: {c["panel_soft"]};
            selection-color: {c["text_bright"]};
        }}
        QHeaderView::section, QTableCornerButton::section {{
            background: {c["panel_soft"]};
            color: {c["accent"]};
            border: 1px solid {c["border_soft"]};
            padding: 6px 8px;
            font-weight: 800;
            letter-spacing: 1px;
        }}
        QTabWidget::pane {{
            border: 1px solid {c["border_soft"]};
            border-radius: 6px;
            background: {c["panel"]};
            top: -1px;
        }}
        QTabBar::tab {{
            background: {c["panel_alt"]};
            color: {c["text_soft"]};
            border: 1px solid {c["border_soft"]};
            border-bottom: none;
            padding: 8px 14px;
            min-width: 130px;
            font-weight: 800;
            letter-spacing: 1px;
        }}
        QTabBar::tab:selected {{
            background: {c["panel_soft"]};
            color: {c["text_bright"]};
            border-color: {c["border_bright"]};
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
            background: {c["accent_deep"]};
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
            padding: 8px 12px;
            color: {c["text_bright"]};
            letter-spacing: 1px;
            font-weight: 800;
        }}
        QPushButton:hover {{
            background: {c["panel_soft"]};
            border-color: {c["text_bright"]};
        }}
        QPushButton[role="ctaButton"] {{
            padding: 10px 14px;
            font-size: 13px;
            background: {c["accent_dark"]};
        }}
        QPushButton[role="inlineButton"], QPushButton[role="navButton"] {{
            padding: 6px 9px;
            font-size: 10px;
        }}
        QPushButton[role="navButton"] {{
            text-align: left;
            border-color: {c["border_soft"]};
            background: {c["panel"]};
        }}
        QPushButton[role="navButton"][active="true"] {{
            border-color: {c["border_bright"]};
            background: {c["accent_dark"]};
            color: {c["text_bright"]};
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
            font-weight: 800;
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
            margin: 8px 0;
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
