import logging
import pup
from pup import Session, Event, EventType

CHARSET_OPTION = 42
REQUEST = 1
ACCEPTED = 2
REJECTED = 3

ACCEPTED_ENCODINGS = [b"UTF-8", b"ASCII", b"US-ASCII"]


async def on_new_session(sesh: Session):
    logging.debug(f"{sesh} setting up telnet charset handlers")
    sesh.add_event_handler(EventType.SessionConnected, enable_charset)
    sesh.add_event_handler(EventType.TelnetSubnegotiation, telnet_subnegotiation)


logging.debug("module loaded")
pup.new_session_handler(on_new_session)


async def enable_charset(sesh: Session, _: Event):
    logging.debug(f"{sesh} requesting telnet charset option")
    sesh.telnet().request_enable_option(CHARSET_OPTION)


async def telnet_subnegotiation(sesh: Session, ev: Event):
    # Ignore subneg data for other options
    if ev.option != CHARSET_OPTION:
        return

    logging.debug(f"{sesh} handling telnet subnegotiation")
    if len(ev.data) < 2 or ev.data[0] != REQUEST:
        logging.debug(f"received unknown charset request: {ev.data}")
        return

    telnet = sesh.telnet()
    offered = ev.data[2:].split(b" ")
    logging.debug(f"{sesh} server offered: {offered}")
    for offer in offered:
        if offer in ACCEPTED_ENCODINGS:
            telnet.send_subnegotiation(CHARSET_OPTION, bytes([ACCEPTED]) + offer)
            # TODO(XXX): keep track of the negotiated charset?
            logging.debug(f"{sesh} accepted server's offer - {offer}")
            return

    telnet.send_subnegotiation(CHARSET_OPTION, bytes([REJECTED]))
    logging.debug(f"{sesh} rejected server's offer - none compatible")
