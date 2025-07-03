import logging
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


class InputHistory:
    sesh: Session
    input: Input
    lines: list[InputLine]
    max_lines: int
    cursor_pos: Optional[int] = None
    partial: Optional[InputLine] = None

    def __init__(self, sesh: Session, input: Input, max_lines: int = 1000):
        self.sesh = sesh
        self.input = input
        self.max_lines = max_lines
        self.lines = []

    def __repr__(self) -> str:
        return f"InputHistory({self.sesh}, lines={len(self.lines)} max_lines={self.max_lines} cursor_pos={self.cursor_pos} partial={self.partial})"

    def reset_cursor(self):
        self.cursor_pos = None

    async def sent_line(self, sesh: Session, ev: Event):
        assert isinstance(ev, Event.InputLine)

        if sesh.id != self.sesh.id:
            return

        self.add(ev.line)

    def add(self, line: InputLine):
        if (
            line.sent.strip() == ""
            and line.original is not None
            and line.original.strip() == ""
        ):
            logging.warn(f"{self} ignoring empty line")
            return

        if self.cursor_pos is not None and not line.scripted:
            logging.debug(f"{self} resetting cursor pos due to new user input")
            self.cursor_pos = None

        self.lines.append(line)
        logging.debug(
            f"{self} added line: {line} (original={line.original} scripted={line.scripted} echo={line.echo})"
        )
        while len(self.lines) >= self.max_lines:
            removed = self.lines.pop(0)
            logging.debug(f"{self} dropped old line: {removed}")

    async def shortcut_next(
        self, _key_event: KeyEvent, _sesh: Optional[Session], _tab: Tab
    ):
        if self.input.echo() == EchoState.Password:
            logging.debug(f"{self}: ignoring history navigation in password echo state")
            return
        line = self.next()
        if line is None and self.partial is not None:
            self.cursor_pos = None
            logging.debug(
                f"{self}: no next - restoring partial input ({str(self.partial)})"
            )
            self.input.set_value(self.partial)
            return
        elif line is None:
            logging.debug("history: no next/prev - clearing input")
            return

        logging.info(
            f"{self}: populating input with next {line} (original={line.original} scripted={line.scripted} echo={line.echo})"
        )
        if line.original is not None:
            line = line.clone_with_original()
        self.input.set_value(line)

    async def shortcut_prev(
        self, _key_event: KeyEvent, _sesh: Optional[Session], _tab: Tab
    ):
        if self.input.echo() == EchoState.Password:
            logging.debug(f"{self}: ignoring history navigation in password echo state")
            return
        if self.cursor_pos is None:
            current = self.input.value()
            logging.debug(
                f"{self}: first backward cursor move, storing current input as partial ({str(current)})"
            )
            self.partial = current
        line = self.prev()
        if line is None:
            logging.debug(f"{self}: no prev history - staying as-is")
            return
        logging.info(
            f"{self}: populating input with prev {line} (original={line.original} scripted={line.scripted} echo={line.echo})"
        )
        if line.original is not None:
            line = line.clone_with_original()
        self.input.set_value(line)

    def next(self, *, skip_scripted: bool = True) -> Optional[InputLine]:
        logging.debug(
            f"{self} moving next from {self.cursor_pos} skip_scripted={skip_scripted}"
        )
        # If we're not already scrolling back in history, we can't move forward.
        if self.cursor_pos is None:
            return None

        # Searching forwards for an appropriate line
        while self.cursor_pos < len(self.lines) - 1:
            self.cursor_pos += 1
            line = self.lines[self.cursor_pos]
            skip = (line.scripted and skip_scripted) or (
                line.sent == "" and line.original is None
            )
            logging.debug(
                f"{self} pos updated - line={line}, original={line.original}, echo={line.echo}, skip={skip}"
            )
            if skip:
                continue
            return line

        # Reset the cursor pos if we ran out of history in this direction.
        self.cursor_pos = None
        logging.debug(f"{self} finished next() without finding input to use")
        return None

    def prev(self, *, skip_scripted: bool = True) -> Optional[InputLine]:
        logging.debug(
            f"{self} moving prev from {self.cursor_pos} skip_scripted={skip_scripted}"
        )

        # No history to scroll through.
        if len(self.lines) == 0:
            logging.debug(f"{self} no history to move through.")
            return None

        # Starting out through scrolling history
        if self.cursor_pos is None:
            self.cursor_pos = len(self.lines)
            logging.debug(f"{self} set initial pos for new scroll")

        # Searching backwards for an appropriate line
        while self.cursor_pos > 0:
            self.cursor_pos -= 1
            line = self.lines[self.cursor_pos]
            skip = (line.scripted and skip_scripted) or (
                line.sent == "" and line.original is None
            )
            logging.debug(
                f"{self} pos updated - line={line}, original={line.original}, echo={line.echo}, skip={skip}"
            )
            if skip:
                continue
            return line

        logging.debug(f"{self} finished prev() without finding input to use")
        return None

    async def debug(self):
        for idx, line in enumerate(self.lines):
            selected = ""
            if idx == self.cursor_pos:
                selected = "* "
            print(f"{selected}{idx}: {line} (scripted={line.scripted})")


async def create_input_history(ev: GlobalEvent):
    assert isinstance(ev, GlobalEvent.NewSession)

    sesh = ev.session
    logging.info(f"input history: initializing for {sesh}")

    h = InputHistory(sesh, await sesh.input())
    history[sesh.id] = h

    sesh.add_event_handler(EventType.InputLine, h.sent_line)

    tab = await sesh.tab()
    tab.set_shortcut(KeyEvent("down"), Shortcut.Python(PythonShortcut(h.shortcut_next)))
    tab.set_shortcut(KeyEvent("up"), Shortcut.Python(PythonShortcut(h.shortcut_prev)))


history: dict[int, InputHistory] = {}

logging.debug("module loaded")
pup.add_global_event_handler(GlobalEventType.NewSession, create_input_history)
