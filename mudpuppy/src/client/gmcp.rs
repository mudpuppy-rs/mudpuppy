use pyo3::pyclass;
use serde::Serialize;
use serde_json;
use tracing::{debug, trace};

use crate::client::output;
use crate::error::GmcpError;
use crate::net::telnet;
use crate::net::telnet::codec::Item as TelnetItem;
use crate::{python, Result, CRATE_NAME, GIT_COMMIT_HASH};

#[derive(Debug)]
#[pyclass]
pub struct Gmcp {
    pub ready: bool,
    session_id: u32,
}

impl Gmcp {
    #[must_use]
    pub fn new(session_id: u32) -> Self {
        Self {
            ready: false,
            session_id,
        }
    }

    pub fn register(&self, module: &str) -> Result<TelnetItem> {
        debug!("Core.Supports.Add [{module} 1]");
        self.encode("Core.Supports.Add", [format!("{module} 1")])
    }

    pub fn unregister(&self, module: &str) -> Result<TelnetItem> {
        debug!("Core.Supports.Remove [{module} 1]");
        self.encode("Core.Supports.Remove", [format!("{module} 1")])
    }

    pub fn encode(&self, module: &str, data: impl Serialize) -> Result<TelnetItem> {
        self.encode_json(
            module,
            &serde_json::to_string(&data).map_err(GmcpError::BadJson)?,
        )
    }

    pub fn encode_json(&self, module: &str, json: &str) -> Result<TelnetItem> {
        if !self.ready {
            return Err(GmcpError::NotReady.into());
        }
        let subneg_data = format!("{module} {json}");
        Ok(TelnetItem::Subnegotiation(
            telnet::option::GMCP,
            subneg_data.into(),
        ))
    }

    pub fn handle_negotiation(
        &mut self,
        neg: telnet::codec::Negotiation,
    ) -> (Option<TelnetItem>, Option<python::Event>) {
        match neg {
            telnet::codec::Negotiation::Will(telnet::option::GMCP) => {
                trace!("GMCP enabled.");
                self.ready = true;
                (
                    Some(self.hello()),
                    Some(python::Event::GmcpEnabled {
                        id: self.session_id,
                    }),
                )
            }
            telnet::codec::Negotiation::Wont(telnet::option::GMCP) => {
                self.ready = false;
                (
                    None,
                    Some(python::Event::GmcpDisabled {
                        id: self.session_id,
                    }),
                )
            }
            _ => (None, None),
        }
    }

    pub fn decode(&self, raw_data: &[u8]) -> Result<Option<Message>> {
        let raw_data = String::from_utf8(raw_data.to_vec()).map_err(GmcpError::BadEncoding)?;

        let (package, json_data) = raw_data
            .split_once(' ')
            .ok_or_else(|| GmcpError::BadData("malformed subnegotiation".to_string()))?;

        Ok(Some(Message {
            session_id: self.session_id,
            package: package.to_string(),
            json: json_data.to_string(),
        }))
    }

    fn hello(&self) -> TelnetItem {
        // Safety: we know this data is well-formed and will serialize without err.
        self.encode(
            "Core.Hello",
            serde_json::json!({
                "client": CRATE_NAME,
                "version": GIT_COMMIT_HASH,
            }),
        )
        .unwrap()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Message {
    pub session_id: u32,
    pub package: String,
    pub json: String,
}

impl From<Message> for python::Event {
    fn from(msg: Message) -> Self {
        python::Event::GmcpMessage {
            id: msg.session_id,
            package: msg.package,
            json: msg.json,
        }
    }
}

impl From<Message> for output::Item {
    fn from(msg: Message) -> Self {
        output::Item::Debug {
            line: format!("GMCP: {} {}", msg.package, msg.json),
        }
    }
}
