import logging

from mudpuppy_core import Event, EventType, SessionId, mudpuppy_core

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
    def on_connect(session: SessionId):
        logging.debug(
            f"charset: negotiating Telnet charset protocol for conn {session}"
        )
        mudpuppy_core.request_enable_option(session, CHARSET_OPTION)

    @staticmethod
    def raw_receive(session: SessionId, data: bytes):
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
                mudpuppy_core.send_subnegotiation(
                    session, CHARSET_OPTION, bytes([ACCEPTED]) + offer
                )
                logging.debug(f"charset: accepted server's offer - {offer}")
                return

        mudpuppy_core.send_subnegotiation(session, CHARSET_OPTION, bytes([REJECTED]))
        logging.debug("charset: rejected server's offer - none compatible")


@on_connected()
async def connected(event: Event):
    TelnetCharsetHandler.on_connect(event.id)


@on_event(EventType.Subnegotiation)
async def telnet_subneg_receive(event: Event):
    if event.option != CHARSET_OPTION:
        return
    TelnetCharsetHandler.raw_receive(event.id, bytes(event.data))


logging.debug("telnet charset module loaded")
