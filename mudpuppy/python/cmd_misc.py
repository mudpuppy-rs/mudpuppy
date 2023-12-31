import logging
from argparse import Namespace

from commands import Command, add_command
from mudpuppy_core import Event, OutputItem, SessionId, Status, mudpuppy_core

from mudpuppy import on_new_session


class ConnectCmd(Command):
    def __init__(self, session: SessionId):
        super().__init__("connect", session, self.connect, "Re-connect to the server")

    async def connect(self, sesh_id: SessionId, _args: Namespace):
        status = await mudpuppy_core.status(sesh_id)
        if not isinstance(status, Status.Disconnected):
            await mudpuppy_core.add_output(
                sesh_id, OutputItem.command_result("Already connected")
            )
            return

        logging.debug(f"Connecting for sesh ID {sesh_id}")
        await mudpuppy_core.connect(sesh_id)


class DisconnectCmd(Command):
    def __init__(self, session: SessionId):
        super().__init__(
            "disconnect", session, self.disconnect, "Disconnect from the server"
        )

    async def disconnect(self, sesh_id: SessionId, _args: Namespace):
        status = await mudpuppy_core.status(sesh_id)
        if not isinstance(status, Status.Connected):
            await mudpuppy_core.add_output(
                sesh_id, OutputItem.command_result("Not connected")
            )
            return

        logging.debug(f"Disconnecting sesh ID {sesh_id}")
        await mudpuppy_core.disconnect(sesh_id)


class QuitCmd(Command):
    def __init__(self, session: SessionId):
        super().__init__("quit", session, self.quit, "Quit Mudpuppy")

    async def quit(self, _sesh_id: SessionId, _args: Namespace):
        await mudpuppy_core.quit()


class ReloadCmd(Command):
    def __init__(self, session: SessionId):
        super().__init__("reload", session, self.reload, "Reload Mudpuppy")

    async def reload(self, _sesh_id: SessionId, _args: Namespace):
        await mudpuppy_core.reload()


@on_new_session()
async def setup(event: Event):
    add_command(event.id, ConnectCmd(event.id))
    add_command(event.id, DisconnectCmd(event.id))
    add_command(event.id, QuitCmd(event.id))
    add_command(event.id, ReloadCmd(event.id))
