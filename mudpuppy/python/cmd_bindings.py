import logging
from argparse import Namespace

from mudpuppy_core import (
    Event,
    OutputItem,
    SessionId,
    mudpuppy_core,
)
from commands import Command, add_command
from cformat import cformat
from mudpuppy import on_new_session


class BindingsCmd(Command):
    def __init__(self, sesh_id: SessionId):
        super().__init__("bindings", sesh_id, self.run, "View keybindings")
        subparsers = self.parser.add_subparsers(
            required=True,
        )

        list_parser = subparsers.add_parser(
            "list",
            help="List keybindings",
            exit_on_error=False,
            add_help=False,
        )
        list_parser.add_argument("--mode", help="Filter by input mode")
        list_parser.set_defaults(func=self.list)
        list_parser.error = Command.on_error

    async def run(self, sesh_id: SessionId, args: Namespace):
        logging.debug(f"args: {args}")
        if hasattr(args, "func"):
            await args.func(sesh_id, args)
        else:
            await self.display_help(sesh_id)

    async def list(self, sesh_id: SessionId, args: Namespace):
        keybindings = mudpuppy_core.config().keybindings()
        output_items = []

        for mode in sorted(keybindings.modes()):
            if args.mode is not None and args.mode.lower() != mode.lower():
                continue

            output_items.append(OutputItem.command_result(cformat(f"{mode} bindings:")))
            for input, action in keybindings.bindings(mode):
                output_items.append(
                    OutputItem.command_result(cformat(f"\t{input} -> {action}"))
                )

        await mudpuppy_core.add_outputs(sesh_id, output_items)


@on_new_session()
async def setup(event: Event):
    assert isinstance(event, Event.NewSession)
    add_command(event.id, BindingsCmd(event.id))
