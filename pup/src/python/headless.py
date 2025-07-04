import logging
import pup

from pup import (
    Session,
    Event,
    EventType,
)

logging.debug("module loaded")
print("[G] Welcome to MudPuppy (headless)")


# TODO(XXX): figure out new output rendering to stdout.

async def print_session_event(sesh: Session, ev: Event):
    if isinstance(ev, Event.Line) or isinstance(ev, Event.InputLine):
        return
    print(f"[E] {sesh}: {ev}")


async def print_line(sesh: Session, ev: Event):
    print(f"[L] {sesh}: {ev.line}")

async def print_input(sesh: Session, ev: Event):
    if ev.line.original is not None:
        print(f"[E] {sesh}: > {ev.line.sent} ({ev.line.original})")
    else:
        print(f"[E] {sesh}: > {ev.line.sent}")


async def new_session(sesh: Session):
    logging.debug(f"configuring session {sesh}")
    sesh.add_event_handler(EventType.All, print_session_event)
    sesh.add_event_handler(EventType.Line, print_line)
    sesh.add_event_handler(EventType.InputLine, print_input)


async def setup():
    logging.info("setting up for headless mode")
    pup.new_session_handler(new_session)
