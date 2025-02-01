import logging
import json
from typing import Any, Dict
from pathlib import Path

from custom_layout import CUSTOM_LAYOUT_READY, layouts
from mudpuppy_core import (
    Event,
    EventType,
    MudLine,
    OutputItem,
    mudpuppy_core,
)

from mudpuppy import on_event, on_gmcp, unload_handlers

STATE_FILE = Path(mudpuppy_core.data_dir()) / "status_state.json"


class StatusArea:
    session_id: int

    def __init__(self, session_id: int):
        self.session_id = session_id
        self.hp: int = 0
        self.max_hp: int = 0
        self.sp: int = 0
        self.max_sp: int = 0
        self.char_name: str = "Unknown"
        self.full_name: str = "Unknown"
        self.guild: str = "Unknown"
        self.money: int = 0
        self.bank_money: int = 0
        self.exp: int = 0
        self.level: int = 0

    def to_dict(self) -> dict:
        return {
            "hp": self.hp,
            "max_hp": self.max_hp,
            "sp": self.sp,
            "max_sp": self.max_sp,
            "char_name": self.char_name,
            "full_name": self.full_name,
            "guild": self.guild,
            "money": self.money,
            "bank_money": self.bank_money,
            "exp": self.exp,
            "level": self.level,
        }

    @classmethod
    def from_dict(cls, session_id: int, data: dict) -> "StatusArea":
        status = cls(session_id)
        status.hp = data.get("hp", 0)
        status.max_hp = data.get("max_hp", 0)
        status.sp = data.get("sp", 0)
        status.max_sp = data.get("max_sp", 0)
        status.char_name = data.get("char_name", "Unknown")
        status.full_name = data.get("full_name", "Unknown")
        status.guild = data.get("guild", "Unknown")
        status.money = data.get("money", 0)
        status.bank_money = data.get("bank_money", 0)
        status.exp = data.get("exp", 0)
        status.level = data.get("level", 0)
        return status

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


def save_state():
    logging.info(f"trying to save to {STATE_FILE}")
    state = {}
    for session_id, status in status_areas.items():
        state[int(session_id)] = status.to_dict()
    try:
        with open(STATE_FILE, "w") as f:
            json.dump(state, f)
    except OSError as e:
        logging.error(f"failed to save state: {e}")


def load_state() -> Dict[int, dict]:
    logging.info(f"trying to load from {STATE_FILE}")
    if not STATE_FILE.exists():
        return {}

    try:
        with open(STATE_FILE) as f:
            state = json.load(f)
        STATE_FILE.unlink()
        return state
    except (OSError, json.JSONDecodeError) as e:
        logging.warning(f"no pre-existing state to load: {e}")
        return {}


@on_event(EventType.GmcpEnabled)
async def gmcp_enabled(event: Event):
    assert isinstance(event, Event.GmcpEnabled)
    logging.debug(f"telling {event.id} that we support gmcp Char")
    await mudpuppy_core.gmcp_register(event.id, "Char")


@on_gmcp("Char.Vitals")
async def gmcp_vitals(session_id: int, data: Any):
    await status_area(session_id).vitals(data)


@on_gmcp("Char.Name")
async def gmcp_name(session_id: int, data: Any):
    await status_area(session_id).name(data)


@on_gmcp("Char.Status")
async def gmcp_status(session_id: int, data: Any):
    await status_area(session_id).status(data)


@on_event(EventType.Python)
async def py_event(event: Event):
    assert isinstance(event, Event.Python)

    if event.id is None:
        return

    if event.custom_type == CUSTOM_LAYOUT_READY:
        status_area(event.id)._layout_ready()


def status_area(session_id: int) -> StatusArea:
    status_area = status_areas.get(session_id)
    if status_area is None:
        logging.debug(f"constructing status area for {session_id}")
        status_area = StatusArea(session_id)
        status_areas[session_id] = status_area

    return status_area


status_areas: Dict[int, StatusArea] = {}
logging.debug("status_buf plugin loaded")

prev_data = load_state()
if len(prev_data) > 0:
    for session_id_raw, data in prev_data.items():
        logging.debug("restoring session {session_id} data from prior reload")
        sesh_id = int(session_id_raw)
        status_areas[sesh_id] = StatusArea.from_dict(sesh_id, data)


def __reload__():
    logging.debug("Saving status areas before reload")
    save_state()
    unload_handlers(__name__)
