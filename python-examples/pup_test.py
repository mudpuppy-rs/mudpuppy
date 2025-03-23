import logging
import pup
import asyncio

from pup import (
    Session,
    Trigger,
    Mud,
    GlobalEvent,
    GlobalEventType,
    Event,
    EventType,
    Tls,
    KeyEvent
)

logging.info("hi from pup_test.py")

active_session = None


async def global_event_handler(ev: Event):
    logging.info(f"global event: {ev}")
    print(f"[!] global event: {ev}")


async def new_session(ev: Event):
    global active_session
    active_session = ev.session
    active_session.add_event_handler(EventType.All, event_handler)
    active_session.add_event_handler(EventType.Line, print_line)


async def session_changed(ev: Event):
    active_session = ev.changed_to


async def event_handler(sesh: Session, ev: Event):
    if isinstance(ev, Event.Line):
        return
    logging.info(f"{sesh}: event: {ev}")
    print(f"[!] {sesh}: event: {ev}")


async def print_line(sesh: Session, ev: Event):
    print(f"[*] {sesh}: {ev.line}")


async def on_connected(sesh: Session, ev: Event):
    print(f"[!] {sesh}: connected - logging in to sneak")
    sesh.send_line("sneak")


async def stdin_reader():
    while True:
        try:
            line = await asyncio.to_thread(input)
            if active_session:
                print(f"> {line}")
                active_session.send_line(line)
        except Exception as e:
            logging.error(f"error reading from stdin: {e}")
            exit(1)


async def setup():
    logging.info("I'm setting up!")

    asyncio.create_task(stdin_reader())

    pup.add_global_event_handler(GlobalEventType.All, global_event_handler)
    pup.add_global_event_handler(GlobalEventType.ActiveSessionChanged, session_changed)
    pup.add_global_event_handler(GlobalEventType.NewSession, new_session)

    config = await pup.config()

    my_mud = Mud("Test", "192.168.40.97", 6789)
    muds = config.muds
    muds.append(my_mud)
    config.muds = muds

    sesh = await pup.new_session(my_mud)
    sesh.add_event_handler(EventType.SessionConnected, on_connected)
    sesh.connect()
