import asyncio
import logging
from typing import Optional, List

from pup import Session, Event, EventType, Tab

logging.debug("TabAnimator package loaded")


class TabAnimator:
    """
    Silly example of an async task being used to animate a Tab's title with
    an activity throbber.

    Usage:
    ```py
    from title_anim import TabAnimator
    await TabAnimator().setup(sesh) # where 'sesh' is a Session, e.g. from setup()
    ```
    """

    name: str
    tab: Optional[Tab] = None
    frames: List[str] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]

    async def setup(self, sesh: Session):
        self.name = str(sesh)
        logging.debug(f"TabAnimator: setting up for {self.name}")
        sesh.add_event_handler(EventType.SessionClosed, self.on_close)

        self.tab = await sesh.tab()
        asyncio.create_task(self.animate())

    async def on_close(self, _sesh: Session, _ev: Event):
        logging.debug(f"TabAnimator: {self.name} tab closed")
        self.tab = None

    async def animate(self):
        i = 0
        while True:
            if self.tab is None:
                logging.debug(f"{self.name} tab_title_task ending")
                break
            self.tab.set_title(f"{self.frames[i]}")
            i = (i + 1) % len(self.frames)
            await asyncio.sleep(0.1)
