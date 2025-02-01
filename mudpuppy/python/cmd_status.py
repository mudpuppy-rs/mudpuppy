import logging
from argparse import Namespace

from commands import Command, add_command
from mudpuppy_core import (
    Event,
    OutputItem,
    Status,
    StreamInfo,
    mudpuppy_core,
)

from mudpuppy import on_new_session


class StatusCmd(Command):
    def __init__(self, session: int):
        super().__init__("status", session, self.status, "Connection status")
        self.parser.add_argument(
            "--verbose",
            "-v",
            action="store_true",
            help="Show detailed status",
        )
        logging.debug(f"parser in Status = {self.parser}")

    async def status(self, sesh_id: int, args: Namespace):
        conn_status = await mudpuppy_core.status(sesh_id)
        items = []
        if args.verbose:
            items.extend(self.verbose_status(conn_status))
        else:
            items.append(self.simple_status(conn_status))
        await mudpuppy_core.add_outputs(sesh_id, items)

    def simple_status(self, status: Status):
        status_str = "Unknown"
        if isinstance(status, Status.Connected):
            status_str = "Connected"
        elif isinstance(status, Status.Connecting):
            status_str = "Connecting"
        elif isinstance(status, Status.Disconnected):
            status_str = "Disconnected"
        return OutputItem.command_result("Connection Status: " + status_str)

    def verbose_status(self, status: Status):
        items = [self.simple_status(status)]
        if not isinstance(status, Status.Connected):
            return items

        info = status.info
        assert isinstance(info, StreamInfo.Tcp) or isinstance(info, StreamInfo.Tls)
        items.append(OutputItem.command_result(f"IP: {info.ip}"))
        items.append(OutputItem.command_result(f"Port: {info.port}"))
        if not isinstance(info, StreamInfo.Tls):
            return items

        items.append(OutputItem.command_result(f"Protocol: {info.protocol}"))
        items.append(OutputItem.command_result(f"Ciphersuite: {info.ciphersuite}"))
        items.append(
            OutputItem.command_result(f"Verify Skipped: {info.verify_skipped}")
        )
        return items


@on_new_session()
async def setup(event: Event):
    assert isinstance(event, Event.NewSession)
    add_command(event.id, StatusCmd(event.id))
