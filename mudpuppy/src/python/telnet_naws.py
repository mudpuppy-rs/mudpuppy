import logging
import struct
from typing import Set

from pup import Session, Event

from pup_events import (
    session_connected,
    session_disconnected,
    telnet_option_enabled,
    telnet_option_disabled,
    buffer_resized,
)

NAWS_OPTION = 31

logging.debug("module loaded")
enabled_sessions: Set[Session] = set()


@session_connected()
async def connected(sesh: Session, _: Event):
    logging.debug(f"{sesh} requesting telnet NAWS option")
    sesh.telnet().request_enable_option(NAWS_OPTION)


@session_disconnected()
async def disconnected(sesh: Session, _: Event):
    logging.debug(f"{sesh} disconnected")
    if sesh in enabled_sessions:
        enabled_sessions.remove(sesh)


@telnet_option_enabled(option=NAWS_OPTION)
async def telnet_option_enabled(sesh: Session, _ev: Event):
    logging.debug(f"{sesh} option enabled")
    enabled_sessions.add(sesh)


@telnet_option_disabled(option=NAWS_OPTION)
async def telnet_option_disabled(sesh: Session, _ev: Event):
    logging.debug(f"{sesh} option disabled")
    enabled_sessions.remove(sesh)


@buffer_resized()
async def buffer_resized(sesh: Session, ev: Event):
    # TODO(XXX): const for this?
    # Ignore buffers that aren't the main output area
    if ev.name != "MUD Output":
        return

    if sesh not in enabled_sessions:
        logging.debug(f"{sesh} ignoring resize - NAWS not enabled")
        return

    logging.debug(f"{sesh} NAWS updating to {ev.to}")
    sesh.telnet().send_subnegotiation(
        NAWS_OPTION,
        struct.pack(">HH", ev.to.width(), ev.to.height()),
    )
