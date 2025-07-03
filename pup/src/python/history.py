import logging
from enum import Enum
from typing import Optional

import pup
from pup import (
    GlobalEventType,
    GlobalEvent,
    Session,
    InputLine,
    Event,
    EventType,
    KeyEvent,
    Shortcut,
    PythonShortcut,
    Tab,
    Input,
    EchoState,
)


class Direction(Enum):
    NEXT = "next"
    PREV = "prev"


class InputHistory:
    def __init__(self, sesh: Session, input: Input, max_lines: int = 1000):
        self.sesh = sesh
        self.input = input
        self.max_lines = max_lines
        self.lines: list[InputLine] = []
        self.cursor_pos: Optional[int] = None
        self.partial: Optional[InputLine] = None

    def __repr__(self) -> str:
        return f"InputHistory({self.sesh}, lines={len(self.lines)}, cursor_pos={self.cursor_pos})"

    def reset_cursor(self):
        self.cursor_pos = None

    async def sent_line(self, sesh: Session, ev: Event):
        if isinstance(ev, Event.InputLine) and sesh.id == self.sesh.id:
            self.add(ev.line)

    def add(self, line: InputLine):
        # Skip empty lines
        if not line.sent.strip() and (not line.original or not line.original.strip()):
            return

        # Reset cursor on new user input
        if self.cursor_pos is not None and not line.scripted:
            self.cursor_pos = None

        self.lines.append(line)

        # Trim old lines
        if len(self.lines) >= self.max_lines:
            self.lines.pop(0)

    async def _navigate_history(self, direction: str):
        if self.input.echo() == EchoState.Password:
            return

        if direction == Direction.PREV and self.cursor_pos is None:
            # Store current input as partial on first backward move
            self.partial = self.input.value()

        line = self.next() if direction == Direction.NEXT else self.prev()

        if line is None and direction == Direction.NEXT and self.partial is not None:
            # Restore partial input when no next available
            self.cursor_pos = None
            self.input.set_value(self.partial)
            return

        if line is not None:
            # Use original if available
            if line.original is not None:
                line = line.clone_with_original()
            self.input.set_value(line)

    async def shortcut_next(
        self, _key_event: KeyEvent, _sesh: Optional[Session], _tab: Tab
    ):
        await self._navigate_history(Direction.NEXT)

    async def shortcut_prev(
        self, _key_event: KeyEvent, _sesh: Optional[Session], _tab: Tab
    ):
        await self._navigate_history(Direction.PREV)

    @staticmethod
    def _should_skip_line(line: InputLine, skip_scripted: bool = True) -> bool:
        return (line.scripted and skip_scripted) or (
            not line.sent and line.original is None
        )

    def next(self, *, skip_scripted: bool = True) -> Optional[InputLine]:
        if self.cursor_pos is None:
            return None

        while self.cursor_pos < len(self.lines) - 1:
            self.cursor_pos += 1
            line = self.lines[self.cursor_pos]
            if not self._should_skip_line(line, skip_scripted):
                return line

        self.cursor_pos = None
        return None

    def prev(self, *, skip_scripted: bool = True) -> Optional[InputLine]:
        if not self.lines:
            return None

        if self.cursor_pos is None:
            self.cursor_pos = len(self.lines)

        while self.cursor_pos > 0:
            self.cursor_pos -= 1
            line = self.lines[self.cursor_pos]
            if not self._should_skip_line(line, skip_scripted):
                return line

        return None


async def create_input_history(ev: GlobalEvent):
    if not isinstance(ev, GlobalEvent.NewSession):
        return

    # Create a history instance for the session and the session's input area.
    sesh = ev.session
    h = InputHistory(sesh, await sesh.input())
    history[sesh.id] = h

    # Listen for session input being sent, add it to the history.
    sesh.add_event_handler(EventType.InputLine, h.sent_line)

    # Set up default down/up arrow key shortcuts for navigating input history.
    tab = await sesh.tab()
    tab.set_shortcut(KeyEvent("down"), Shortcut.Python(PythonShortcut(h.shortcut_next)))
    tab.set_shortcut(KeyEvent("up"), Shortcut.Python(PythonShortcut(h.shortcut_prev)))


logging.debug("module loaded")
history: dict[int, InputHistory] = {}
pup.add_global_event_handler(GlobalEventType.NewSession, create_input_history)
