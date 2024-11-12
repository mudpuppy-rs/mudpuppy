import logging
from argparse import Namespace
from typing import Optional

from mudpuppy_core import (
    Event,
    OutputItem,
    SessionId,
    TimerConfig,
    TimerId,
    mudpuppy_core,
)
from commands import Command, add_command
from cformat import cformat
from mudpuppy import on_new_session


class TimerCmd(Command):
    def __init__(self, session_id: SessionId):
        super().__init__("timer", session_id, self.run, "Manage timers")
        subparsers = self.parser.add_subparsers(
            required=True,
        )

        list_parser = subparsers.add_parser(
            "list",
            help="List timers",
            exit_on_error=False,
            add_help=False,
        )
        list_parser.add_argument(
            "--verbose", action="store_true", help="Verbose output"
        )
        list_parser.set_defaults(func=self.list)
        list_parser.error = Command.on_error

        add_parser = subparsers.add_parser(
            "add",
            help="Add an alias",
            exit_on_error=False,
            add_help=False,
        )
        add_parser.add_argument("--name", help="Timer name", required=True)
        add_parser.add_argument(
            "--milliseconds", help="Milliseconds of delay", required=False
        )
        add_parser.add_argument("--seconds", help="Seconds of delay", required=False)
        add_parser.add_argument("--minutes", help="Minutes of delay", required=False)
        add_parser.add_argument("--hours", help="Hours of delay", required=False)
        add_parser.add_argument(
            "--max-ticks", help="Maximum number of ticks", required=False
        )
        add_parser.add_argument(
            "command", nargs="+", help="Command to run on timer tick"
        )
        add_parser.set_defaults(func=self.add)
        add_parser.error = Command.on_error

        stop_parser = subparsers.add_parser(
            "stop",
            help="Stop a timer",
            exit_on_error=False,
            add_help=False,
        )
        stop_parser.add_argument("timer_id", type=int, help="Timer ID to stop")
        stop_parser.set_defaults(func=self.stop)
        stop_parser.error = Command.on_error

        start_parser = subparsers.add_parser(
            "start",
            help="Start a timer",
            exit_on_error=False,
            add_help=False,
        )
        start_parser.add_argument("timer_id", type=int, help="Timer ID to start")
        start_parser.set_defaults(func=self.start)
        start_parser.error = Command.on_error

        remove_parser = subparsers.add_parser(
            "remove",
            help="Remove a timer",
            exit_on_error=False,
            add_help=False,
        )
        remove_parser.add_argument("timer_id", type=int, help="Timer ID to remove")
        remove_parser.set_defaults(func=self.remove)
        remove_parser.error = Command.on_error

    async def run(self, sesh_id: SessionId, args: Namespace):
        logging.debug(f"args: {args}")
        if hasattr(args, "func"):
            await args.func(sesh_id, args)
        else:
            self.display_help(sesh_id)

    async def stop(self, sesh_id: SessionId, args: Namespace):
        # TODO(XXX): check if already stopped, provide output if so.
        await mudpuppy_core.stop_timer(TimerId(args.timer_id))
        await mudpuppy_core.add_output(
            sesh_id, OutputItem.command_result(f"Stopped timer {args.timer_id}")
        )

    async def start(self, sesh_id: SessionId, args: Namespace):
        # TODO(XXX): check if already running, provide output if so.
        await mudpuppy_core.start_timer(TimerId(args.timer_id))
        await mudpuppy_core.add_output(
            sesh_id, OutputItem.command_result(f"Started timer {args.timer_id}")
        )

    async def remove(self, sesh_id: SessionId, args: Namespace):
        await mudpuppy_core.remove_timer(TimerId(args.timer_id))
        await mudpuppy_core.add_output(
            sesh_id, OutputItem.command_result(f"Removed timer {args.timer_id}")
        )

    async def list(self, sesh_id: SessionId, _args: Namespace):
        timers = await mudpuppy_core.timers()
        output_items = []
        for timer in sorted(timers, key=lambda a: a.id):
            prefix = "<green>"
            if not timer.running:
                prefix = "<red>"
            output_items.append(
                OutputItem.command_result(
                    cformat(
                        f"{prefix}{timer.id}: Running={timer.running} {timer.config}<reset>"
                    ),
                )
            )
        await mudpuppy_core.add_outputs(sesh_id, output_items)

    async def add(self, sesh_id: SessionId, args: Namespace):
        async def callback(_timer_id: TimerId, session_id: Optional[SessionId]):
            assert session_id is not None
            await mudpuppy_core.send_line(session_id, " ".join(args.command))

        total_delay_ms = (
            int(args.milliseconds or 0)
            + (int(args.seconds or 0) * 1000)
            + (int(args.minutes or 0) * 60 * 1000)
            + (int(args.hours or 0) * 3600 * 1000)
        )

        if total_delay_ms <= 0:
            raise ValueError("The timer duration must be greater than zero.")

        config = TimerConfig(args.name, total_delay_ms, callback, sesh_id)
        if args.max_ticks:
            config.max_ticks = int(args.max_ticks)
        timer_id = await mudpuppy_core.new_timer(config, __name__)
        await mudpuppy_core.add_output(
            sesh_id, OutputItem.command_result(f"Added timer {timer_id}")
        )


@on_new_session()
async def setup_session(event: Event):
    assert isinstance(event, Event.NewSession)
    add_command(event.id, TimerCmd(event.id))
