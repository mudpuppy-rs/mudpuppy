import logging
from argparse import Namespace

from mudpuppy_core import (
    Event,
    OutputItem,
    TriggerConfig,
    mudpuppy_core,
)
from commands import Command, add_command
from cformat import cformat
from mudpuppy import on_new_session


class TriggerCmd(Command):
    def __init__(self, sesh_id: int):
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
            "--prompt",
            help="Only match prompt lines",
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

    async def run(self, sesh_id: int, args: Namespace):
        logging.debug(f"args: {args}")
        if hasattr(args, "func"):
            await args.func(sesh_id, args)
        else:
            await self.display_help(sesh_id)

    async def add(self, sesh_id: int, args: Namespace):
        if args.pattern is None:
            return
        new_trigger = TriggerConfig(
            args.pattern,
            args.name,
            gag=args.gag,
            prompt=args.prompt,
            strip_ansi=not args.ansi,
        )
        expansion = " ".join(args.command).strip()
        if expansion != "":
            new_trigger.expansion = expansion
        trig_id = await mudpuppy_core.new_trigger(sesh_id, new_trigger, __name__)
        await mudpuppy_core.add_output(
            sesh_id, OutputItem.command_result(f"Created trigger {trig_id}")
        )

    async def remove(self, sesh_id: int, args: Namespace):
        await mudpuppy_core.remove_trigger(sesh_id, args.trigger_id)
        await mudpuppy_core.add_output(
            sesh_id,
            OutputItem.command_result(f"Removed trigger {args.trigger_id}"),
        )

    async def disable(self, sesh_id: int, args: Namespace):
        await mudpuppy_core.disable_trigger(sesh_id, args.trigger_id)
        await mudpuppy_core.add_output(
            sesh_id,
            OutputItem.command_result(f"Disabled trigger {args.trigger_id}"),
        )

    async def enable(self, sesh_id: int, args: Namespace):
        await mudpuppy_core.enable_trigger(sesh_id, args.trigger_id)
        await mudpuppy_core.add_output(
            sesh_id,
            OutputItem.command_result(f"Enabled trigger {args.trigger_id}"),
        )

    async def list(self, sesh_id: int, args: Namespace):
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
            prefix = "<green>"
            if not trigger.enabled:
                prefix = "<red>"
            output_items.append(
                OutputItem.command_result(
                    cformat(
                        f"{prefix}{trigger.id}: Enabled={trigger.enabled} Hits={trigger.config.hit_count} Gag={trigger.config.gag} Prompt={trigger.config.prompt} Pattern={repr(trigger.config.pattern())} {label}<reset>"
                    ),
                )
            )
        await mudpuppy_core.add_outputs(sesh_id, output_items)


@on_new_session()
async def setup(event: Event):
    assert isinstance(event, Event.NewSession)
    add_command(event.id, TriggerCmd(event.id))
