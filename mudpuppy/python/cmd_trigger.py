import logging
from argparse import Namespace

from commands import Command, add_command
from mudpuppy_core import (
    Event,
    OutputItem,
    SessionId,
    TriggerConfig,
    TriggerId,
    mudpuppy_core,
)

from mudpuppy import on_new_session


class TriggerCmd(Command):
    def __init__(self, sesh_id: SessionId):
        super().__init__("trigger", sesh_id, self.run, "Manage triggers")
        subparsers = self.parser.add_subparsers(
            required=True,
        )

        list_parser = subparsers.add_parser(
            "list",
            help="List triggers",
            exit_on_error=False,
            add_help=False,
        )
        list_parser.add_argument(
            "--verbose", action="store_true", help="Verbose output"
        )
        list_parser.add_argument("--pattern", help="Filter by pattern")
        list_parser.add_argument(
            "--gag", action="store_true", help="Filter by gag triggers"
        )
        list_parser.set_defaults(func=self.list)
        list_parser.error = Command.on_error

        add_parser = subparsers.add_parser(
            "add",
            help="Add a trigger",
            exit_on_error=False,
            add_help=False,
        )
        add_parser.add_argument("--name", help="Trigger name", required=True)
        add_parser.add_argument("--pattern", help="Regex pattern", required=True)
        add_parser.add_argument(
            "--gag",
            help="Gag matching line",
            action="store_true",
        )
        add_parser.add_argument(
            "--ansi",
            help="Match trigger with ANSI colour preserved",
            action="store_true",
        )
        add_parser.add_argument("command", nargs="*", help="Content to send on match")
        add_parser.set_defaults(func=self.add)
        add_parser.error = Command.on_error

        remove_parser = subparsers.add_parser(
            "remove",
            help="Remove a trigger",
            exit_on_error=False,
            add_help=False,
        )
        remove_parser.add_argument("trigger_id", type=int, help="Trigger ID to remove")
        remove_parser.set_defaults(func=self.remove)
        remove_parser.error = Command.on_error

        disable_parser = subparsers.add_parser(
            "disable",
            help="Disable a trigger",
            exit_on_error=False,
            add_help=False,
        )
        disable_parser.add_argument(
            "trigger_id", type=int, help="Trigger ID to disable"
        )
        disable_parser.set_defaults(func=self.disable)
        disable_parser.error = Command.on_error

        enable_parser = subparsers.add_parser(
            "enable",
            help="Enable a trigger",
            exit_on_error=False,
            add_help=False,
        )
        enable_parser.add_argument("trigger_id", type=int, help="Trigger ID to enable")
        enable_parser.set_defaults(func=self.enable)
        enable_parser.error = Command.on_error

    async def run(self, sesh_id: SessionId, args: Namespace):
        logging.debug(f"args: {args}")
        if hasattr(args, "func"):
            await args.func(sesh_id, args)
        else:
            self.display_help(sesh_id)

    async def add(self, sesh_id: SessionId, args: Namespace):
        if args.pattern is None:
            return
        new_trigger = TriggerConfig(args.pattern, args.name)
        new_trigger.expansion = " ".join(args.command)
        if args.gag:
            new_trigger.gag = True
        if not args.ansi:
            new_trigger.strip_ansi = True
        trig_id = await mudpuppy_core.new_trigger(sesh_id, new_trigger, __name__)
        await mudpuppy_core.add_output(
            sesh_id, OutputItem.command_result(f"Created trigger {trig_id}")
        )

    async def remove(self, sesh_id: SessionId, args: Namespace):
        await mudpuppy_core.remove_trigger(sesh_id, TriggerId(args.trigger_id))
        await mudpuppy_core.add_output(
            sesh_id,
            OutputItem.command_result(f"Removed trigger {args.trigger_id}"),
        )

    async def disable(self, sesh_id: SessionId, args: Namespace):
        await mudpuppy_core.disable_trigger(sesh_id, TriggerId(args.trigger_id))
        await mudpuppy_core.add_output(
            sesh_id,
            OutputItem.command_result(f"Disabled trigger {args.trigger_id}"),
        )

    async def enable(self, sesh_id: SessionId, args: Namespace):
        await mudpuppy_core.enable_trigger(sesh_id, TriggerId(args.trigger_id))
        await mudpuppy_core.add_output(
            sesh_id,
            OutputItem.command_result(f"Enabled trigger {args.trigger_id}"),
        )

    async def list(self, sesh_id: SessionId, args: Namespace):
        triggers = await mudpuppy_core.triggers(sesh_id)
        output_items = []
        for trigger in sorted(triggers, key=lambda t: t.id):
            if args.gag and not trigger.config.gag:
                continue

            if trigger.config.highlight is None:
                label = trigger.config.expansion
                if trigger.config.callback is not None:
                    label = str(trigger.config.callback)
                label = f"Action={repr(label)}"
            else:
                label = f"Highlight={str(trigger.config.highlight)}"

            output_items.append(
                OutputItem.command_result(
                    f"{trigger.id}: Hits={trigger.config.hit_count} Gag={trigger.config.gag} Pattern={repr(trigger.config.pattern())} {label}",
                )
            )
        await mudpuppy_core.add_outputs(sesh_id, output_items)


@on_new_session()
async def setup(event: Event):
    add_command(event.id, TriggerCmd(event.id))
