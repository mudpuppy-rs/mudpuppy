import logging
from typing import Optional

from mudpuppy_core import (
    Event,
    EventType,
    InputLine,
    SessionInfo,
    Shortcut,
    mudpuppy_core,
    OutputItem,
)

from mudpuppy import on_event, on_new_session


class History:
    session: SessionInfo
    lines: list[InputLine]
    max_lines: int
    cursor_pos: Optional[int] = None
    # When input matched an alias, should the history be populated with the _input_ to the alias (default) or what
    # was sent to the game when the alias was expanded?
    use_alias_expanded: bool = False

    def __init__(self, session: SessionInfo, max_lines: int = 1000):
        self.session = session
        self.max_lines = max_lines
        self.lines = []
        logging.debug(f"history constructed for conn: {self.session}")

    def __repr__(self) -> str:
        return f"History({self.session.id}, lines={len(self.lines)} max_lines={self.max_lines} cursor_pos={self.cursor_pos})"

    def reset_cursor(self):
        self.cursor_pos = None

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
            f"{self} added line: {line} (original={line.original} scripted={line.scripted})"
        )
        while len(self.lines) >= self.max_lines:
            removed = self.lines.pop(0)
            logging.debug(f"{self} dropped old line: {removed}")

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
            skip = line.scripted and skip_scripted
            logging.debug(f"{self} pos updated - line={line}, skip={skip}")
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
            skip = line.scripted and skip_scripted
            logging.debug(f"{self} pos updated - line={line}, skip={skip}")
            if skip:
                continue
            return line

        logging.debug(f"{self} finished prev() without finding input to use")
        return None

    async def debug(self):
        output = []
        for idx, line in enumerate(self.lines):
            selected = ""
            if idx == self.cursor_pos:
                selected = "* "
            output.append(
                OutputItem.debug(f"{selected}{idx}: {line} (scripted={line.scripted})")
            )

        logging.debug(f"{self} adding debug output")
        await mudpuppy_core.add_outputs(self.session.id, output)


def get_history(session_id: int) -> Optional[History]:
    return history.get(session_id)


@on_event(EventType.InputLine)
async def input_sent(event: Event):
    assert isinstance(event, Event.InputLine)
    if history.get(event.id) is None:
        return
    history[event.id].add(event.input)


@on_event(EventType.Shortcut)
async def shortcut(event: Event):
    assert isinstance(event, Event.Shortcut)

    h = history[event.id]
    if event.shortcut == Shortcut.HistoryNext:
        line = h.next()
    elif event.shortcut == Shortcut.HistoryPrevious:
        line = h.prev()
    else:
        return

    if line is None:
        await mudpuppy_core.set_input(event.id, "")
    else:
        logging.info(
            f"history: populating {event.id} input with: {line} (original={line.original} scripted={line.scripted})"
        )
        value = line.sent
        if line.original is not None and not h.use_alias_expanded:
            value = line.original

        await mudpuppy_core.set_input(event.id, value)


@on_new_session()
async def setup(event: Event):
    assert isinstance(event, Event.NewSession)
    sesh_info = await mudpuppy_core.session_info(event.id)
    history[event.id] = History(sesh_info)


history: dict[int, History] = {}
logging.debug("history module loaded")
