import logging
from typing import Any, Awaitable, Callable, Dict, List, Optional

from mudpuppy_core import (
    BufferConfig,
    BufferId,
    Constraint,
    Direction,
    Event,
    EventType,
    LayoutNode,
    SessionId,
    SessionInfo,
    mudpuppy_core,
)

from mudpuppy import on_event, on_new_session

__all__ = ["LayoutManager", "LayoutHandler", "manager"]

# TODO(XXX): Any should be LayoutManager, but there's a circular definition issue where this
#   type needs to refer to the class, but the class refers to the type as an arg type.
LayoutHandler = Callable[[SessionInfo, Any], Awaitable[None]]


class LayoutManager:
    def __init__(self):
        self.callbacks: List[LayoutHandler] = []
        self.layouts: Dict[SessionId, LayoutNode] = {}

    def add_callback(self, callback: LayoutHandler):
        logging.debug(f"adding layout callback: {callback}")
        self.callbacks.append(callback)

    def remove_callback(self, callback: LayoutHandler):
        logging.debug(f"removing layout callback: {callback}")
        self.callbacks.remove(callback)

    async def on_new_session(self, sesh_info: SessionInfo):
        logging.debug(f"layout manager dispatching callbacks for {sesh_info}")
        layout = await mudpuppy_core.layout(sesh_info.id)
        logging.debug(f"layout: {layout}")
        self.layouts[sesh_info.id] = layout
        logging.debug("time to call callbacks")

        # Importantly, this is done in order of callback add.
        # Combined with user python scripts being loaded in name order this allows
        # multiple scripts to add to the layout in a predictable order.
        for callback in self.callbacks:
            logging.debug(f"awaiting {callback}")
            await callback(sesh_info, self)

    def get_constraint(self, sesh_id: SessionId, section_name: str) -> Constraint:
        layout = self.layouts.get(sesh_id)
        if layout is None:
            raise ValueError("no layout found for session")
        (constraint, _) = layout.find_section(section_name)
        return constraint

    def hide_section(self, sesh_id: SessionId, section_name: str):
        self.get_constraint(sesh_id, section_name).set_max(0)

    def show_section(
        self, sesh_id: SessionId, section_name: str, constraint: Constraint
    ):
        self.get_constraint(sesh_id, section_name).set_from(constraint)

    async def extend_section(
        self,
        sesh_id: SessionId,
        *,
        section_name: str,
        new_section_name: str,
        constraint: Constraint,
        direction: Direction = Direction.Vertical,
        margin: int = 0,
        buffer_config: BufferConfig = None,
    ) -> Optional[BufferId]:
        if section_name == "" or new_section_name == "":
            raise ValueError("section names must not be empty")

        layout = self.layouts.get(sesh_id)
        if layout is None:
            raise ValueError("no layout found for session")

        (_, parent_section) = layout.find_section(
            section_name
        )  # NB: errors if not found.
        logging.debug(f"found parent section: {parent_section}")
        parent_section.direction = direction

        new_section = LayoutNode(new_section_name)
        new_section.margin = margin
        logging.debug(f"adding new section: {new_section}")
        parent_section.add_section(new_section, constraint)

        buffer_id = None
        if buffer_config is not None:
            logging.debug("creating buffer")
            buffer_config.layout_name = new_section_name
            buffer_id = await mudpuppy_core.new_buffer(sesh_id, buffer_config)

        logging.debug("completed addition. all sections:")
        all_layouts = layout.all_layouts()
        for name, layout in all_layouts.items():
            logging.debug(f"layout: {name} -> {layout}")

        return buffer_id

    async def split_section(
        self,
        sesh_id: SessionId,
        *,
        section_name: str,
        constraint: Constraint,
        old_section_first: bool = True,
        new_section_name: str,
        new_constraint: Constraint,
        direction: Direction = Direction.Vertical,
        margin: int = 0,
        buffer_config: BufferConfig = None,
    ):
        if section_name == "" or new_section_name == "":
            raise ValueError("section names must not be empty")

        layout = self.layouts.get(sesh_id)
        if layout is None:
            raise ValueError("no layout found for session")

        (_, parent_section) = layout.find_section(
            section_name
        )  # NB: errors if not found.
        # Create a replacement node for the parent section.
        new_old_section = LayoutNode(section_name)
        new_old_section.margin = parent_section.margin
        new_old_section.direction = parent_section.direction

        # Then update the parent section so that it can be split, and the old section preserved as subsection.
        parent_section.direction = direction
        parent_section.name = f"{section_name}_{new_section_name}"

        # We also create a new section to go alongside the old.
        new_section = LayoutNode(new_section_name)
        new_section.margin = margin

        if old_section_first:
            parent_section.add_section(new_old_section, constraint)
            parent_section.add_section(new_section, new_constraint)
        else:
            parent_section.add_section(new_section, new_constraint)
            parent_section.add_section(new_old_section, constraint)

        if buffer_config:
            buffer_config.layout_name = new_section_name
            buffer_id = await mudpuppy_core.new_buffer(sesh_id, buffer_config)
            return buffer_id

        return None


@on_new_session()
async def setup_session(event: Event):
    await manager.on_new_session(event.info)


@on_event(EventType.ResumeSession)
async def on_resume(event: Event):
    info = await mudpuppy_core.session_info(event.id)
    await manager.on_new_session(info)


manager = LayoutManager()
logging.debug("layout manager module loaded")
