import logging
import shlex
from argparse import ArgumentError, ArgumentParser, Namespace
from io import StringIO
from typing import Awaitable, Callable, Optional

from mudpuppy_core import AliasId, OutputItem, SessionId, mudpuppy_core

from mudpuppy import alias

__all__ = ["add_command", "all_commands", "Command", "CommandCallable"]

CommandCallable = Callable[[SessionId, Namespace], Awaitable[None]]

# This is a gross hack, but we can't call an async method to display
# the parser error from the `on_error` handler e set on the ArgumentParser.
last_error: Optional[str] = None


class Command:
    """
    An instance of a Mudpuppy "slash command".
    """

    def __init__(
        self,
        name: str,
        session: SessionId,
        handler: CommandCallable,
        description: Optional[str] = None,
        aliases: Optional[list[str]] = None,
    ):
        """
        Create an instance of a `Command` that is run for `/name`.

        `session` is the `mudpuppy_core.SessionId` that the command has been added for with `add_command`.

        `handler` is the `CommandCallable` to invoke when the command is run.

        `description` is an optional description of the command that can be shown in a command list.

        `aliases` is an optional list of other names the command should respond to in addition to `name`.
        """

        self.name = name
        self.session = session
        self.handler = handler
        self.parser = ArgumentParser(
            description=description, exit_on_error=False, add_help=False
        )
        self.parser.add_argument("--help", "-h", action="store_true")
        self.parser.error = lambda msg: self.on_error(msg)
        if aliases is not None:
            self.aliases = aliases
        else:
            self.aliases = []

    @staticmethod
    def on_error(message):
        """Called by the arg parser when an error occurs"""
        global last_error
        last_error = message
        logging.error(f"command error: {message}")

    def display_help(self, sesh_id: SessionId):
        """
        Called when the user requests help for this command.
        """
        file = StringIO()
        self.parser.print_help(file)
        for line in file.getvalue().split("\n"):
            mudpuppy_core.add_output(sesh_id, OutputItem.command_result(line))

    async def invoke(self, sesh_id: SessionId, args: str):
        """
        Invoke the command for the provided `mudpuppy_core.SessionId` by parsing `args` with the `Command`'s
        parser.
        """
        logging.debug(f"invoking in sesh {sesh_id}: cmd: {self.name} args: {args}")
        global last_error
        last_error = None
        try:
            cli_args = self.parser.parse_args(shlex.split(args))
            if cli_args.help:
                self.display_help(sesh_id)
                return
            logging.debug(f"args = {cli_args}")

            if last_error is not None:
                logging.error(f"not running: {last_error}")
                await mudpuppy_core.add_output(
                    sesh_id, OutputItem.failed_command_result(f"{last_error}")
                )
                if args == "":
                    self.display_help(sesh_id)
            else:
                await self.handler(sesh_id, cli_args)
        except ArgumentError as e:
            await mudpuppy_core.add_output(
                sesh_id,
                OutputItem.failed_command_result(f"Error parsing arguments: {e}"),
            )


def add_command(sesh_id: SessionId, command: Command):
    """
    Register the given `Command` as usable by the given `mudpuppy_core.SessionId`.
    """
    command_map = commands.get(sesh_id, {})
    command_map[command.name] = command
    for a in command.aliases:
        command_map[a] = command
    commands[sesh_id] = command_map


def all_commands(sesh_id: SessionId) -> list[Command]:
    """
    Returns a list of all `Command`s that have been registered for the given `mudpuppy_core.SessionId`
    with `add_command`.
    """
    return list(commands.get(sesh_id, {}).values())


# TODO(XXX): support removing commands
# TODO(XXX): support adding commands ahead of session ID (?)


@alias(pattern=r"^/([\w]+) ?(.*)?", name="Run a command")
async def __command_callback(
    session_id: SessionId, _alias_id: AliasId, _line: str, args: list[str]
):
    assert len(args) == 2

    command_map = commands.get(session_id, {})
    command = args[0]
    arguments = args[1]
    command_ob = command_map.get(command)
    if command_ob is None:
        await mudpuppy_core.add_output(
            session_id,
            OutputItem.failed_command_result(f"Unknown command {repr(command)}"),
        )
        return
    await command_ob.invoke(session_id, arguments)


logging.debug("commands: plugin loaded.")
commands: dict[SessionId, dict[str, Command]] = {}
