"""
Example: Custom Tab Creation
"""

import logging
from typing import Optional

import pup
from pup import (
    Session,
    Event,
    Buffer,
    Section,
    Constraint,
    OutputItem,
    MudLine,
    Tab,
)

from pup_events import tab_closed

logging.debug("module loaded")

_tab: Optional[Tab] = None


@tab_closed()
async def on_close(_sesh: Session, ev: Event):
    global _tab
    if _tab is not None and ev.tab_id == _tab.id:
        logging.debug("custom tab was closed.")
        _tab = None


async def custom_tab() -> Tab:
    logging.debug("custom_tab called")
    global _tab
    if _tab is not None:
        logging.debug(f"already have custom tab: {_tab}")
        return _tab

    # Create a buffer for some static content
    tab_name = "Custom Tab"
    buffer = Buffer(tab_name)

    # Add some initial content to the buffer
    for line in [
        "Welcome to the Custom Tab Demo! 🎸",
        "",
        "This tab was created dynamically from Python code.",
        "Pretty rad, right?",
    ]:
        buffer.add(OutputItem.mud(MudLine(bytes(line + "\n", "utf-8"))))

    # Create the custom tab w/ the buffer
    tab = await pup.create_tab(tab_name, buffers=[buffer])
    logging.info(f"Created custom tab with ID {tab.id}")

    _tab = tab
    return _tab
