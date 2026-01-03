import logging
import struct
from typing import Set, cast

from pup import Session, Event, Dimensions

from pup_events import (
    session_connected as on_session_connected,
    session_disconnected as on_session_disconnected,
    telnet_option_enabled as on_telnet_option_enabled,
    telnet_option_disabled as on_telnet_option_disabled,
    buffer_resized as on_buffer_resized,
)

NAWS_OPTION = 31

logging.debug("module loaded")
enabled_sessions: Set[Session] = set()


@on_session_connected()
async def connected(sesh: Session, _: Event):
    logging.debug(f"{sesh} requesting telnet NAWS option")
    sesh.telnet().request_enable_option(NAWS_OPTION)


@on_session_disconnected()
async def disconnected(sesh: Session, _: Event):
    logging.debug(f"{sesh} disconnected")
    if sesh in enabled_sessions:
        enabled_sessions.remove(sesh)


@on_telnet_option_enabled(option=NAWS_OPTION)
async def on_option_enabled(sesh: Session, _ev: Event):
    logging.debug(f"{sesh} option enabled")
    enabled_sessions.add(sesh)


@on_telnet_option_disabled(option=NAWS_OPTION)
async def on_option_disabled(sesh: Session, _ev: Event):
    logging.debug(f"{sesh} option disabled")
    enabled_sessions.remove(sesh)


# TODO(XXX): const for this?
# Ignore buffers that aren't the main output area
@on_buffer_resized(name="MUD Output")
async def on_resized(sesh: Session, ev: Event):
    assert isinstance(ev, Event.BufferResized)

    if sesh not in enabled_sessions:
        logging.debug(f"{sesh} ignoring resize - NAWS not enabled")
        return

    logging.debug(f"{sesh} NAWS updating to {ev.to}")
    dimensions = cast(Dimensions, ev.to)
    sesh.telnet().send_subnegotiation(
        NAWS_OPTION,
        struct.pack(">HH", dimensions.width(), dimensions.height()),
    )
