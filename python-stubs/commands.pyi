"""
The `commands` module offers functions and types for adding new `Command` instances
to mudpuppy. Typically these are called "slash commands" since the default command
prefix is a `/` character.

`Command`s are an alternative to `mudpuppy_core.Alias` instances and are helpful if you
want to do more complex argument parsing.
"""

# Defined explicitly to control rendered order in docs.
__all__ = ["CommandCallable", "Command", "add_command", "all_commands"]

import argparse
import mudpuppy_core
from typing import Callable, Awaitable, Optional

type CommandCallable = Callable[
    [mudpuppy_core.SessionId, argparse.Namespace], Awaitable[None]
]
"""
An async function that is called when input sent to a MUD matches the command prefix
and name of a command. Typically you will assign a `CommandCallable` to the `handler`
property of a `Command`.

The handler is called with:

* the `mudpuppy_core.SessionId` of the session that received the input.
* an [`argparse.Namespace`](https://docs.python.org/3/library/argparse.html#argparse.Namespace)
  with parsed arguments.

"""

class Command:
    """
    An instance of a Mudpuppy "slash command".

    See `add_command()`.
    """

    parser: argparse.ArgumentParser
    """
    An `argparse.ArgumentParser` that is used to parse arguments for this command.

    It is created with `exit_on_error=False` and `add_help=False` to allow for custom
    error handling and help display.
    """

    def __init__(
        self,
        name: str,
        session: mudpuppy_core.SessionId,
        handler: CommandCallable,
        description: Optional[str] = None,
        aliases: Optional[list[str]] = None,
    ):
        """
        Create an instance of a `Command` that is run for `/$NAME`.

        * `name` - The name of the command. This is what the user will type to trigger
          the command. E.g. `/$NAME`
        * `session` - The session ID that the command should be
           associated with using `add_command()`.`
        * `handler` - a `CommandCallable` to invoke when the command is run.
        * `description` - An optional description of the command that can be
          shown in a command list.
        * `aliases` - An optional list of other names the command should
           respond to in addition to `name`.
        """
        ...

    def display_help(self, sesh_id: mudpuppy_core.SessionId):
        """
        Called when the user requests help for this command.
        """

    @staticmethod
    def on_error(message):
        """Called by the arg parser when an error occurs"""
        ...

    async def invoke(self, sesh_id: mudpuppy_core.SessionId, args: str):
        """
        Invoke the command for the provided `mudpuppy_core.SessionId` by parsing `args` with the `Command`'s
        parser.
        """
        ...

def add_command(sesh_id: mudpuppy_core.SessionId, command: Command):
    """
    Register the given `Command` as usable by the given `mudpuppy_core.SessionId`.
    """
    ...

def all_commands(sesh_id: mudpuppy_core.SessionId) -> list[Command]:
    """
    Returns a list of all `Command`s that have been registered for the given `mudpuppy_core.SessionId`
    with `add_command()`.
    """
    ...
