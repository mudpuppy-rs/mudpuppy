import logging
import json
import re
import unicodedata
from datetime import datetime, timezone
from typing import Optional, TextIO
from pathlib import Path

import pup
from pup import (
    Session,
    Event,
    EventType,
    Buffer,
    Section,
    Constraint,
    OutputItem,
    MudLine,
    Tab,
    KeyEvent,
    Scrollbar,
)

GMCP_WINDOW_SCROLL_UP_KEY = "gmcp_window_scroll_up"
GMCP_WINDOW_SCROLL_UP_DEFAULT = "Shift-PageUp"
GMCP_WINDOW_SCROLL_DOWN_KEY = "gmcp_window_scroll_down"
GMCP_WINDOW_SCROLL_DOWN_DEFAULT = "Shift-PageDown"
GMCP_WINDOW_RESIZE_UP_KEY = "gmcp_window_resize_up"
GMCP_WINDOW_RESIZE_UP_DEFAULT = "Alt-j"
GMCP_WINDOW_RESIZE_DOWN_KEY = "gmcp_window_resize_down"
GMCP_WINDOW_RESIZE_DOWN_DEFAULT = "Alt-k"

logging.debug("pup gmcp channel package loaded")


class GmcpChannelWindow:
    """
    Create an output area that displays received GMCP channel messages. Optionally
    logs to a file, and uses that file to pre-populate the output area with the
    most recent messages received in the last session.

    Demonstrates creating a custom layout element and buffer, as well as using GMCP.
    Usage:
    ```python
    from pup_gmcp_channels import GmcpChannelWindow
    await GmcpChannelWindow().setup(sesh, with_logfile=True) # where 'sesh' is a Session, e.g. from setup()
    ```
    """

    logger: logging.Logger = logging.getLogger(__name__ + "." + __qualname__)
    sesh: Optional[Session] = None
    buffer: Optional[Buffer] = None
    logfile: Optional[TextIO] = None
    layout_min_rows: int = 10

    async def setup(self, sesh: Session, *, with_logfile: bool = False):
        self.sesh = sesh
        self.logger.debug(f"{self.sesh}: setting up")

        # Create a layout section in front of the MUD Output.
        tab = await sesh.tab()
        layout = await tab.layout()
        output_parent = layout.get_parent("MUD Output")
        if output_parent is None:
            self.logger.error("Could not find 'MUD Output' parent in layout")
            return
        output_parent.insert_child(
            0, Constraint.Min(self.layout_min_rows), Section("Channels")
        )
        self.logger.debug(f"{self.sesh}: added channel")

        # Create a buffer in the tab to put channel messages into. Using the same name
        # as the layout section we created.
        buffer = Buffer("Channels")
        buffer.config.border_top = False
        buffer.config.scrollbar = Scrollbar.Always
        tab.add_buffer(buffer)
        self.logger.debug(f"{self.sesh}: added buffer: {buffer}")
        self.buffer = buffer

        # Set up a log file, if requested.
        if with_logfile:
            charname = unicodedata.normalize("NFKC", self.sesh.character)
            charname = re.sub(r"[^\w\s-]", "", charname.lower())
            charname = re.sub(r"[-\s]+", "-", charname).strip("-_")

            char_info = await self.sesh.character_config()
            mudname = unicodedata.normalize("NFKC", char_info.mud)
            mudname = re.sub(r"[^\w\s-]", "", mudname.lower())
            mudname = re.sub(r"[-\s]+", "-", mudname).strip("-_")

            data_dir = pup.data_dir()
            channel_log_dir = Path(data_dir, "logs/channels")
            channel_log_dir.mkdir(parents=True, exist_ok=True)
            logfile_path = Path(channel_log_dir, f"{mudname}.{charname}.log")

            # See if we can read any pre-existing lines to pre-populate the buffer.
            try:
                logfile = open(logfile_path, "rb")
                last_lines = logfile.readlines()[-50:]
            except FileNotFoundError:
                last_lines = []
            for line in last_lines:
                buffer.add(OutputItem.mud(MudLine(line)))
            if len(last_lines) > 0:
                buffer.add(OutputItem.mud(MudLine(bytes("--------", "utf-8"))))

            # Then open it for appending new messages.
            self.logfile = open(logfile_path, "a", encoding="utf-8")

        # Set up the GMCP bits.
        gmcp = sesh.gmcp()
        gmcp.register("Comm.Channel")
        sesh.add_event_handler(EventType.GmcpMessage, self.handle_gmcp)
        self.logger.debug(f"{self.sesh}: registered for GMCP Comm.Channel")

        # Resolve keybindings from config with character-specific overrides.
        config = await pup.config()
        scroll_up_key = config.resolve_extra_setting(
            sesh.character,
            GMCP_WINDOW_SCROLL_UP_KEY,
            default=GMCP_WINDOW_SCROLL_UP_DEFAULT,
        )
        scroll_down_key = config.resolve_extra_setting(
            sesh.character,
            GMCP_WINDOW_SCROLL_DOWN_KEY,
            default=GMCP_WINDOW_SCROLL_DOWN_DEFAULT,
        )
        resize_up_key = config.resolve_extra_setting(
            sesh.character,
            GMCP_WINDOW_RESIZE_UP_KEY,
            default=GMCP_WINDOW_RESIZE_UP_DEFAULT,
        )
        resize_down_key = config.resolve_extra_setting(
            sesh.character,
            GMCP_WINDOW_RESIZE_DOWN_KEY,
            default=GMCP_WINDOW_RESIZE_DOWN_DEFAULT,
        )

        tab.set_shortcut(scroll_up_key, self.scroll_up)
        tab.set_shortcut(scroll_down_key, self.scroll_down)

        def resize_shortcut(adjustment: int):
            async def handler(key_event, sesh, tab):
                await self.resize_window(key_event, sesh, tab, adjustment)

            return handler

        tab.set_shortcut(resize_up_key, resize_shortcut(5))
        tab.set_shortcut(resize_down_key, resize_shortcut(-5))

    async def handle_gmcp(self, _sesh: Session, ev: Event):
        assert isinstance(ev, Event.GmcpMessage)
        if ev.package != "Comm.Channel.Text":
            return

        if self.sesh is None or self.buffer is None:
            self.logger.warning("handle_gmcp() called before Setup()")
            return

        now = datetime.now(timezone.utc)
        ts = now.strftime("%a %b %d %H:%M:%S %Y")

        data = json.loads(ev.json)
        channel = data.get("channel_ansi", "unknown")
        text = data.get("text", "").rstrip() + "\n"
        msg = f"{ts} {channel} {text}"

        if self.logfile is not None:
            self.logfile.write(msg)
            self.logfile.flush()

        self.buffer.add(OutputItem.mud(MudLine(bytes(msg, "utf-8"))))
        self.logger.debug(f"{self.sesh}: added channel msg to buffer")

    async def resize_window(
        self, _key_event: KeyEvent, sesh: Optional[Session], tab: Tab, adjustment: int
    ):
        if sesh is None or sesh != self.sesh:
            self.logger.warning("session mismatch, we have {self.sesh}, given {sesh}")
            return

        layout = await tab.layout()

        constraint = layout.get_constraint("Channels")
        if constraint is None:
            self.logger.warning("missing channel capture window constraint")
            return

        match constraint:
            case Constraint.Min(rows):
                new = rows + adjustment
                if new <= 0:
                    new = 0
                layout.set_constraint("Channels", Constraint.Min(new))
                self.logger.info(f"resized channel capture window to {new} rows")
            case _:
                self.logger.warning(
                    f"unexpected non-Min channel capture window constraint: {constraint}"
                )

    async def scroll_up(
        self, _key_event: KeyEvent, _sesh: Optional[Session], _tab: Tab
    ):
        if self.buffer is None:
            self.logger.warning("scroll up called before setup")
            return
        self.buffer.scroll_up(5)

    async def scroll_down(
        self, _key_event: KeyEvent, _sesh: Optional[Session], _tab: Tab
    ):
        if self.buffer is None:
            self.logger.warning("scroll down called before setup")
            return
        self.buffer.scroll_down(5)
