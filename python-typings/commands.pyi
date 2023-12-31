from argparse import ArgumentParser, Namespace
from mudpuppy_core import AliasId as AliasId, SessionId
from typing import Awaitable, Callable

CommandCallable = Callable[[SessionId, Namespace], Awaitable[None]]
last_error: str | None

class Command:
    name: str
    session: SessionId
    aliases: list[str]
    handler: CommandCallable
    parser: ArgumentParser
    def __init__(self, name: str, session: SessionId, handler: CommandCallable, description: str | None = None, aliases: list[str] | None = None) -> None: ...
    @staticmethod
    def on_error(message) -> None: ...
    def display_help(self, sesh_id: SessionId): ...
    async def invoke(self, sesh_id: SessionId, args: str): ...

def add_command(sesh_id: SessionId, command: Command): ...
def all_commands(sesh_id: SessionId) -> list[Command]: ...

commands: dict[SessionId, dict[str, Command]]
