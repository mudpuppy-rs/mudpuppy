"""
The `mudpuppy_core` module offers low-level access to Mudpuppy.
"""

# Defined explicitly to control rendered order in docs.
__all__ = [
    "mudpuppy_core",
    "MudpuppyCore",
    "Config",
    "KeyBindings",
    "Shortcut",
    "SessionInfo",
    "SessionId",
    "Mud",
    "Tls",
    "MudLine",
    "InputLine",
    "Event",
    "EventType",
    "EventHandlers",
    "Trigger",
    "TriggerConfig",
    "Alias",
    "AliasConfig",
    "AliasId",
    "TimerConfig",
    "Timer",
    "TimerId",
    "PromptSignal",
    "PromptMode",
    "Status",
    "StreamInfo",
    "OutputItem",
    "Input",
    "EchoState",
    "LayoutNode",
    "Constraint",
    "Direction",
    "BufferConfig",
    "BufferDirection",
    "BufferId",
    "ExtraBuffer",
]

from typing import Optional, List

class Config:
    """
    Read-only access to the Mudpuppy config.
    """

    ...

SessionId = int
"""
A `SessionInfo` identifier.
"""

AliasId = int
"""
An `Alias` identifier.
"""

TriggerId = int
"""
A `Trigger` identifier.
"""

TimerId = int
"""
A `Timer` identifier.
"""

BufferId = int
"""
An `ExtraBuffer` identifier.
"""


class SessionInfo:
    """
    Information about a session.

    """
    ...

class MudpuppyCore:
    def config(self) -> Config:
        """
        Returns a `Config` instance.

        Note that when the configuration changes old `Config` instances
        are not automatically updated. You should use a
        `EventType.ConfigReloaded` event handler to respond to updates
        to `Config`.
        """
        ...

    @staticmethod
    def config_dir() -> str:
        """
        Returns the path to the Mudpuppy configuration directory.
        """
        ...

    @staticmethod
    def data_dir() -> str:
        """
        Returns the path to the Mudpuppy data directory.
        """
        ...

    @staticmethod
    def name(self) -> str:
        """
        Returns the name of the program.
        """
        ...

    @staticmethod
    def version(self) -> str:
        """
        Returns the version of the program.
        """
        ...

    async def print(self, *args, sep: Optional[str]=None, end: Optional[str]=None):
        """
        Outputs each line of the rendered arguments as debug output items in
        the currently active mudpuppy session (if any).

        The arguments and behaviour match that of `builtins.print`.

        :param args: arguments to be printed
        :param sep: an optional separator to use between `args`.
        :param end: an optional ending string to use after `args`.
        """

    async def active_session(self) -> Optional[SessionId]:
        """
        Returns the ID of the currently active session, or `None` if no session
        is active.
        """
        ...

    async def sessions(self) -> List[SessionInfo]:
        """
        Returns a list of `SessionInfo` instances for all sessions.
        """
        ...

class KeyBindings: ...
class Shortcut: ...
class Mud: ...
class Tls: ...
class MudLine: ...
class InputLine: ...
class Event: ...
class EventType: ...
class EventHandlers: ...
class Trigger: ...
class TriggerConfig: ...
class Alias: ...
class AliasConfig: ...
class TimerConfig: ...
class Timer: ...
class PromptSignal: ...
class PromptMode: ...
class Status: ...
class StreamInfo: ...
class OutputItem: ...
class Input: ...
class EchoState: ...
class LayoutNode: ...
class Constraint: ...
class Direction: ...
class BufferConfig: ...
class BufferDirection: ...
class ExtraBuffer: ...

mudpuppy_core: MudpuppyCore
"""
A `MudpuppyCore` instance for interacting with the client.

It is automatically set up when Mudpuppy is running and has loaded
your scripts.

You will typically want to call functions on the `mudpuppy_core.mudpuppy_core`
instance to interact with the client. For example,

```python
from mudpuppy_core import mudpuppy_core
version = mudpuppy_core.version()
print(f"running {version}")
```
"""
