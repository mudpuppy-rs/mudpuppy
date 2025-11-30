use serde::Serialize;
use tracing::debug;

use crate::config;
use crate::error::GmcpError;
use crate::net::telnet;
use crate::net::telnet::codec::Item as TelnetItem;
use crate::python;

pub(super) fn register(module: &str) -> Result<TelnetItem, GmcpError> {
    debug!(module, "registering");
    encode("Core.Supports.Add", [format!("{module} 1")])
}

pub(super) fn unregister(module: &str) -> Result<TelnetItem, GmcpError> {
    debug!(module, "unregistering");
    encode("Core.Supports.Remove", [format!("{module} 1")])
}

pub(super) fn encode_hello() -> TelnetItem {
    // Safety: we know this data is well-formed and will serialize without error.
    encode(
        "Core.Hello",
        serde_json::json!({
            "client": config::CRATE_NAME,
            "version": config::GIT_COMMIT_HASH,
        }),
    )
    .unwrap()
}

pub(super) fn encode(module: &str, data: impl Serialize) -> Result<TelnetItem, GmcpError> {
    let json_data = serde_json::to_string(&data).map_err(|_| GmcpError::InvalidJson)?;
    Ok(TelnetItem::Subnegotiation(
        telnet::option::GMCP,
        format!("{module} {json_data}").into(),
    ))
}

pub(super) fn decode(raw_data: &[u8]) -> Result<python::Event, GmcpError> {
    let raw_data = String::from_utf8(raw_data.to_vec()).map_err(|_| GmcpError::InvalidEncoding)?;
    let (package, json_data) = raw_data.split_once(' ').ok_or(GmcpError::Malformed)?;
    Ok(python::Event::GmcpMessage {
        package: package.to_string(),
        json: json_data.to_string(),
    })
}
