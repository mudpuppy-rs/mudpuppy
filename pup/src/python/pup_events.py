import re
from dataclasses import dataclass, field
from collections import defaultdict
from typing import Dict, DefaultDict, List, Tuple, Callable, Any, Awaitable, Optional

import pup
from pup import EventType, KeyEvent, Session, Event, Tab, Shortcut, PythonShortcut

EventHandler = Callable[[Session, Event], Awaitable[None]]
ShortcutHandler = Callable[[KeyEvent, Optional[Session], Tab], Awaitable[None]]
SetupHandler = Callable[[Session], Awaitable[None]]
EventFilters = Dict[str, Any]

@dataclass
class Handlers:
    events: List[Tuple[EventType, EventFilters, EventHandler]] = field(default_factory=list)
    shortcuts: List[Tuple[KeyEvent, ShortcutHandler]] = field(default_factory=list)
    setup: List[SetupHandler] = field(default_factory=list)

_modules: DefaultDict[str, Handlers] = defaultdict(Handlers)

def on_event(event_type: EventType, **filters: Any) -> Callable[[EventHandler], EventHandler]:
    def decorator(func: EventHandler) -> EventHandler:
        _modules[func.__module__].events.append((event_type, filters, func))
        return func
    return decorator

def on_shortcut(key_event: KeyEvent) -> Callable[[ShortcutHandler], ShortcutHandler]:
    def decorator(func: ShortcutHandler) -> ShortcutHandler:
        _modules[func.__module__].shortcuts.append((key_event, func))
        return func
    return decorator

def on_setup(func: SetupHandler) -> SetupHandler:
    _modules[func.__module__].setup.append(func)
    return func

def _event_decorator(event_type: EventType):
    def decorator(**filters: Any) -> Callable[[EventHandler], EventHandler]:
        return on_event(event_type, **filters)
    return decorator

_generated_decorators = []

for name in dir(EventType):
    if not name.startswith('_') and hasattr(EventType, name):
        # Convert PascalCase to snake_case
        snake_name = re.sub(r'(?<!^)(?=[A-Z])', '_', name).lower()
        decorator_name = f"on_{snake_name}"

        event_value = getattr(EventType, name)
        if isinstance(event_value, EventType):
            decorator_func = _event_decorator(event_value)

            # Add to module globals
            globals()[decorator_name] = decorator_func
            _generated_decorators.append(decorator_name)

__all__ = [
              'on_event', 'on_shortcut', 'on_setup',
              'EventType', 'KeyEvent', 'Session', 'Event', 'Tab',
          ] + _generated_decorators

async def setup_session(sesh: Session) -> None:
    for module_name, handlers in _modules.items():
        for setup_handler in handlers.setup:
            await setup_handler(sesh)

        for event_type, filters, handler in handlers.events:
            async def wrapper(s: Session, e: Event, h: EventHandler = handler, f: EventFilters = filters) -> None:
                if all(getattr(e, k, None) == v for k, v in f.items()):
                    await h(s, e)
            sesh.add_event_handler(event_type, wrapper)

        if handlers.shortcuts:
            tab = await sesh.tab()
            for key_event, handler in handlers.shortcuts:
                tab.set_shortcut(key_event, Shortcut.Python(PythonShortcut(handler)))

pup.new_session_handler(setup_session)
