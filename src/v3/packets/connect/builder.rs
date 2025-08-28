//! Builder for the [`Connect`] packet.

use crate::bytes::{Error, Input};
use crate::v3::packets::Qos;

use super::{ClientId, ConnectFlags, KeepAlive};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Will<'a> {
    pub(crate) topic: &'a str,
    pub(crate) message: &'a [u8],
}

/// Constructs a connect packet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectBuilder<'b> {
    pub(crate) client_id: &'b str,
    pub(crate) flags: ConnectFlags,
    pub(crate) keepalive: KeepAlive,
    pub(crate) will: Option<Will<'b>>,
    pub(crate) username: Option<&'b str>,
    pub(crate) password: Option<&'b [u8]>,
}

impl<'b> ConnectBuilder<'b> {
    /// Creates the builder from the given client id.
    pub fn create(client_id: &'b str) -> Result<Self, Error> {
        ClientId::validate_value(client_id)?;

        Ok(Self {
            client_id,
            flags: ConnectFlags::empty(),
            keepalive: KeepAlive::default(),
            will: None,
            username: None,
            password: None,
        })
    }

    /// Creates a builder with an empty client id.
    ///
    /// This will set the clean-session to true
    pub fn with_empty_client_id() -> Self {
        let this = Self {
            client_id: "",
            flags: ConnectFlags::empty(),
            will: None,
            username: None,
            password: None,
            keepalive: KeepAlive::default(),
        };

        this.clean_session()
    }

    /// Create a new builder without checking for a valid client ID.
    pub fn new_unchecked(client_id: &'b str) -> Self {
        Self {
            client_id,
            flags: ConnectFlags::empty(),
            will: None,
            username: None,
            password: None,
            keepalive: KeepAlive::default(),
        }
    }

    /// Sets the clean session flag for the connection.
    ///
    /// It's used to control the lifetime of the session state in the broker. Calling this function
    /// will set the clean session flag and session must be discarded for the broker.
    pub fn clean_session(mut self) -> Self {
        self.flags |= ConnectFlags::CLEAN_SESSION;

        self
    }

    /// Sets the keep alive for the connection
    pub fn keepalive(mut self, keepalive: KeepAlive) -> Self {
        self.keepalive = keepalive;

        self
    }

    /// Sets the will for the connection.
    ///
    /// If the client is disconnected for a network failure, the keep alive expires, protocol error, or the
    /// connection is closed without a DISCONNECT packet. The Server will publish the will message
    /// on the specified topic.
    pub fn will(&mut self, topic: &'b str, message: &'b [u8], qos: Qos, retain: bool) -> &mut Self {
        self.will = Some(Will { topic, message });

        self.flags |= ConnectFlags::WILL_FLAG;

        self.flags &= ConnectFlags::WILL_QOS_RESET;
        match qos {
            Qos::AtMostOnce => {}
            Qos::AtLeastOnce => self.flags |= ConnectFlags::WILL_QOS_1,
            Qos::ExactlyOnce => self.flags |= ConnectFlags::WILL_QOS_2,
        };

        if retain {
            self.flags |= ConnectFlags::WILL_RETAIN;
        }

        self
    }

    /// Username for to authenticate the connection.
    pub fn username(&mut self, username: &'b str) -> &mut Self {
        self.flags |= ConnectFlags::USERNAME;

        self.username = Some(username);

        self
    }

    /// Username and password to authenticate the connection.
    pub fn username_password(&mut self, username: &'b str, password: &'b [u8]) -> &mut Self {
        self.flags |= ConnectFlags::USERNAME | ConnectFlags::PASSWORD;

        self.username = Some(username);
        self.password = Some(password);

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_set_clean_session_on_empty() {
        let conn = ConnectBuilder::with_empty_client_id();

        assert!(conn.flags.contains(ConnectFlags::CLEAN_SESSION));
    }
}
