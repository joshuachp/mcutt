//! [`Connect`] and [`ConnAck`] packages.

use core::{
    fmt::{Debug, Display},
    mem,
    num::TryFromIntError,
    ops::Deref,
    time::Duration,
};

use bitflags::bitflags;

use crate::bytes::read_u8;

use super::{
    header::{BytesBuf, ControlPacketType, FixedHeader, RemainingLength, StrRef, TypeFlags},
    DecodeError, DecodePacket, Encode, EncodeError, EncodePacket, Qos,
};

/// First message sent by the Client to the Server.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718028>
#[derive(Clone, Copy)]
pub struct Connect<'a> {
    flags: ConnectFlags,
    keep_alive: KeepAlive,
    client_id: StrRef<'a>,
    will: Option<Will<'a>>,
    username: Option<StrRef<'a>>,
    password: Option<BytesBuf<'a>>,
}

impl<'a> Connect<'a> {
    /// MQTT UTF-8 encoded protocol name "MQTT" capitalize.
    ///
    /// This will not change if future versions.
    pub const PROTOCOL_NAME: [u8; 6] = [0x0, 0x4, b'M', b'Q', b'T', b'T'];

    /// The value of the Protocol Level for the version v3.1.1 of the specification.
    pub const PROTOCOL_LEVEL_V3_1_1: u8 = 0x4;

    /// Create a new packet with the given client identifier (`client_id`).
    //
    /// If the provided client identifier is empty, the [`clean session flag`](ConnectFlags::CLEAN_SESSION) will be set to true.
    #[must_use]
    pub fn new(client_id: StrRef<'a>, keep_alive: KeepAlive) -> Self {
        let mut flags = ConnectFlags::empty();

        if client_id.is_empty() {
            flags |= ConnectFlags::CLEAN_SESSION;
        }

        Self {
            flags,
            keep_alive,
            client_id,
            will: None,
            username: None,
            password: None,
        }
    }

    /// Sets the clean session flag for the connection.
    ///
    /// It's used to control the lifetime of the session state in the broker. Calling this function
    /// will set the clean session flag and session must be discarded for the broker.
    pub fn clean_session(&mut self) -> &mut Self {
        self.flags |= ConnectFlags::CLEAN_SESSION;

        self
    }

    /// Sets the will for the connection.
    ///
    /// If the client is disconnected for a network failure, the keep alive expires, protocol error, or the
    /// connection is closed without a DISCONNECT packet. The Server will publish the will message
    /// on the specified topic.
    pub fn will(&mut self, message: Will<'a>, qos: Qos, retain: bool) -> &mut Self {
        self.will = Some(message);

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
    pub fn username(&mut self, username: StrRef<'a>) -> &mut Self {
        self.flags |= ConnectFlags::USERNAME;

        self.username = Some(username);

        self
    }

    /// Username and password to authenticate the connection.
    pub fn username_password(&mut self, username: StrRef<'a>, password: BytesBuf<'a>) -> &mut Self {
        self.flags |= ConnectFlags::USERNAME | ConnectFlags::PASSWORD;

        self.username = Some(username);
        self.password = Some(password);

        self
    }
}

impl<'a> Debug for Connect<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Connect")
            .field("flags", &self.flags)
            .field("keep_alive", &self.keep_alive)
            .field("client_id", &self.client_id)
            .field("will", &self.will)
            .field("username", &self.username)
            .field("password", &self.password.map(|_| "..."))
            .finish()
    }
}

impl<'a> EncodePacket for Connect<'a> {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::Connect
    }

    fn packet_flags(&self) -> TypeFlags {
        TypeFlags::empty()
    }

    fn remaining_len(&self) -> usize {
        let variable_headers = Self::PROTOCOL_NAME
            .len()
            .saturating_add(mem::size_of_val(&Self::PROTOCOL_LEVEL_V3_1_1))
            .saturating_add(mem::size_of::<ConnectFlags>())
            .saturating_add(mem::size_of::<KeepAlive>());

        let mut payload = self.client_id.encode_len();
        if let Some(will) = self.will {
            payload = payload.saturating_add(will.encode_len());
        }

        if let Some(username) = self.username {
            payload = payload.saturating_add(username.encode_len());
        }

        if let Some(password) = self.password {
            payload = payload.saturating_add(password.encode_len());
        }

        variable_headers.saturating_add(payload)
    }

    fn write_packet<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let variable = writer
            .write_slice(&Self::PROTOCOL_NAME)
            .and_then(|n| Ok(n.saturating_add(writer.write_u8(Self::PROTOCOL_LEVEL_V3_1_1)?)))
            .and_then(|n| Ok(n.saturating_add(writer.write_u8(self.flags.bits())?)))
            .and_then(|n| Ok(n.saturating_add(writer.write_u16(*self.keep_alive)?)))
            .map_err(EncodeError::Write)?;

        let mut payload = self.client_id.write(writer)?;
        if let Some(will) = self.will {
            payload = payload.saturating_add(will.write(writer)?);
        }

        if let Some(username) = self.username {
            payload = payload.saturating_add(username.write(writer)?);
        }

        if let Some(password) = self.password {
            payload = payload.saturating_add(password.write(writer)?);
        }

        Ok(variable.saturating_add(payload))
    }
}

bitflags! {
    /// Connect packet flags for the variable header.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConnectFlags: u8 {
        /// The username is present in the payload.
        const USERNAME = 0b1000_0000;
        /// The password is present in the payload.
        const PASSWORD = 0b0100_0000;

        /// The Will is retained in the Server.
        const WILL_RETAIN = 0b0010_0000;

        /// The QoS first bit.
        const WILL_QOS_1 = 0b0000_1000;
        /// The QoS second bit.
        const WILL_QOS_2 = 0b0001_0000;

        /// Inverse mask to resets the Will QOS flags.
        const WILL_QOS_RESET = 0b1110_0111;

        /// The Will topic and message is present in the payload
        const WILL_FLAG = 0b0000_0100;

        /// The Clean Session flag is set for the connection.
        const CLEAN_SESSION = 0b0000_0010;
    }
}

/// Interval in second for the client keep alive.
///
/// It is the maximum time interval that is permitted to elapse between the point at which the
/// Client finishes transmitting one Control Packet and the point it starts sending the next.
#[derive(Debug, Clone, Copy)]
pub struct KeepAlive(u16);

impl KeepAlive {
    /// Check if the keep alive is not `0`.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.0 != 0
    }
}

impl From<u16> for KeepAlive {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl TryFrom<Duration> for KeepAlive {
    type Error = TryFromIntError;

    fn try_from(value: Duration) -> Result<Self, Self::Error> {
        let keep_alive = u16::try_from(value.as_secs())?;

        Ok(Self(keep_alive))
    }
}

impl Deref for KeepAlive {
    type Target = u16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Will message struct
#[derive(Debug, Clone, Copy)]
pub struct Will<'a> {
    topic: StrRef<'a>,
    message: BytesBuf<'a>,
}

impl<'a> Will<'a> {
    /// Create a new will message with the specified topic and payload.
    #[must_use]
    pub fn new(topic: StrRef<'a>, message: BytesBuf<'a>) -> Self {
        Self { topic, message }
    }
}

impl<'a> Encode for Will<'a> {
    fn encode_len(&self) -> usize {
        self.topic
            .encode_len()
            .saturating_add(self.message.encode_len())
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let topic = self.topic.write(writer)?;
        let msg = self.message.write(writer)?;

        Ok(topic.saturating_add(msg))
    }
}

/// Is the server response to the [`Connect`] packet.
///
/// It's the first packet sent from the client.
#[derive(Debug, Clone, Copy)]
pub struct ConnAck {
    /// Session present on the server.
    session_present: bool,
    /// Return code value.
    return_code: ReturnCode,
}

impl ConnAck {
    /// The remaining length of the CONNACK.
    pub const REMAINING_LENGTH: RemainingLength = RemainingLength::new_const(2);

    /// Flag to indicate if the session is present on the Server.
    #[must_use]
    pub fn session_present(&self) -> bool {
        self.session_present
    }

    /// The return code of the CONNACK
    #[must_use]
    pub fn return_code(&self) -> ReturnCode {
        self.return_code
    }
}

impl Display for ConnAck {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} session({}) {}",
            ControlPacketType::ConnAck,
            self.session_present,
            self.return_code
        )
    }
}

impl<'a> DecodePacket<'a> for ConnAck {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::ConnAck
    }

    fn fixed_remaining_length() -> Option<RemainingLength> {
        Some(Self::REMAINING_LENGTH)
    }

    fn parse_with_header(_header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let (flags, bytes) = read_u8(bytes)?;
        let flags = ConnAckFlags::from_bits_retain(flags);

        if !(flags & ConnAckFlags::MASK).is_empty() {
            return Err(DecodeError::Reserved);
        }

        let session_present = flags.contains(ConnAckFlags::SESSION_PRESENT);

        let (return_code, bytes) = read_u8(bytes)?;
        debug_assert!(bytes.is_empty());

        let return_code = ReturnCode::try_from(return_code)?;

        Ok(Self {
            session_present,
            return_code,
        })
    }
}

/// Connection return code in the [`ConnAck`] packet.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ReturnCode {
    /// Connection accepted.
    Accepted = 0,
    /// The Server does not support the level of the MQTT protocol requested by the client.
    ProtocolVersion = 1,
    /// The Client identifier is correct UTF-8 but not allowed by the Server.
    IdentifierRejected = 2,
    /// The Network Connection has been made but the MQTT service is unavailable.
    ServerUnavailable = 3,
    /// The data in the user name or password is malformed.
    BadUsernamePassword = 4,
    /// The Client is not authorized to connect.
    NotAuthorized = 5,
}

impl ReturnCode {
    /// Returns `true` if the connect return code is [`Accepted`].
    ///
    /// [`Accepted`]: ReturnCode::Accepted
    #[must_use]
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }
}

impl Display for ReturnCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let code = *self as u8;
        match self {
            ReturnCode::Accepted => write!(f, "connection accepted ({code})"),
            ReturnCode::ProtocolVersion => {
                write!(f, "unacceptable protocol version ({code})")
            }
            ReturnCode::IdentifierRejected => write!(f, "identifier rejected ({code})"),
            ReturnCode::ServerUnavailable => write!(f, "server unavailable ({code})"),
            ReturnCode::BadUsernamePassword => {
                write!(f, "bad user name or password ({code})")
            }
            ReturnCode::NotAuthorized => write!(f, "not authorized ({code})"),
        }
    }
}

impl TryFrom<u8> for ReturnCode {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let code = match value {
            0 => ReturnCode::Accepted,
            1 => ReturnCode::ProtocolVersion,
            2 => ReturnCode::IdentifierRejected,
            3 => ReturnCode::ServerUnavailable,
            4 => ReturnCode::BadUsernamePassword,
            5 => ReturnCode::NotAuthorized,
            6.. => return Err(DecodeError::Reserved),
        };

        Ok(code)
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    struct ConnAckFlags: u8 {
        const MASK = 0b1111_1110;
        const SESSION_PRESENT = 0b0000_0001;
    }
}

#[cfg(test)]
mod tests {

    use crate::v3::{header::Str, tests::TestWriter};

    use super::*;

    #[test]
    fn should_set_clean_session_on_empty() {
        let client_id = StrRef::new();
        let keep_alive = Duration::from_secs(30).try_into().unwrap();

        let conn = Connect::new(client_id, keep_alive);

        assert!(conn.flags.contains(ConnectFlags::CLEAN_SESSION));
    }

    #[test]
    fn should_encode_connect() {
        let mut connect = Connect::new(
            Str::try_from("client_id").unwrap(),
            KeepAlive::try_from(Duration::from_secs(10)).unwrap(),
        );

        let will = Will::new(
            Str::try_from("/will").unwrap(),
            BytesBuf::try_from(b"hello".as_slice()).unwrap(),
        );

        connect
            .clean_session()
            .will(will, Qos::AtLeastOnce, true)
            .username_password(
                Str::try_from("username").unwrap(),
                BytesBuf::try_from(b"passwd".as_slice()).unwrap(),
            );

        let mut writer = TestWriter::new();

        let written = connect.write(&mut writer).unwrap();

        let expect = [
            // head flags
            0b0001_0000,
            // remaining length
            53,
            // protocol name
            0,
            4,
            b'M',
            b'Q',
            b'T',
            b'T',
            // Protocol level
            4,
            // con flags
            0b1110_1110,
            // keep alive
            0,
            10,
            // client id
            0,
            9,
            b'c',
            b'l',
            b'i',
            b'e',
            b'n',
            b't',
            b'_',
            b'i',
            b'd',
            // will topic
            0,
            5,
            b'/',
            b'w',
            b'i',
            b'l',
            b'l',
            // will msg
            0,
            5,
            b'h',
            b'e',
            b'l',
            b'l',
            b'o',
            // username
            0,
            8,
            b'u',
            b's',
            b'e',
            b'r',
            b'n',
            b'a',
            b'm',
            b'e',
            // password
            0,
            6,
            b'p',
            b'a',
            b's',
            b's',
            b'w',
            b'd',
        ];

        pretty_assertions::assert_eq!(written, expect.len());
        pretty_assertions::assert_eq!(expect, writer.buf[..55]);
    }
}
