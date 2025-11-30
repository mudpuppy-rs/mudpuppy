import logging

from pup import Session, Event

from pup_events import session_connected, telnet_subnegotiation

CHARSET_OPTION = 42
REQUEST = 1
ACCEPTED = 2
REJECTED = 3

ACCEPTED_ENCODINGS = [b"UTF-8", b"ASCII", b"US-ASCII"]


logging.debug("module loaded")


@session_connected()
async def enable_charset(sesh: Session, _: Event):
    logging.debug(f"{sesh} requesting telnet charset option")
    sesh.telnet().request_enable_option(CHARSET_OPTION)


@telnet_subnegotiation(option=CHARSET_OPTION)
async def charset_option(sesh: Session, ev: Event):
    if len(ev.data) < 2 or ev.data[0] != REQUEST:
        logging.warning(f"received unknown charset request: {ev.data}")
        return

    telnet = sesh.telnet()
    offered = ev.data[2:].split(b" ")
    logging.debug(f"{sesh} server offered: {offered}")
    for offer in offered:
        if offer in ACCEPTED_ENCODINGS:
            telnet.send_subnegotiation(CHARSET_OPTION, bytes([ACCEPTED]) + offer)
            # TODO(XXX): keep track of the negotiated charset?
            logging.info(f"{sesh} accepted server's offer - {offer}")
            return

    telnet.send_subnegotiation(CHARSET_OPTION, bytes([REJECTED]))
    logging.debug(f"{sesh} rejected server's offer - none compatible")
