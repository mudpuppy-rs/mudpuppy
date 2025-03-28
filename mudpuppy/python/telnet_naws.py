import logging
import struct
from typing import Tuple

from mudpuppy_core import Event, EventType, mudpuppy_core

from mudpuppy import on_connected, on_disconnected, on_event

NAWS_OPTION = 31


class TelnetNawsHandler:
    state: dict[int, Tuple[int, int]]

    """
    A crude approximation of RFC 1073 - "Telnet Window Size Option"
    """

    def __init__(self):
        self.state = {}

    @staticmethod
    async def on_connect(session: int):
        logging.debug(f"naws: enabling Telnet NAWS protocol for conn {session}")
        await mudpuppy_core.request_enable_option(session, NAWS_OPTION)

    def on_enabled(self, session: int):
        logging.debug(f"naws: enabled for conn {session}")
        self.state[session] = (80, 40)

    def on_disabled(self, session: int):
        if session not in self.state:
            return
        logging.debug(f"naws: disabled for conn {session}")
        del self.state[session]

    async def resize(self, session: int, columns: int, rows: int):
        if session not in self.state:
            return
        logging.debug(f"naws: session {session} resized to {columns}x{rows}")
        self.state[session] = (columns, rows)
        await mudpuppy_core.send_subnegotiation(
            session,
            NAWS_OPTION,
            struct.pack(">HH", columns, rows),
        )


@on_connected()
async def connected(event: Event):
    assert isinstance(event, Event.Connection)
    await handler.on_connect(event.id)


@on_disconnected()
async def disconnected(event: Event):
    assert isinstance(event, Event.Connection)
    handler.on_disabled(event.id)


@on_event(EventType.OptionEnabled)
async def telnet_option_enabled(event: Event):
    assert isinstance(event, Event.OptionEnabled)
    if event.option != NAWS_OPTION:
        return
    handler.on_enabled(event.id)


@on_event(EventType.OptionDisabled)
async def telnet_option_disabled(event: Event):
    assert isinstance(event, Event.OptionDisabled)
    if event.option != NAWS_OPTION:
        return
    handler.on_disabled(event.id)


@on_event(EventType.BufferResized)
async def buffer_resized(event: Event):
    assert isinstance(event, Event.BufferResized)
    await handler.resize(event.id, event.dimensions[0], event.dimensions[1])


handler = TelnetNawsHandler()
logging.debug("telnet naws module loaded")
