import logging
import pup

from pup import (
    Session,
    GlobalEventType,
    Event,
    EventType,
)

logging.debug("module loaded")
print("[G] Welcome to MudPuppy (headless)")


# TODO(XXX): figure out new output rendering to stdout.


async def print_global_event(ev: Event):
    print(f"[G] {ev}")


async def print_session_event(sesh: Session, ev: Event):
    if isinstance(ev, Event.Line):
        return
    elif isinstance(ev, Event.InputLine):
        if ev.line.original is not None:
            print(f"[E] {sesh}: > {ev.line.sent} ({ev.line.original})")
        else:
            print(f"[E] {sesh}: > {ev.line.sent}")
    else:
        print(f"[E] {sesh}: {ev}")


async def print_line(sesh: Session, ev: Event):
    print(f"[L] {sesh}: {ev.line}")


async def new_session(ev: Event):
    logging.debug(f"configuring session {ev.session}")
    ev.session.add_event_handler(EventType.All, print_session_event)
    ev.session.add_event_handler(EventType.Line, print_line)


async def setup():
    logging.info("setting up for headless mode")
    pup.add_global_event_handler(GlobalEventType.All, print_global_event)
    pup.add_global_event_handler(GlobalEventType.NewSession, new_session)
