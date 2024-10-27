import logging
import re
import unicodedata
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, TextIO

from custom_layout import CUSTOM_LAYOUT_READY, layouts
from mudpuppy_core import (
    Event,
    EventType,
    MudLine,
    OutputItem,
    SessionId,
    SessionInfo,
    mudpuppy_core,
)

from mudpuppy import on_event, on_gmcp, unload_handlers


class ChannelLogger:
    session_info: SessionInfo
    logfile: TextIO
    logfile_path: Path
    count: int

    def __init__(self, session: SessionInfo):
        self.session_info = session

        mudname = unicodedata.normalize("NFKC", session.mud_name)
        mudname = re.sub(r"[^\w\s-]", "", mudname.lower())
        mudname = re.sub(r"[-\s]+", "-", mudname).strip("-_")

        data_dir = mudpuppy_core.data_dir()
        channel_log_dir = Path(data_dir, "logs/channels")
        channel_log_dir.mkdir(parents=True, exist_ok=True)
        self.logfile_path = Path(channel_log_dir, f"{mudname}.log")

        # Open for reading/appending.
        self.logfile = open(self.logfile_path, "a")

    def __del__(self, *args):
        if not self.logfile.closed:
            self.logfile.close()

    def layout_ready(self):
        logging.debug(f"custom layout ready for {self.session_info}")
        try:
            logfile = open(self.logfile_path, "rb")
            last_lines = logfile.readlines()[-50:]
        except FileNotFoundError:
            last_lines = []
        logging.debug(f"found {len(last_lines)} last lines")

        layout = layouts.get(self.session_info.id)
        if layout is None:
            logging.warning(f"no custom layout found for {self.session_info}")
            return

        logging.info(f"populating channel buffer (id: {layout.channel_buffer_id})")
        for line in last_lines:
            layout.channel_buffer.output.push(
                OutputItem.previous_session(MudLine(line))
            )

    async def channel_msg(self, data: Any):
        logging.debug(f"channels: handling gmcp channel msg for {self.session_info}")
        channel = data.get("channel_ansi", "unknown")
        text = data.get("text", "").rstrip() + "\n"
        now = datetime.now(timezone.utc)
        ts = now.strftime("%a %b %d %H:%M:%S %Y")

        self.logfile.write(f"{ts} {channel} {text}")
        self.logfile.flush()

        layout = layouts.get(self.session_info.id)
        if layout is None:
            logging.warning(f"no custom layout found for {self.session_info}")
            return
        logging.info(f"pushing to channel buffer (id: {layout.channel_buffer_id})")
        layout.channel_buffer.output.push(
            OutputItem.mud(MudLine(bytes(f"{ts} {channel} {text}", "utf-8")))
        )


@on_event(EventType.GmcpEnabled)
async def gmcp_ready(event: Event):
    assert isinstance(event, Event.GmcpEnabled)
    logging.debug(f"telling session {event.id} that we support gmcp Comm.Channel")
    await mudpuppy_core.gmcp_register(event.id, "Comm.Channel")


@on_gmcp("Comm.Channel.Text")
async def channel_text(session_id: SessionId, data: Any):
    logger = loggers.get(session_id)
    if logger is None:
        return
    await logger.channel_msg(data)


@on_event(EventType.Python)
async def py_event(event: Event):
    assert isinstance(event, Event.Python)
    if event.custom_type != CUSTOM_LAYOUT_READY:
        return

    if event.id is None:
        return

    if loggers.get(event.id) is None:
        sesh_info = await mudpuppy_core.session_info(event.id)
        logging.debug(f"channels: constructed gmcp channel logger for {sesh_info}")
        loggers[sesh_info.id] = ChannelLogger(sesh_info)

    loggers[event.id].layout_ready()


logging.getLogger().setLevel(0)

loggers: Dict[int, ChannelLogger] = {}
logging.debug("channels: initialized gmcp channel logger module")


def __reload__():
    logging.debug("\n\n\n\nUser Python About To Reload!\n\n\n\n")
    unload_handlers(__name__)
