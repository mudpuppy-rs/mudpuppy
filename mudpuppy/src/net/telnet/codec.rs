//! Adapted from telnet codec impl found in Darksonn/telnet-chat[^1] (MIT).
//!
//! [^1]: <https://github.com/Darksonn/telnet-chat/blob/master/src/telnet.rs>

use std::mem;

use tokio_util::bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::error::Error;
use crate::net::telnet;

#[derive(Debug, Default)]
pub struct Codec {
    state: State,
    line_buffer: BytesMut,
}

impl Codec {
    #[must_use]
    pub fn new() -> Self {
        Codec {
            state: State::default(),
            line_buffer: BytesMut::with_capacity(1024),
        }
    }

    /// If the decoder is buffering a line, return the partial line, clearing the buffer.
    ///
    /// Returns `None` if there is no partial line content, or if the codec is not presently
    /// buffering a line.
    pub fn partial_line(&mut self) -> Option<Bytes> {
        match self.line_buffer.is_empty() {
            false => Some(self.line_buffer.split().freeze()),
            true => None,
        }
    }

    // TODO(XXX): this should be rolled into the parser operation to avoid the O(N) scan
    //   over the partial buffer each time we want to consider whether we can deframe
    //   a line.
    fn deframe_line(&mut self) -> Option<Item> {
        const EOL: &[u8] = b"\r\n";
        const REVERSE_EOL: &[u8] = b"\n\r"; // For compat w/ Aardwolf (Blightmud@11e78c3).

        // Note: this deframer logic is _very_ permissive. We intentionally allow deframing lines
        // delimited with:
        //  - \r\n (proper telnet EOL)
        //  - \n\r (lol - compat for Aardwolf (Blightmud@11e78c3)).
        // We may need to consider allowing just '\n'.
        if let Some(line_end) = self
            .line_buffer
            .windows(2)
            .position(|bytes| bytes == EOL || bytes == REVERSE_EOL)
        {
            // Split the BytesMut buffer at the line end index - self.buf keeps the
            // content after the line_end index, the parts before are moved into a Line
            // as a frozen Bytes.
            // All of this is O(1) and should not allocate.
            let line = Item::Line(self.line_buffer.split_to(line_end).freeze());
            // Consume the line ending we left behind after splitting the line.
            self.line_buffer.advance(2);
            return Some(line);
        }

        None
    }
}

impl Decoder for Codec {
    type Item = Item;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            if src.is_empty() {
                return Ok(None);
            }

            // If the first byte isn't an IAC escape character, buffer the byte as data
            // based on our current state.
            if src.first() != Some(&telnet::command::IAC) {
                self.state.buffer_data(&mut self.line_buffer, src.get_u8());
                // If we can, deframe a full line from what we've buffered.
                if let Some(line) = self.deframe_line() {
                    return Ok(Some(line));
                }
                continue;
            }

            // Otherwise, try and parse an IAC prefixed command sequence, advancing the buffer
            // beyond the parsed command.
            let res = try_parse_iac(src.chunk());
            src.advance(res.len());

            match res {
                // More data needed.
                ParseIacResult::NeedMore => return Ok(None),

                // Escaped data byte 0xFF.
                ParseIacResult::DataByte255 => {
                    self.state.buffer_data(&mut self.line_buffer, 0xFF);
                    // We know 0xFF isn't a line ending, or an SE, so we don't attempt
                    // to deframe anything after buffering this new data.
                }

                // Telnet command.
                ParseIacResult::Command(item) => match item {
                    // Begin a subnegotiation.
                    Command::SubnegotiationBegin => {
                        self.state.begin_subnegotiation()?;
                    }

                    // End a subnegotiation, returning the subnegotiation item.
                    Command::SubnegotiationEnd => {
                        return Ok(Some(self.state.end_subnegotiation()?))
                    }

                    // Pass through the command as an item. It's a Telnet negotiation or
                    // a command we don't handle ourselves (e.g. EOR, NOP).
                    item => return Ok(Some(Item::try_from(item)?)),
                },
            }
        }
    }
}

impl Encoder<Item> for Codec {
    type Error = Error;

    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.encode(dst);
        Ok(())
    }
}

/// An item of processed Telnet data.
#[derive(Debug)]
pub enum Item {
    /// A line of text data (without terminating `\r\n`).
    ///
    /// This is the most common item type, representing a line of text data received from the
    /// Telnet stream. The terminating `\r\n` is stripped from the line.
    Line(Bytes),

    /// A telnet negotiation command.
    Negotiation(Negotiation),

    /// A one-byte IAC prefixed telnet command that wasn't recognized as a negotiation command.
    ///
    /// For example, NOP (241), Go ahead (249). See [`telnet::command`] for helfpul known
    /// command constants.
    IacCommand(u8),

    /// A subnegotiation of the specified option, with associated subnegotiation data.
    ///
    /// For example, GMCP (201) and a GMCP payload. See [`telnet::option`] for helpful known
    /// option constants.
    Subnegotiation(u8, Bytes),
}

impl Item {
    /// Encode the telnet item into the given buffer.
    fn encode(&self, buf: &mut BytesMut) {
        match self {
            Item::Line(line) => {
                buf.put_slice(&escape_iac(line));
                buf.put_slice(b"\r\n");
            }
            Item::Negotiation(Negotiation::Will(opt)) => {
                buf.put_slice(&[telnet::command::IAC, telnet::command::WILL, *opt]);
            }
            Item::Negotiation(Negotiation::Wont(opt)) => {
                buf.put_slice(&[telnet::command::IAC, telnet::command::WONT, *opt]);
            }
            Item::Negotiation(Negotiation::Do(opt)) => {
                buf.put_slice(&[telnet::command::IAC, telnet::command::DO, *opt]);
            }
            Item::Negotiation(Negotiation::Dont(opt)) => {
                buf.put_slice(&[telnet::command::IAC, telnet::command::DONT, *opt]);
            }
            Item::IacCommand(cmd) => {
                buf.put_slice(&[telnet::command::IAC, *cmd]);
            }
            Item::Subnegotiation(opt, data) => {
                buf.put_slice(&[telnet::command::IAC, telnet::command::SB, *opt]);
                buf.put_slice(&escape_iac(data));
                buf.put_slice(&[telnet::command::IAC, telnet::command::SE]);
            }
        }
    }
}

// Some low-level telnet commands are translated directly into items.
// The subnegotiation markers are handled by the codec itself.
impl TryFrom<Command> for Item {
    type Error = Error;

    fn try_from(raw: Command) -> Result<Self, Self::Error> {
        Ok(match raw {
            Command::Will(opt) => Item::Negotiation(Negotiation::Will(opt)),
            Command::Wont(opt) => Item::Negotiation(Negotiation::Wont(opt)),
            Command::Do(opt) => Item::Negotiation(Negotiation::Do(opt)),
            Command::Dont(opt) => Item::Negotiation(Negotiation::Dont(opt)),
            Command::Other(cmd) => Item::IacCommand(cmd),
            _ => return Err(Error::Internal(format!("unexpected raw item: {raw:?}"))),
        })
    }
}

/// A telnet negotiation command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Negotiation {
    /// A "WILL" negotiation command for the specified option.
    ///
    /// "Indicates the desire to begin performing, or confirmation that you are now performing,
    /// the indicated option."
    ///
    /// See [`telnet::option`] for helpful known option constants.
    Will(u8),

    /// A "WONT" negotiation command for the specified option.
    ///
    /// "Indicates the refusal to perform, or continue performing, the indicated option."
    ///
    /// See [`telnet::option`] for helpful known option constants.
    Wont(u8),

    /// A "DO" negotiation command for the specified option.
    ///
    /// "Indicates the request that the other party perform, or confirmation that you are expecting
    /// the other party to perform, the indicated option."
    ///
    /// See [`telnet::option`] for helpful known option constants.
    Do(u8),

    /// A "DONT" negotiation command for the specified option.
    ///
    /// "Indicates the demand that the other party stop performing, or confirmation that you are no
    /// longer expecting the other party to perform, the indicated option."
    ///
    /// See [`telnet::option`] for helpful known option constants.
    Dont(u8),
}

impl Negotiation {
    #[must_use]
    pub fn option(&self) -> u8 {
        match self {
            Negotiation::Will(opt)
            | Negotiation::Wont(opt)
            | Negotiation::Do(opt)
            | Negotiation::Dont(opt) => *opt,
        }
    }
}

impl From<Negotiation> for Item {
    fn from(neg: Negotiation) -> Self {
        Item::Negotiation(neg)
    }
}

#[derive(Debug, Default)]
enum State {
    #[default]
    BufferingLine,
    SubnegotiationStart,
    Subnegotiation(u8, BytesMut),
}

impl State {
    fn buffer_data(&mut self, line_buffer: &mut BytesMut, byte: u8) {
        // TODO(XXX): impose buffer limits for lines/subnegotiations. After a certain threshold
        //  this should error.
        match self {
            State::BufferingLine => {
                line_buffer.put_u8(byte);
            }
            State::Subnegotiation(_, buf) => buf.put_u8(byte),
            State::SubnegotiationStart => {
                *self = State::Subnegotiation(byte, BytesMut::with_capacity(1024));
            }
        }
    }

    fn begin_subnegotiation(&mut self) -> Result<(), Error> {
        if !matches!(self, State::BufferingLine) {
            return Err(Error::Internal(format!("unexpected state: {self:?}")));
        }
        *self = State::SubnegotiationStart;
        Ok(())
    }

    fn end_subnegotiation(&mut self) -> Result<Item, Error> {
        if !matches!(self, State::Subnegotiation(_, _)) {
            return Err(Error::Internal(format!("unexpected state: {self:?}")));
        }
        let State::Subnegotiation(opt, buf) = mem::take(self) else {
            unreachable!();
        };
        Ok(Item::Subnegotiation(opt, buf.freeze()))
    }
}

/// Supported low level IAC prefixed telnet commands.
///
/// Some are passed through as-is as higher level [`Item`]s, while others are buffered and used
/// to drive the codec's state.
///
/// See [RFC 854](https://tools.ietf.org/html/rfc854) "TELNET COMMAND STRUCTURE"
/// for more information.
#[derive(Debug)]
enum Command {
    Will(u8),
    Wont(u8),
    Do(u8),
    Dont(u8),

    /// "Indicates what follows is subnegotiation of the indicated option."
    SubnegotiationBegin,

    /// "End of subnegotiation parameters."
    SubnegotiationEnd,

    /// Any other IAC prefixed Telnet command code. E.g. NOP, EOR, GA, etc.
    Other(u8),
}

impl Command {
    /// Length in bytes of the command, including the leading IAC.
    fn len(&self) -> usize {
        match self {
            Command::Will(_) | Command::Wont(_) | Command::Do(_) | Command::Dont(_) => 3,
            Command::SubnegotiationBegin | Command::SubnegotiationEnd | Command::Other(_) => 2,
        }
    }
}

/// Try to interpret the given bytes as a telnet IAC command.
fn try_parse_iac(bytes: &[u8]) -> ParseIacResult {
    // All IAC sequences should be at least two bytes.
    if bytes.len() < 2 {
        return ParseIacResult::NeedMore;
    }
    // Indexing safety: len checked above.
    let iac = bytes[0];
    let cmd = bytes[1];
    let option_code = bytes.get(2);

    // This function should only have been called when bytes is lead by an 0xFF IAC byte.
    debug_assert_eq!(iac, telnet::command::IAC);

    // Command sequences that require an option code should have one, or we need more data.
    if matches!(cmd, telnet::command::WILL..=telnet::command::WONT) && option_code.is_none() {
        return ParseIacResult::NeedMore;
    }

    match (cmd, option_code) {
        // Subnegotiation data is deframed at a higher level based on the SB/SE events.
        (telnet::command::SE, _) => Command::SubnegotiationEnd.into(),
        (telnet::command::SB, _) => Command::SubnegotiationBegin.into(),
        // Negotiation commands require a third byte for the option code. If it's `None` we need more data.
        (
            telnet::command::WILL
            | telnet::command::WONT
            | telnet::command::DO
            | telnet::command::DONT,
            None,
        ) => ParseIacResult::NeedMore,
        // Negotiation commands with their option code.
        (telnet::command::WILL, Some(opt)) => Command::Will(*opt).into(),
        (telnet::command::WONT, Some(opt)) => Command::Wont(*opt).into(),
        (telnet::command::DO, Some(opt)) => Command::Do(*opt).into(),
        (telnet::command::DONT, Some(opt)) => Command::Dont(*opt).into(),
        // An escaped 0xFF byte.
        (telnet::command::IAC, _) => ParseIacResult::DataByte255,
        _ => Command::Other(cmd).into(),
    }
}

/// Result from parsing an IAC prefixed command from a buffer.
enum ParseIacResult {
    /// The buffer didn't contain enough data to parse an IAC prefixed command.
    NeedMore,

    /// The buffer contained an IAC prefixed escaped data byte, 0xFF.
    DataByte255,

    /// The buffer contained an IAC prefixed Telnet command.
    Command(Command),
}

impl ParseIacResult {
    /// The number of bytes required to encode the parsed IAC result.
    fn len(&self) -> usize {
        match self {
            ParseIacResult::NeedMore => 0,
            ParseIacResult::DataByte255 => 1,
            ParseIacResult::Command(item) => item.len(),
        }
    }
}

impl From<Command> for ParseIacResult {
    fn from(cmd: Command) -> Self {
        ParseIacResult::Command(cmd)
    }
}

/// Escape IAC bytes in data that is to be transmitted and treated as a non-IAC sequence.
///
/// # Example
/// `[255, 1, 6, 2]` -> `[255, 255, 1, 6, 2]`
fn escape_iac(data: &Bytes) -> Bytes {
    let mut res = BytesMut::with_capacity(data.len());
    for byte in data {
        res.put_u8(*byte);
        if byte == &telnet::command::IAC {
            res.put_u8(telnet::command::IAC);
        }
    }
    res.freeze()
}
