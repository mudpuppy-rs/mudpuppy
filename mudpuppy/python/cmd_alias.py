import logging
from argparse import Namespace

from mudpuppy_core import (
    AliasConfig,
    Event,
    OutputItem,
    mudpuppy_core,
)
from commands import Command, add_command
from mudpuppy import on_new_session
from cformat import cformat


class AliasCmd(Command):
    def __init__(self, session_id: int):
        super().__init__("alias", session_id, self.run, "Manage aliases")
        subparsers = self.parser.add_subparsers(
            required=True,
        )

        list_parser = subparsers.add_parser(
            "list",
            help="List aliases",
            exit_on_error=False,
            add_help=False,
        )
        list_parser.add_argument(
            "--verbose", action="store_true", help="Verbose output"
        )
        list_parser.add_argument("--pattern", help="Filter by pattern")
        list_parser.set_defaults(func=self.list)
        list_parser.error = Command.on_error

        add_parser = subparsers.add_parser(
            "add",
            help="Add an alias",
            exit_on_error=False,
            add_help=False,
        )
        add_parser.add_argument("--name", help="Alias name", required=True)
        add_parser.add_argument("--pattern", help="Regex pattern", required=True)
        add_parser.add_argument("command", nargs="+", help="Content to expand alias to")
        add_parser.set_defaults(func=self.add)
        add_parser.error = Command.on_error

        remove_parser = subparsers.add_parser(
            "remove",
            help="Remove an alias",
            exit_on_error=False,
            add_help=False,
        )
        remove_parser.add_argument("alias_id", type=int, help="Alias ID to remove")
        remove_parser.set_defaults(func=self.remove)
        remove_parser.error = Command.on_error

        disable_parser = subparsers.add_parser(
            "disable",
            help="Disable an alias",
            exit_on_error=False,
            add_help=False,
        )
        disable_parser.add_argument("alias_id", type=int, help="Alias ID to disable")
        disable_parser.set_defaults(func=self.disable)
        disable_parser.error = Command.on_error

        enable_parser = subparsers.add_parser(
            "enable",
            help="Enable an alias",
            exit_on_error=False,
            add_help=False,
        )
        enable_parser.add_argument("alias_id", type=int, help="Alias ID to enable")
        enable_parser.set_defaults(func=self.enable)
        enable_parser.error = Command.on_error

    async def run(self, sesh_id: int, args: Namespace):
        logging.debug(f"args: {args}")
        if hasattr(args, "func"):
            await args.func(sesh_id, args)
        else:
            await self.display_help(sesh_id)

    async def add(self, sesh_id: int, args: Namespace):
        if args.pattern is None:
            return
        new_alias = AliasConfig(
            args.pattern, args.name, expansion=" ".join(args.command)
        )
        alias_id = mudpuppy_core.new_alias(sesh_id, new_alias, __name__)
        await mudpuppy_core.add_output(
            sesh_id,
            OutputItem.command_result(f"Created alias {alias_id}"),
        )

    async def remove(self, sesh_id: int, args: Namespace):
        await mudpuppy_core.remove_alias(sesh_id, args.alias_id)
        await mudpuppy_core.add_output(
            sesh_id, OutputItem.command_result(f"Removed alias {args.alias_id}")
        )

    async def disable(self, sesh_id: int, args: Namespace):
        await mudpuppy_core.disable_alias(sesh_id, args.alias_id)
        await mudpuppy_core.add_output(
            sesh_id, OutputItem.command_result(f"Disabled alias {args.alias_id}")
        )

    async def enable(self, sesh_id: int, args: Namespace):
        await mudpuppy_core.enable_alias(sesh_id, args.alias_id)
        await mudpuppy_core.add_output(
            sesh_id, OutputItem.command_result(f"Enabled alias {args.alias_id}")
        )

    async def list(self, sesh_id: int, _args: Namespace):
        aliases = await mudpuppy_core.aliases(sesh_id)
        output_items = []
        for alias in sorted(aliases, key=lambda a: a.id):
            label = alias.config.expansion
            if alias.config.callback is not None:
                label = str(alias.config.callback)
            prefix = "<green>"
            if not alias.enabled:
                prefix = "<red>"
            output_items.append(
                OutputItem.command_result(
                    cformat(
                        f"{prefix}{alias.id}: Enabled={alias.enabled} Hits={alias.config.hit_count} {repr(alias.config.pattern())} -> {repr(label)}<reset>"
                    ),
                )
            )
        await mudpuppy_core.add_outputs(sesh_id, output_items)


@on_new_session()
async def setup_session(event: Event):
    assert isinstance(event, Event.NewSession)
    add_command(event.id, AliasCmd(event.id))
