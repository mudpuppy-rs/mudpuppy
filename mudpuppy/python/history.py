import logging
from typing import Optional

from mudpuppy_core import (
    Event,
    EventType,
    InputLine,
    SessionInfo,
    Shortcut,
    mudpuppy_core,
)

from mudpuppy import on_event, on_new_session


class History:
    session: SessionInfo
    lines: list[InputLine]
    max_lines: int
    cursor_pos: Optional[int] = None

    def __init__(self, session: SessionInfo, max_lines: int = 1000):
        self.session = session
        self.max_lines = max_lines
        self.lines = []
        logging.debug(f"history constructed for conn: {self.session}")

    def reset_cursor(self):
        self.cursor_pos = None

    def add(self, line: InputLine):
        if line.sent.strip() == "":
            return

        if self.cursor_pos is not None:
            self.cursor_pos = None

        logging.debug(f"[{self.session}] history: adding line: {line}")
        self.lines.append(line)
        while len(self.lines) >= self.max_lines:
            logging.debug(f"[{self.session}] history: removing line: {self.lines[0]}")
            self.lines.pop(0)
        logging.debug(f"[{self.session}] history: {len(self.lines)} lines in history")

    def next(self) -> Optional[InputLine]:
        logging.debug(
            f"[{self.session}] history: moving next from {self.cursor_pos} - {len(self.lines)} of history"
        )
        if self.cursor_pos is None:
            return None
        elif self.cursor_pos < len(self.lines) - 1:
            self.cursor_pos += 1
        else:
            self.cursor_pos = None
            return

        logging.debug(f"[{self.session}] history: post-move next: {self.cursor_pos}")
        return self.lines[self.cursor_pos]

    def prev(self) -> Optional[InputLine]:
        logging.debug(
            f"[{self.session}] history: moving prev from {self.cursor_pos} - {len(self.lines)} of history"
        )

        if len(self.lines) == 0:
            return None

        if self.cursor_pos is None:
            self.cursor_pos = len(self.lines) - 1
        elif self.cursor_pos > 0:
            self.cursor_pos -= 1

        logging.debug(f"[{self.session}] history: post-move prev: {self.cursor_pos}")
        return self.lines[self.cursor_pos]


def get_history(session_id: int) -> Optional[History]:
    return history.get(session_id)


@on_event(EventType.InputLine)
async def input_sent(event: Event):
    if history.get(event.id) is None:
        return
    history[event.id].add(event.input)


@on_event(EventType.Shortcut)
async def shortcut(event: Event):
    if event.shortcut == Shortcut.HistoryNext:
        line = history[event.id].next()
    elif event.shortcut == Shortcut.HistoryPrevious:
        line = history[event.id].prev()
    else:
        return

    logging.info(f"populating {event.id} input with: {line}")
    if line is None:
        mudpuppy_core.set_input(event.id, "")
    else:
        if line.original is None:
            mudpuppy_core.set_input(event.id, line.sent)
        else:
            mudpuppy_core.set_input(event.id, line.original)

    input = await mudpuppy_core.get_input(event.id)
    logging.info(f"val is: {input}")


@on_new_session()
async def setup(event: Event):
    sesh_info = await mudpuppy_core.session_info(event.id)
    history[event.id] = History(sesh_info)


history: dict[int, History] = {}
logging.debug("history module loaded")
