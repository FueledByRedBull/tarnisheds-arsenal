from __future__ import annotations

from dataclasses import dataclass

from PyQt6 import QtCore, QtWidgets

import theme as desktop_theme


@dataclass
class UiState:
    active_workspace: str = "rankings"
    advanced_open: bool = False
    inspector_mode: str = "empty"
    busy_search: bool = False
    busy_paths: bool = False
    busy_affinity: bool = False


class AdvancedDrawer(QtWidgets.QFrame):
    toggled = QtCore.pyqtSignal(bool)

    def __init__(self, title: str = "Advanced", parent: QtWidgets.QWidget | None = None) -> None:
        super().__init__(parent)
        self.setProperty("role", "advancedDrawer")
        self.setObjectName("AdvancedDrawer")
        self._open = False

        layout = QtWidgets.QVBoxLayout(self)
        layout.setContentsMargins(10, 10, 10, 10)
        layout.setSpacing(8)

        self.toggle_button = QtWidgets.QPushButton(title.upper())
        self.toggle_button.setObjectName("AdvancedToggle")
        self.toggle_button.setProperty("role", "inlineButton")
        self.toggle_button.setIcon(desktop_theme.icon("warning", desktop_theme.THEME["accent"]))
        self.toggle_button.clicked.connect(self._toggle)
        layout.addWidget(self.toggle_button)

        self.body = QtWidgets.QWidget()
        self.body.setObjectName("AdvancedDrawerBody")
        self.body_layout = QtWidgets.QVBoxLayout(self.body)
        self.body_layout.setContentsMargins(0, 0, 0, 0)
        self.body_layout.setSpacing(8)
        layout.addWidget(self.body)
        self.set_open(False)

    def set_open(self, open_: bool) -> None:
        self._open = open_
        self.body.setVisible(open_)
        self.toggle_button.setText("ADVANCED - OPEN" if open_ else "ADVANCED")
        self.toggled.emit(open_)

    def is_open(self) -> bool:
        return self._open

    def _toggle(self) -> None:
        self.set_open(not self._open)


def band_frame(object_name: str | None = None) -> QtWidgets.QFrame:
    frame = QtWidgets.QFrame()
    frame.setProperty("role", "band")
    if object_name is not None:
        frame.setObjectName(object_name)
    return frame


def inspector_block(title: str) -> tuple[QtWidgets.QFrame, QtWidgets.QVBoxLayout]:
    frame = QtWidgets.QFrame()
    frame.setProperty("role", "inspectorBlock")
    layout = QtWidgets.QVBoxLayout(frame)
    layout.setContentsMargins(12, 12, 12, 12)
    layout.setSpacing(7)
    heading = QtWidgets.QLabel(title.upper())
    heading.setProperty("role", "inspectorHeading")
    layout.addWidget(heading)
    return frame, layout


def text_label(text: str, role: str, word_wrap: bool = False) -> QtWidgets.QLabel:
    label = QtWidgets.QLabel(text)
    label.setProperty("role", role)
    label.setWordWrap(word_wrap)
    return label
