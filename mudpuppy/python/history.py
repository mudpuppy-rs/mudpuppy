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
    EchoState,
    Input,
)

from mudpuppy import on_event, on_new_session


class History:
    session: SessionInfo
    lines: list[InputLine]
    max_lines: int
    cursor_pos: Optional[int] = None
    partial: Optional[InputLine] = None

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
            f"{self} added line: {line} (original={line.original} scripted={line.scripted} echo={line.echo})"
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

    if (
        event.shortcut != Shortcut.HistoryNext
        and event.shortcut != Shortcut.HistoryPrevious
    ):
        return

    input: Input = await mudpuppy_core.input(event.id)
    if input.telnet_echo() == EchoState.Password:
        logging.debug("history: ignoring history navigation in password echo state")
        return

    h = history[event.id]

    if h.cursor_pos is None and event.shortcut == Shortcut.HistoryPrevious:
        current = input.value()
        logging.debug(
            f"history: first backward cursor move, storing current input as partial ({str(current)})"
        )
        h.partial = current

    if event.shortcut == Shortcut.HistoryNext:
        line = h.next()
    elif event.shortcut == Shortcut.HistoryPrevious:
        line = h.prev()
    else:
        raise ValueError(f"unexpected shortcut: {event.shortcut}")

    if line is None and event.shortcut == Shortcut.HistoryPrevious:
        logging.debug("history: no prev history - staying as-is")
        return
    elif (
        line is None
        and event.shortcut == Shortcut.HistoryNext
        and h.partial is not None
    ):
        h.cursor_pos = None
        logging.debug(f"history: no next - restoring partial input ({str(h.partial)})")
        input.set_value(h.partial)
    elif line is None:
        logging.debug("history: no next/prev - clearing input")
        input.reset()
    else:
        logging.info(
            f"history: populating {event.id} input with {event.shortcut} {line} (original={line.original} scripted={line.scripted} echo={line.echo})"
        )
        # When items are added to history its after processing for aliases, etc.
        # We want to reconstitute the InputLine the user produced pre-processing for display in the
        # input area. E.g. we replace sent with original if there is an original value.
        if line.original is not None:
            line = line.clone_with_original()
        input.set_value(line)


@on_new_session()
async def setup(event: Event):
    assert isinstance(event, Event.NewSession)
    sesh_info = await mudpuppy_core.session_info(event.id)
    history[event.id] = History(sesh_info)


history: dict[int, History] = {}
logging.debug("history module loaded")
