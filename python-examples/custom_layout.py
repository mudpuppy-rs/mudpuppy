import logging
from typing import Any, Dict, Optional

from layout import LayoutManager, manager
from mudpuppy_core import (
    BufferConfig,
    BufferDirection,
    Constraint,
    Direction,
    Event,
    EventType,
    SessionInfo,
    mudpuppy_core,
)

from mudpuppy import on_event, unload_handlers

CHANNEL_CAPTURE_SECTION = "channel_capture"
STATUS_SECTION = "status"
CUSTOM_LAYOUT_READY = "custom_layout_ready"


class TestCustomLayout:
    session_id: int
    channel_buffer: BufferConfig
    channel_buffer_constraint: Constraint
    channel_buffer_id: Optional[int] = None
    status_buffer: BufferConfig
    status_buffer_id: Optional[int] = None
    status_on_left: bool = False

    # TODO(XXX): expose status_on_left setting?
    def __init__(self, sesh_id: int):
        self.session_id = sesh_id

        self.channel_buffer = BufferConfig(CHANNEL_CAPTURE_SECTION)
        self.channel_buffer.line_wrap = True
        self.channel_buffer.border_bottom = True
        self.channel_buffer.border_left = True
        self.channel_buffer.border_right = True

        self.status_buffer = BufferConfig(STATUS_SECTION)
        self.status_buffer.line_wrap = False
        self.status_buffer.direction = BufferDirection.TopToBottom

        if self.status_on_left:
            self.status_buffer.border_right = True
        else:
            self.status_buffer.border_left = True

    async def layout_init(self, layout_manager: LayoutManager):
        existing_buffers = await mudpuppy_core.buffers(self.session_id)

        status_buffer_id = next(
            (x.id for x in existing_buffers if x.config.layout_name == STATUS_SECTION),
            None,
        )
        if status_buffer_id is None:
            logging.debug("creating status buffer section for {self.session_id}")
            # Create a status buffer split horizontally in the output area.
            status_buffer_id = await layout_manager.split_section(
                self.session_id,
                section_name="output_area",
                constraint=Constraint.with_min(40),
                new_section_name=STATUS_SECTION,
                new_constraint=Constraint.with_percentage(25),
                direction=Direction.Horizontal,
                buffer_config=self.status_buffer,
                old_section_first=not self.status_on_left,
            )
        logging.debug(f"status buffer id: {status_buffer_id}")
        self.status_buffer_id = status_buffer_id

        channel_buffer_id = next(
            (
                x.id
                for x in existing_buffers
                if x.config.layout_name == CHANNEL_CAPTURE_SECTION
            ),
            None,
        )
        if channel_buffer_id is None:
            logging.debug(f"creating channel section for {self.session_id}")
            self.channel_buffer_constraint = Constraint.with_percentage(25)
            # Create a channel buffer split vertically in the output area.
            channel_buffer_id = await layout_manager.split_section(
                self.session_id,
                section_name="output_area",
                constraint=Constraint.with_min(1),
                new_section_name=CHANNEL_CAPTURE_SECTION,
                new_constraint=self.channel_buffer_constraint,
                direction=Direction.Vertical,
                buffer_config=self.channel_buffer,
                old_section_first=False,
            )
        logging.debug(f"channel buffer id: {channel_buffer_id}")
        self.channel_buffer_id = channel_buffer_id

        await mudpuppy_core.emit_event(CUSTOM_LAYOUT_READY, None, self.session_id)

    def toggle_status_area(self):
        constraint = manager.get_constraint(self.session_id, STATUS_SECTION)
        if constraint.max == 0:
            manager.show_section(
                self.session_id, STATUS_SECTION, Constraint.with_percentage(25)
            )
        else:
            manager.hide_section(self.session_id, STATUS_SECTION)

    def resize_status_area(self, amount: int = 5):
        logging.debug(f"resizing status area by {amount}")
        constraint = manager.get_constraint(self.session_id, STATUS_SECTION)
        logging.debug(f"current: {constraint}")
        if constraint.percentage is None or constraint.percentage == 0:
            return
        constraint.set_from(Constraint.with_percentage(constraint.percentage + amount))
        logging.debug(f"now: {constraint}")

    def toggle_channel_area(self):
        constraint = manager.get_constraint(self.session_id, CHANNEL_CAPTURE_SECTION)
        if constraint.max == 0:
            manager.show_section(
                self.session_id, CHANNEL_CAPTURE_SECTION, Constraint.with_percentage(25)
            )
        else:
            manager.hide_section(self.session_id, CHANNEL_CAPTURE_SECTION)

    def resize_channel_area(self, amount: int = 5):
        logging.debug(f"resizing channel area by {amount}")
        constraint = manager.get_constraint(self.session_id, CHANNEL_CAPTURE_SECTION)
        logging.debug(f"current: {constraint}")
        if constraint.percentage is None or constraint.percentage == 0:
            return
        constraint.set_from(Constraint.with_percentage(constraint.percentage + amount))
        logging.debug(f"now: {constraint}")


async def layout_init(session: SessionInfo, layout_manager: LayoutManager):
    logging.debug("processing custom layout init")
    # You would probably customize this :-)
    if session.mud_name != "Test (TLS)":
        logging.warning("layout_init: ignoring non-Test (TLS) session: {session}")
        return

    logging.debug(f"layout_init for {session}")
    layouts[session.id] = TestCustomLayout(session.id)
    await layouts[session.id].layout_init(layout_manager)


# TODO(XXX): read key bindings from config
@on_event(EventType.KeyPress)
async def on_key(event: Event):
    assert isinstance(event, Event.KeyPress)

    layout = layouts.get(event.id)
    if layout is None:
        logging.warning(f"no custom layout found for {event.id}")
        return

    code = event.key.code()
    modifiers = event.key.modifiers()
    no_modifiers = len(modifiers) == 0
    if no_modifiers and code == "f4":
        layout.toggle_channel_area()
        return
    elif no_modifiers and code == "f5":
        layout.toggle_status_area()
        return

    if modifiers == ["alt"]:
        if code == "h":
            layout.resize_status_area(5)
        elif code == "l":
            layout.resize_status_area(-5)
        elif code == "j":
            layout.resize_channel_area(5)
        elif code == "k":
            layout.resize_channel_area(-5)
        elif code == "pageup":
            layout.channel_buffer.scroll_max()
        elif code == "pagedown":
            layout.channel_buffer.scroll_bottom()
    elif modifiers == ["shift"]:
        if code == "pageup":
            layout.channel_buffer.scroll_up(5)
        elif code == "pagedown":
            layout.channel_buffer.scroll_down(5)


def __reload__():
    logging.debug("Unregistering layouts in prep for reload")
    manager.remove_callback(layout_init)
    unload_handlers(__name__)


layouts: Dict[int, Any] = {}
manager.add_callback(layout_init)
