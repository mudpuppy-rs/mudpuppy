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
    Gauge,
    Button,
)

from mudpuppy import on_event, unload_handlers

CHANNEL_CAPTURE_SECTION = "channel_capture"
GAUGE_SECTION = "gauges"
HP_GAUGE_SECTION = "hp_gauge"
CP_GAUGE_SECTION = "cp_gauge"
STATUS_SECTION = "status"
NAV_AREA_SECTION = "nav_area"
CUSTOM_LAYOUT_READY = "custom_layout_ready"


class TestCustomLayout:
    session_id: int
    channel_buffer: BufferConfig
    channel_buffer_constraint: Constraint
    channel_buffer_id: Optional[int] = None
    status_buffer: BufferConfig
    status_buffer_id: Optional[int] = None
    status_on_left: bool = False
    gauges: Optional[Dict[str, Gauge]] = None
    nav_buttons: Optional[Dict[int, Button]] = None

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

        # Set up a nav button area below the STATUS_SECTION.
        await self.nav_layout_init(layout_manager)

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

        if self.gauges is None:
            await self.gauge_layout_init(layout_manager)

        await mudpuppy_core.emit_event(CUSTOM_LAYOUT_READY, None, self.session_id)

    # TODO(XXX): deduplicate/tidy this code a bit :)
    # TODO(XXX): fix for /reload ?
    async def nav_layout_init(self, layout_manager: LayoutManager):
        # Create a section under the status buffer for the navigation buttons
        await layout_manager.split_section(
            self.session_id,
            section_name=STATUS_SECTION,
            constraint=Constraint.with_percentage(80),
            new_section_name=NAV_AREA_SECTION,
            new_constraint=Constraint.with_min(12),
            direction=Direction.Vertical,
            old_section_first=True,
        )

        # Extend the nav area into three rows
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION,
            new_section_name="nav_area_top",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Vertical,
        )
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION,
            new_section_name="nav_area_middle",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Vertical,
        )
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION,
            new_section_name="nav_area_bottom",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Vertical,
        )

        # Extend each nav area row into three sub-sections

        # Top row: NW, N, NE
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION + "_top",
            new_section_name=NAV_AREA_SECTION + "_top_left",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Horizontal,
        )
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION + "_top",
            new_section_name=NAV_AREA_SECTION + "_top_middle",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Horizontal,
        )
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION + "_top",
            new_section_name=NAV_AREA_SECTION + "_top_right",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Horizontal,
        )

        # Middle row: W, Look, E
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION + "_middle",
            new_section_name=NAV_AREA_SECTION + "_middle_left",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Horizontal,
        )
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION + "_middle",
            new_section_name=NAV_AREA_SECTION + "_middle_middle",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Horizontal,
        )
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION + "_middle",
            new_section_name=NAV_AREA_SECTION + "_middle_right",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Horizontal,
        )

        # Bottom row: SW, S, SE
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION + "_bottom",
            new_section_name=NAV_AREA_SECTION + "_bottom_left",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Horizontal,
        )
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION + "_bottom",
            new_section_name=NAV_AREA_SECTION + "_bottom_middle",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Horizontal,
        )
        await layout_manager.extend_section(
            self.session_id,
            section_name=NAV_AREA_SECTION + "_bottom",
            new_section_name=NAV_AREA_SECTION + "_bottom_right",
            constraint=Constraint.with_percentage(33),
            direction=Direction.Horizontal,
        )

        # Create the nav buttons!
        logging.debug("creating nav buttons for {self.session_id}")

        # Top row: NW, N, NE
        nav_north_west_btn = await mudpuppy_core.new_button(
            self.session_id,
            self.nav_button_click,
            layout_name=NAV_AREA_SECTION + "_top_left",
            label="NW",
        )
        nav_north_btn = await mudpuppy_core.new_button(
            self.session_id,
            self.nav_button_click,
            layout_name=NAV_AREA_SECTION + "_top_middle",
            label="N",
        )
        nav_north_east_btn = await mudpuppy_core.new_button(
            self.session_id,
            self.nav_button_click,
            layout_name=NAV_AREA_SECTION + "_top_right",
            label="NE",
        )

        # Middle row: W, Look, E
        nav_west_btn = await mudpuppy_core.new_button(
            self.session_id,
            self.nav_button_click,
            layout_name=NAV_AREA_SECTION + "_middle_left",
            label="W",
        )
        nav_look_btn = await mudpuppy_core.new_button(
            self.session_id,
            self.nav_button_click,
            layout_name=NAV_AREA_SECTION + "_middle_middle",
            label="Look",
        )
        nav_east_btn = await mudpuppy_core.new_button(
            self.session_id,
            self.nav_button_click,
            layout_name=NAV_AREA_SECTION + "_middle_right",
            label="E",
        )

        # Bottom row: SW, S, SE
        nav_north_south_west_btn = await mudpuppy_core.new_button(
            self.session_id,
            self.nav_button_click,
            layout_name=NAV_AREA_SECTION + "_bottom_left",
            label="SW",
        )
        nav_south_btn = await mudpuppy_core.new_button(
            self.session_id,
            self.nav_button_click,
            layout_name=NAV_AREA_SECTION + "_bottom_middle",
            label="S",
        )
        nav_south_east_btn = await mudpuppy_core.new_button(
            self.session_id,
            self.nav_button_click,
            layout_name=NAV_AREA_SECTION + "_bottom_right",
            label="SE",
        )

        # Store the created nav buttons in a dict for easy access by button ID.
        self.nav_buttons = {
            nav_north_west_btn.id: nav_north_west_btn,
            nav_north_btn.id: nav_north_btn,
            nav_north_east_btn.id: nav_north_east_btn,
            nav_west_btn.id: nav_west_btn,
            nav_look_btn.id: nav_look_btn,
            nav_east_btn.id: nav_east_btn,
            nav_north_south_west_btn.id: nav_north_south_west_btn,
            nav_south_btn.id: nav_south_btn,
            nav_south_east_btn.id: nav_south_east_btn,
        }

    async def gauge_layout_init(self, layout_manager: LayoutManager):
        logging.debug(f"creating gauge section for {self.session_id}")
        # Create a veritical split for the set of gauges
        await layout_manager.split_section(
            self.session_id,
            section_name="output_area",
            constraint=Constraint.with_min(1),
            new_section_name=GAUGE_SECTION,
            new_constraint=Constraint.with_max(3),
            direction=Direction.Vertical,
            old_section_first=True,
        )
        # Extend the gauge section with a horizontal section for the HP gauge
        await layout_manager.extend_section(
            self.session_id,
            section_name=GAUGE_SECTION,
            new_section_name=HP_GAUGE_SECTION,
            constraint=Constraint.with_percentage(50),
            direction=Direction.Horizontal,
        )
        # Extend the gauge section with a horizontal section for the CP gauge
        await layout_manager.extend_section(
            self.session_id,
            section_name=GAUGE_SECTION,
            new_section_name=CP_GAUGE_SECTION,
            constraint=Constraint.with_percentage(50),
            direction=Direction.Horizontal,
        )
        # Create the HP gauge, assigned to the HP_GAUGE_SECTION
        logging.debug(f"creating health gauge for {self.session_id}")
        health_gauge = await mudpuppy_core.new_gauge(
            self.session_id,
            layout_name=HP_GAUGE_SECTION,
            title="HP",
            rgb=(0, 255, 0),
        )
        logging.debug(f"health gauge id: {health_gauge.id}")

        # Create the CP gauge, assigned to the CP_GAUGE_SECTION
        logging.debug(f"creating cp gauge for {self.session_id}")
        cp_gauge = await mudpuppy_core.new_gauge(
            self.session_id,
            layout_name=CP_GAUGE_SECTION,
            title="CP",
            rgb=(0, 0, 255),
        )
        logging.debug(f"cp gauge id: {cp_gauge.id}")

        # Store both gauges keyed by their layout name.
        # Consumers can poke at the Gauge instances in this dict to update as needed.
        self.gauges = {HP_GAUGE_SECTION: health_gauge, CP_GAUGE_SECTION: cp_gauge}

    def toggle_status_and_nav_area(self):
        section = STATUS_SECTION + "_" + NAV_AREA_SECTION
        constraint = manager.get_constraint(self.session_id, section)
        if constraint.max == 0:
            manager.show_section(
                self.session_id, section, Constraint.with_percentage(25)
            )
        else:
            manager.hide_section(self.session_id, section)

    def resize_status_and_nav_area(self, amount: int = 5):
        section = STATUS_SECTION + "_" + NAV_AREA_SECTION
        logging.debug(f"resizing status area by {amount}")
        constraint = manager.get_constraint(self.session_id, section)
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

    def toggle_gauge_area(self):
        constraint = manager.get_constraint(self.session_id, GAUGE_SECTION)
        if constraint.max == 0:
            manager.show_section(self.session_id, GAUGE_SECTION, Constraint.with_max(3))
        else:
            manager.hide_section(self.session_id, GAUGE_SECTION)

    async def nav_button_click(self, session_id: int, button_id: int):
        assert self.nav_buttons is not None
        clicked_btn = self.nav_buttons[button_id]
        await mudpuppy_core.send_line(session_id, clicked_btn.label.lower())


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
        layout.toggle_status_and_nav_area()
        return
    elif no_modifiers and code == "f6":
        layout.toggle_gauge_area()
        return

    if modifiers == ["alt"]:
        if code == "h":
            layout.resize_status_and_nav_area(5)
        elif code == "l":
            layout.resize_status_and_nav_area(-5)
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
