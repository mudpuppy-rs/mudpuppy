//! Adapted from libmudtelnet (MIT), attempting to roughly emulate Blightmud's telnet negotiation.
//!
//! In general, I'm skeptical of this logic. It seems like something closer to RFC 1143 and the
//! "Q Method" would be more appropriate. Deferring a rewrite for another day :-)
use std::fmt::{Debug, Formatter};

use crate::net::telnet::codec::Negotiation;

/// A table of options that are supported locally or remotely and their current state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Table {
    options: [Entry; TABLE_SIZE],
}

impl Default for Table {
    fn default() -> Self {
        Self {
            options: [Entry::default(); TABLE_SIZE],
        }
    }
}

impl Table {
    /// Reset all negotiated states
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        for opt in &mut self.options {
            opt.clear_local_enabled();
            opt.clear_remote_enabled();
        }
    }

    pub fn request_enable_option(&mut self, option: u8) -> Option<Negotiation> {
        let entry = self.option_mut(option);
        entry.set_local_support();
        entry.set_remote_support();
        match entry.remote_enabled() {
            false => Some(Negotiation::Do(option)),
            true => None,
        }
    }

    pub fn request_disable_option(&mut self, option: u8) -> Option<Negotiation> {
        let entry = self.option_mut(option);
        entry.clear_local_support();
        entry.clear_remote_support();
        match entry.remote_enabled() {
            false => None,
            true => Some(Negotiation::Dont(option)),
        }
    }

    pub fn reply_enable_if_supported(
        &mut self,
        option: u8,
        replying_to_will: bool,
    ) -> Option<Negotiation> {
        let entry = self.option_mut(option);
        match entry.local_support() && !entry.local_enabled() {
            true => {
                entry.set_local_enabled();
                Some(match replying_to_will {
                    true => Negotiation::Do(option),    // Send DO for WILL
                    false => Negotiation::Will(option), // Send WILL for DO
                })
            }
            false => None,
        }
    }

    pub fn reply_disable_if_enabled(
        &mut self,
        option: u8,
        replying_to_wont: bool,
    ) -> Option<Negotiation> {
        let entry = self.option_mut(option);
        match entry.local_enabled() {
            true => {
                entry.clear_local_enabled();
                Some(match replying_to_wont {
                    true => Negotiation::Dont(option),  // Send DONT for WONT
                    false => Negotiation::Wont(option), // Send WONT for DONT
                })
            }
            false => None,
        }
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn enabled_locally(&self) -> Vec<u8> {
        self.options
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                if entry.local_enabled() {
                    // Safety: the table is a fixed size with indexes in range of u8.
                    Some(u8::try_from(i).unwrap())
                } else {
                    None
                }
            })
            .collect()
    }

    #[must_use]
    pub fn option(&self, opt: u8) -> &Entry {
        &self.options[opt as usize]
    }

    fn option_mut(&mut self, opt: u8) -> &mut Entry {
        &mut self.options[opt as usize]
    }
}

impl<I> From<I> for Table
where
    I: IntoIterator<Item = u8>,
{
    fn from(supported: I) -> Self {
        let mut table = Self::default();
        for opt in supported {
            table.option_mut(opt).set_local_support();
            table.option_mut(opt).set_remote_support();
        }
        table
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Entry(u8);

impl Entry {
    /// Option is locally supported.
    const SUPPORT_LOCAL: u8 = 1;
    /// Option is remotely supported.
    const SUPPORT_REMOTE: u8 = 1 << 1;
    /// Option is currently enabled locally.
    const LOCAL_STATE: u8 = 1 << 2;
    /// Option is currently enabled remotely.
    const REMOTE_STATE: u8 = 1 << 3;

    #[must_use]
    pub fn new(supported: bool) -> Self {
        let mut entry = Self::default();
        if supported {
            entry.set_local_support();
            entry.set_remote_support();
        }
        entry
    }

    #[must_use]
    pub fn local_support(self) -> bool {
        self.0 & Entry::SUPPORT_LOCAL == Entry::SUPPORT_LOCAL
    }

    pub fn set_local_support(&mut self) {
        self.0 |= Entry::SUPPORT_LOCAL;
    }

    pub fn clear_local_support(&mut self) {
        self.0 &= !Entry::SUPPORT_LOCAL;
    }

    #[must_use]
    pub fn remote_support(self) -> bool {
        self.0 & Entry::SUPPORT_REMOTE == Entry::SUPPORT_REMOTE
    }

    pub fn set_remote_support(&mut self) {
        self.0 |= Entry::SUPPORT_REMOTE;
    }

    pub fn clear_remote_support(&mut self) {
        self.0 &= !Entry::SUPPORT_REMOTE;
    }

    #[must_use]
    pub fn local_enabled(self) -> bool {
        self.0 & Entry::LOCAL_STATE == Entry::LOCAL_STATE
    }

    pub fn set_local_enabled(&mut self) {
        self.0 |= Entry::LOCAL_STATE;
    }

    pub fn clear_local_enabled(&mut self) {
        self.0 &= !Entry::LOCAL_STATE;
    }

    #[must_use]
    pub fn remote_enabled(self) -> bool {
        self.0 & Entry::REMOTE_STATE == Entry::REMOTE_STATE
    }

    pub fn set_remote_enabled(&mut self) {
        self.0 |= Entry::REMOTE_STATE;
    }

    pub fn clear_remote_enabled(&mut self) {
        self.0 &= !Entry::REMOTE_STATE;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

impl Debug for Entry {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Entry")
            .field("value", &self.0)
            .field("local_support", &self.local_support())
            .field("local_enabled", &self.local_enabled())
            .field("remote_support", &self.remote_support())
            .field("remote_enabled", &self.remote_enabled())
            .finish()
    }
}

impl From<Entry> for u8 {
    fn from(value: Entry) -> Self {
        value.0
    }
}

const TABLE_SIZE: usize = 1 + u8::MAX as usize;
