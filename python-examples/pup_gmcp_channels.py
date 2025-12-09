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
)

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

    async def setup(self, sesh: Session, *, with_logfile: bool = False):
        self.sesh = sesh
        self.logger.debug(f"{self.sesh}: setting up")

        # Create a layout section in front of the MUD Output.
        tab = await sesh.tab()
        layout = await tab.layout()
        output_parent = layout.get_parent("MUD Output")
        output_parent.insert_child(0, Constraint.Min(5), Section("Channels"))
        self.logger.debug(f"{self.sesh}: added channel")

        # Create a buffer in the tab to put channel messages into. Using the same name
        # as the layout section we created.
        buffer = Buffer("Channels")
        buffer.line_wrap = True
        buffer = Buffer("Channels")
        buffer.line_wrap = True
        buffer.border_bottom = True
        buffer.border_left = True
        buffer.border_right = True
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

    async def handle_gmcp(self, _sesh: Session, ev: Event):
        assert isinstance(ev, Event.GmcpMessage)
        if ev.package != "Comm.Channel.Text":
            return

        if self.sesh is None or self.buffer is None:
            self.logger.warn("handle_gmcp() called before Setup()")
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
