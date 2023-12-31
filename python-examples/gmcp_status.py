import logging
from typing import Any, Dict

from custom_layout import CUSTOM_LAYOUT_READY, layouts
from mudpuppy_core import (
    Event,
    EventType,
    MudLine,
    OutputItem,
    SessionId,
    mudpuppy_core,
)

from mudpuppy import on_event, on_gmcp, unload_handlers


class StatusArea:
    session_id: int

    hp: int = 0
    max_hp: int = 0
    sp: int = 0
    max_sp: int = 0
    char_name: str = "Unknown"
    full_name: str = "Unknown"
    guild: str = "Unknown"
    money: int = 0
    bank_money: int = 0
    exp: int = 0
    level: int = 0

    def __init__(self, session_id: int):
        self.session_id = session_id

    async def vitals(self, data: Any):
        self.hp = data.get("hp", self.hp)
        self.max_hp = data.get("maxhp", self.max_hp)
        self.sp = data.get("sp", self.sp)
        self.max_sp = data.get("maxsp", self.max_sp)
        self._update()

    async def name(self, data: Any):
        self.char_name = data.get("name", self.char_name)
        self.full_name = data.get("fullname", self.full_name)
        self.guild = data.get("guild", self.guild)
        self._update()

    async def status(self, data: Any):
        self.money = data.get("money", self.money)
        self.bank_money = data.get("bankmoney", self.bank_money)
        self.exp = data.get("xp", self.exp)
        self.level = data.get("level", self.level)
        self._update()

    def _layout_ready(self):
        logging.debug(f"custom layout ready for {self.session_id}")
        self._update()

    def _update(self):
        def make_line(line: str) -> OutputItem:
            return OutputItem.mud(MudLine(line.encode("utf-8")))

        layout = layouts.get(self.session_id)
        if layout is None:
            logging.warning(f"no custom layout found for {self.session_id}")
            return

        logging.debug(f"status: setting buffer for session {self.session_id}")
        layout.status_buffer.output.set(
            [
                make_line(f"Name: {self.char_name}"),
                make_line(f"Title: {self.full_name}"),
                make_line(f"Guild: {self.guild}"),
                make_line(f"HP: {self.hp}/{self.max_hp}"),
                make_line(f"SP: {self.sp}/{self.max_sp}"),
                make_line(f"EXP: {self.exp}"),
                make_line(f"LVL: {self.level}"),
                make_line(f"$$$: {self.money} Bank: {self.bank_money}"),
            ]
        )


@on_event(EventType.GmcpEnabled)
async def gmcp_enabled(event: Event):
    logging.debug(f"telling {event.id} that we support gmcp Char")
    await mudpuppy_core.gmcp_register(event.id, "Char")


@on_gmcp("Char.Vitals")
async def gmcp_vitals(session_id: SessionId, data: Any):
    await status_area(session_id).vitals(data)


@on_gmcp("Char.Name")
async def gmcp_name(session_id: SessionId, data: Any):
    await status_area(session_id).name(data)


@on_gmcp("Char.Status")
async def gmcp_status(session_id: SessionId, data: Any):
    await status_area(session_id).status(data)


@on_event(EventType.Python)
async def py_event(event: Event):
    if event.custom_type == CUSTOM_LAYOUT_READY:
        status_area(event.id)._layout_ready()


def status_area(session_id: SessionId) -> StatusArea:
    status_area = status_areas.get(session_id)
    if status_area is None:
        logging.debug(f"constructing status area for {session_id}")
        status_area = StatusArea(session_id)
        status_areas[session_id] = status_area
    return status_area


status_areas: Dict[int, StatusArea] = {}
logging.debug("status_buf plugin loaded")


def __reload__():
    logging.debug("\n\n\n\nUser Python About To Reload!\n\n\n\n")
    unload_handlers(__name__)
