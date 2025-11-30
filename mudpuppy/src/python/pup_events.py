import re
import inspect
import logging
from dataclasses import dataclass, field
from collections import defaultdict
from typing import Dict, DefaultDict, List, Tuple, Callable, Any, Awaitable, Optional

import pup
from pup import EventType, KeyEvent, Session, Event, Tab, Shortcut, PythonShortcut

EventHandler = Callable[[Session, Event], Awaitable[None]]
ShortcutHandler = Callable[[KeyEvent, Optional[Session], Tab], Awaitable[None]]
SlashCommand = Callable[[str, Session], Awaitable[None]]
SetupHandler = Callable[[Session], Awaitable[None]]
Filters = Dict[str, Any]


@dataclass
class Handlers:
    events: List[Tuple[EventType, Filters, EventHandler]] = field(default_factory=list)
    shortcuts: List[Tuple[KeyEvent, ShortcutHandler]] = field(default_factory=list)
    slash_commands: List[Tuple[str, SlashCommand]] = field(default_factory=list)
    setup: List[SetupHandler] = field(default_factory=list)


_modules: DefaultDict[str, Handlers] = defaultdict(Handlers)


def _require_coroutine(callback: Any):
    if not inspect.iscoroutinefunction(callback):
        raise TypeError(
            f"handler must be async function, got {type(callback).__name__}"
        )


def event(
    event_type: EventType, **filters: Any
) -> Callable[[EventHandler], EventHandler]:
    def decorator(func: EventHandler) -> EventHandler:
        _require_coroutine(func)
        _modules[func.__module__].events.append((event_type, filters, func))
        return func

    return decorator


def shortcut(key_event: KeyEvent) -> Callable[[ShortcutHandler], ShortcutHandler]:
    def decorator(func: ShortcutHandler) -> ShortcutHandler:
        _require_coroutine(func)
        _modules[func.__module__].shortcuts.append((key_event, func))
        return func

    return decorator


def command(name: str) -> Callable[[SlashCommand], SlashCommand]:
    def decorator(func: SlashCommand) -> SlashCommand:
        _require_coroutine(func)
        _modules[func.__module__].slash_commands.append((name, func))
        return func

    return decorator


def setup(func: SetupHandler) -> SetupHandler:
    _require_coroutine(func)
    _modules[func.__module__].setup.append(func)
    return func


async def _new_session(sesh: Session) -> None:
    for module_name, handlers in _modules.items():
        logging.debug(f"setting up {module_name} session {sesh}")
        for setup_handler in handlers.setup:
            await setup_handler(sesh)

        for event_type, filters, handler in handlers.events:
            # Force early binding by making handler/filters default parameters
            async def wrapper(
                s: Session,
                e: Event,
                event_type=event_type,
                handler=handler,
                filters=filters,
            ) -> None:
                for property, required_value in filters.items():
                    if getattr(e, property, None) != required_value:
                        logging.debug(
                            f"{event_type} handler {handler} wrapper skipping: event property {property} != {required_value}"
                        )
                        return
                await handler(s, e)

            sesh.add_event_handler(event_type, wrapper)

        if handlers.shortcuts:
            tab = await sesh.tab()
            for key_event, handler in handlers.shortcuts:
                tab.set_shortcut(key_event, Shortcut.Python(PythonShortcut(handler)))

        for name, handler in handlers.slash_commands:
            sesh.add_slash_command(name, handler)


_generated_decorators = []

for name, event_type in EventType.all().items():
    # Convert PascalCase to snake_case
    decorator_name = re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()

    def _event_decorator(
        event_type: EventType,
    ) -> Callable[[EventHandler], EventHandler]:
        def decorator(**filters: Any) -> Callable[[EventHandler], EventHandler]:
            return event(event_type, **filters)

        return decorator

    globals()[decorator_name] = _event_decorator(event_type)
    _generated_decorators.append(decorator_name)

pup.new_session_handler(_new_session)

__all__ = [
    "event",
    "shortcut",
    "setup",
    "commandEventType",
    "KeyEvent",
    "Session",
    "Event",
    "Tab",
] + _generated_decorators
