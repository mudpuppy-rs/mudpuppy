import logging

from mudpuppy_core import Event, EventType, mudpuppy_core

from mudpuppy import on_connected, on_event

CHARSET_OPTION = 42
REQUEST = 1
ACCEPTED = 2
REJECTED = 3

ACCEPTED_ENCODINGS = [b"UTF-8", b"ASCII", b"US-ASCII"]


class TelnetCharsetHandler:
    """
    A crude approximation of RFC 2066 - "TELNET CHARSET Option"
    """

    @staticmethod
    async def on_connect(session: int):
        logging.debug(
            f"charset: negotiating Telnet charset protocol for conn {session}"
        )
        await mudpuppy_core.request_enable_option(session, CHARSET_OPTION)

    @staticmethod
    async def raw_receive(session: int, data: bytes):
        logging.debug(f"charset: received data for conn {session}: {data}")
        if len(data) < 2 or data[0] != REQUEST:
            logging.debug(
                f"charset: received unknown request for conn {session}: {data}"
            )
            return

        offered = data[2:].split(b" ")
        logging.debug(f"charset: server offered: {offered}")
        for offer in offered:
            if offer in ACCEPTED_ENCODINGS:
                await mudpuppy_core.send_subnegotiation(
                    session, CHARSET_OPTION, bytes([ACCEPTED]) + offer
                )
                logging.debug(f"charset: accepted server's offer - {offer}")
                return

        await mudpuppy_core.send_subnegotiation(
            session, CHARSET_OPTION, bytes([REJECTED])
        )
        logging.debug("charset: rejected server's offer - none compatible")


@on_connected()
async def connected(event: Event):
    assert isinstance(event, Event.Connection)
    await TelnetCharsetHandler.on_connect(event.id)


@on_event(EventType.Subnegotiation)
async def telnet_subneg_receive(event: Event):
    assert isinstance(event, Event.Subnegotiation)
    if event.option != CHARSET_OPTION:
        return
    await TelnetCharsetHandler.raw_receive(event.id, bytes(event.data))


logging.debug("telnet charset module loaded")
