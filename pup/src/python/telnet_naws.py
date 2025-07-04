import logging
import struct
from typing import Set

import pup
from pup import Session, Event, EventType

NAWS_OPTION = 31

enabled_sessions: Set[Session] = set()


async def on_new_session(sesh: Session):
    logging.debug(f"{sesh} setting up telnet NAWS handlers")
    sesh.add_event_handler(EventType.SessionConnected, connected)
    sesh.add_event_handler(EventType.SessionDisconnected, disconnected)
    sesh.add_event_handler(EventType.TelnetOptionEnabled, telnet_option_enabled)
    sesh.add_event_handler(EventType.TelnetOptionDisabled, telnet_option_disabled)
    sesh.add_event_handler(EventType.BufferResized, buffer_resized)


logging.debug("module loaded")
pup.new_session_handler(on_new_session)


async def connected(sesh: Session, _: Event):
    logging.debug(f"{sesh} requesting telnet NAWS option")
    sesh.telnet().request_enable_option(NAWS_OPTION)


async def disconnected(sesh: Session, _: Event):
    logging.debug(f"{sesh} disconnected")
    enabled_sessions.remove(sesh)


async def telnet_option_enabled(sesh: Session, ev: Event):
    # Ignore other options being enabled
    if ev.option != NAWS_OPTION:
        return
    logging.debug(f"{sesh} option enabled")
    enabled_sessions.add(sesh)


async def telnet_option_disabled(sesh: Session, ev: Event):
    if ev.option != NAWS_OPTION:
        return
    logging.debug(f"{sesh} option disabled")
    enabled_sessions.remove(sesh)


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
