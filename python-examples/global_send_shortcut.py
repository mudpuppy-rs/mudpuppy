import logging
from typing import Optional

import pup
from pup import (
    GlobalEventType,
    GlobalEvent,
    Session,
    KeyEvent,
    Shortcut,
    PythonShortcut,
    Tab,
    Input,
    InputLine,
)


async def add_shortcut(ev: GlobalEvent):
    assert isinstance(ev, GlobalEvent.NewSession)

    async def global_send(
        _: KeyEvent, active_sesh: Optional[Session], _active_tab: Tab
    ):
        if active_sesh is None:
            return

        # Get the active session's input line, or make an empty line
        input: Input = await active_sesh.input()
        input_line: InputLine = input.pop() or InputLine("")

        # Send it to every session.
        for session in await pup.sessions():
            logging.debug(f"global_send: sending {input_line} to {session}")
            session.send_line(input_line)

    # Set a shortcut for the session's tab that invokes global_send()
    tab: Tab = await ev.session.tab()
    tab.set_shortcut(KeyEvent("alt-="), Shortcut.Python(PythonShortcut(global_send)))


logging.debug("module loaded")
# For every new session, add a shortcut.
pup.add_global_event_handler(GlobalEventType.NewSession, add_shortcut)
