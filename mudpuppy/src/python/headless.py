import logging

from pup import (
    Session,
    Event,
)

from pup_events import all, input_line, line

logging.debug("module loaded")
print("[G] Welcome to MudPuppy (headless)")


# TODO(XXX): figure out new output rendering to stdout.


@all()
async def print_session_event(sesh: Session, ev: Event):
    if isinstance(ev, Event.Line) or isinstance(ev, Event.InputLine):
        return
    print(f"[E][{ev.type()}] {sesh}: {ev}")


@line()
async def print_line(sesh: Session, ev: Event):
    print(f"[L] {sesh}: {ev.line}")


@input_line()
async def print_input(sesh: Session, ev: Event):
    if ev.line.original is not None:
        print(f"[E][{ev.type()}] {sesh}: > {ev.line.sent} ({ev.line.original})")
    else:
        print(f"[E][{ev.type()}] {sesh}: > {ev.line.sent}")
