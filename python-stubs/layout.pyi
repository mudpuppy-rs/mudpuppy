"""
The `layout` module offers functions and types for manipulating the Mudpuppy
user interface.
"""

# Defined explicitly to control rendered order in docs.
__all__ = ["manager", "LayoutHandler", "LayoutManager"]

from typing import Callable, Awaitable, Optional
import mudpuppy_core

type LayoutHandler = Callable[
    [mudpuppy_core.SessionInfo, LayoutManager], Awaitable[None]
]
"""
An `async` function that is called when a new session is created and its layout
must be initialized.

The handler is called with:

* the `mudpuppy_core.SessionInfo` of the session that received the input.
* a `LayoutManager` to use to manipulate the session's layout.
"""

class LayoutManager:
    """
    A manager for customizing the TUI layout for a session.

    You can register `LayoutHandler` callbacks to be invoked when layout customization
    is required. They will be provided a `LayoutManager` argument to customize the layout.
    """

    def add_callback(self, callback: LayoutHandler):
        """
        Register a `LayoutHandler` to be invoked for `mudpuppy_core.EventType.NewSession`
        and `mudpuppy_core.EventType.ResumeSession` events.

        Crucially, unlike global `mudpuppy.on_new_session` or `mudpuppy.on_new_session_or_reload`
        event handlers the `LayoutManager` handlers are invoked in the order they are added.

        This makes it possible to predictable order layout customizations from multiple
        sources by controlling the order `add_callback()` is used.
        """
        ...

    def remove_callback(self, callback: LayoutHandler):
        """
        Remove a previously `LayoutHandler` callback previously registered with `add_callback()`
        """
        ...

    def get_constraint(
        self, sesh_id: mudpuppy_core.SessionId, section_name: str
    ) -> mudpuppy_core.Constraint:
        """
        Return the `mudpuppy_core.Constraint` for the given section name in the layout for the session,
        or raise an exception if the section name doesn't exist.

        The returned `mudpuppy_core.Constraint` is mutable and can be updated by the caller.
        """
        ...

    def hide_section(self, sesh_id: mudpuppy_core.SessionId, section_name: str):
        """
        Hide the section with the given name in the layout for the session, or raise an exception if the
        section name doesn't exist.

        This will remove the section from the layout until `show_section()` is invoked.
        Internally the sections' `mudpuppy_core.Constraint` will be replaced with a
        `mudpuppy_core.Constraint` with a `max` of `0`.
        """
        ...

    def show_section(
        self,
        sesh_id: mudpuppy_core.SessionId,
        section_name: str,
        constraint: mudpuppy_core.Constraint,
    ):
        """
        Show the section with the given name in the layout for the session, or raise an exception if the
        section name doesn't exist.

        The section's constraint will be restored to the provided `mudpuppy_core.Constraint`.
        """
        ...

    async def extend_section(
        self,
        sesh_id: mudpuppy_core.SessionId,
        *,
        section_name: str,
        new_section_name: str,
        constraint: mudpuppy_core.Constraint,
        direction: mudpuppy_core.Direction = mudpuppy_core.Direction.Vertical,
        margin: int = 0,
        buffer_config: Optional[mudpuppy_core.BufferConfig] = None,
    ) -> Optional[mudpuppy_core.BufferId]:
        """
        Extend the `section_name` section in the `direction` specified by adding a
        `new_section_name`, with size described by `constraint`. Pre-existing subsections
        of `section_name` are not resized. See `split_section()` if you want to subdivide
        an existing section by resizing the existing content.

        An optional `margin` can be specified to separate it from the pre-existing
        sections.

        If `buffer_config` is provided, a new `mudpuppy_core.ExtraBuffer` will be created
        with the provided `mudpuppy_core.BufferConfig` and assigned to the `new_section_name`
        by setting `mudpuppy_core.BufferConfig.layout_name` to `new_section_name`.
        The created `mudpuppy_core.BufferId` is returned to the caller.
        """
        ...

    async def split_section(
        self,
        sesh_id: mudpuppy_core.SessionId,
        *,
        section_name: str,
        constraint: mudpuppy_core.Constraint,
        old_section_first: bool = True,
        new_section_name: str,
        new_constraint: mudpuppy_core.Constraint,
        direction: mudpuppy_core.Direction = mudpuppy_core.Direction.Vertical,
        margin: int = 0,
        buffer_config: Optional[mudpuppy_core.BufferConfig] = None,
    ) -> Optional[mudpuppy_core.BufferId]:
        """
        Split the existing `section_name` section in the `direction` specified.
        A section containing both the pre-existing and new sections named
        `f"{section_name}_{new_section_name}"` will then hold two sections:
        `section_name` and `new_section_name` (when `old_section_first` is `True`), or
        `new_section_name` and `section_name` (when `old_section_first` is `False`). See
        `extend_section()` if you want to add a new section alongside an existing section.
         without subdividing existing sections.

        The direction of the section holding the two subsections is determined by `direction`.
        An optional `margin` can be specified to separate the sections from each other.

        The pre-existing section will be moved to a new subsection, maintaining the
        name `section_name`. Its size will be determined by `constraint`.

        A new section will be added alongside it with the name `new_section_name`. Its
        size will be determined by `new_constraint`.

        If `buffer_config` is provided, a new `mudpuppy_core.ExtraBuffer` will be created
        with the provided `mudpuppy_core.BufferConfig` and assigned to the `new_section_name`
        by setting `mudpuppy_core.BufferConfig.layout_name` to `new_section_name`.
        The created `mudpuppy_core.BufferId` is returned to the caller.
        """
        ...

manager: LayoutManager
"""
A global `LayoutManager` to use for registering custom `LayoutHandler` callbacks.

Typically you will `import` this `LayoutManager` instance to register your callbacks.

```python
from layout import LayoutManager, manager
import mudpuppy_core

async def layout_init(session: mudpuppy_core.SessionInfo, layout_manager: LayoutManager):
    ...

manager.add_callback(layout_init)
```
"""
