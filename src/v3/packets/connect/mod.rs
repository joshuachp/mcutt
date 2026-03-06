//! [`Connect`] and [`ConnAck`](self::ConnAck) packages.

use core::{fmt::Debug, num::TryFromIntError, ops::RangeInclusive, time::Duration};

use bitflags::bitflags;
use zerocopy::{network_endian::U16, *};

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};

use self::builder::{ConnectBuilder, Will};

use super::common::fixed::{ControlPacketType, FixedHeaderSlice};
use super::common::message::Buff;
use super::common::string::MqttStr;

pub mod ack;
pub mod builder;

/// First message sent by the Client to the Server.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718028>
pub struct Connect<'a> {
    fixed: &'a FixedHeaderSlice,
    header: &'a ConnectHeader,
    client_id: ClientId<'a>,
    will: Option<WillMsg<'a>>,
    username: Option<&'a MqttStr>,
    password: Option<&'a Buff>,
}

impl<'a> Connect<'a> {
    /// MQTT UTF-8 encoded protocol name "MQTT" capitalize.
    ///
    /// This will not change if future versions.
    pub const PROTOCOL_NAME: [u8; 6] = [0x0, 0x4, b'M', b'Q', b'T', b'T'];

    /// The value of the Protocol Level for the version v3.1.1 of the specification.
    pub const PROTOCOL_LEVEL_V3_1_1: u8 = 0x4;
}

impl Debug for Connect<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self {
            fixed,
            header,
            client_id,
            will,
            username,
            password,
        } = self;

        f.debug_struct("Connect")
            .field("fixed", fixed)
            .field("variable_header", header)
            .field("client_id", &client_id)
            .field("will", &will)
            .field("username", &username)
            .field("password", &password.as_ref().map(|_| "..."))
            .finish()
    }
}

impl<'a> Input<ConnectBuilder<'a>> for Connect<'a> {
    type Validated = ();

    fn validate_value(value: &ConnectBuilder) -> Result<(), Error> {
        let ConnectBuilder {
            client_id,
            flags: _,
            keepalive: _,
            will,
            username,
            password,
        } = value;

        ConnectHeader::validate_value(value)?;
        ClientId::validate_value(client_id)?;
        if let Some(will) = will {
            WillMsg::validate_value(will)?;
        }
        if let Some(username) = username {
            MqttStr::validate_value(username)?;
        }
        if let Some(password) = password {
            Buff::validate_value(password)?;
        }

        Ok(())
    }
}

impl<'a> Parsed for Connect<'a> {
    fn validate(&self) -> Result<(), Error> {
        let Self {
            fixed,
            header,
            client_id,
            will,
            username,
            password,
        } = self;

        if fixed.packet_type() != ControlPacketType::Connect {
            return Err(Error::new(ErrorKind::Invalid, "controll packet type"));
        }

        header.validate()?;
        client_id.validate()?;
        if let Some(will) = will {
            will.validate()?;
        }
        if let Some(username) = username {
            username.validate()?;
        }
        if let Some(password) = password {
            password.validate()?;
        }

        Ok(())
    }
}

impl<'c> Encode<ConnectBuilder<'c>> for Connect<'c> {
    fn encode_len(value: &ConnectBuilder<'c>) -> Result<usize, Error> {
        let ConnectBuilder {
            client_id,
            flags: _,
            keepalive: _,
            will,
            username,
            password,
        } = value;

        let mut len =
            ConnectHeader::encode_len(value)?.checked_add(ClientId::encode_len(client_id)?);

        if let Some(will) = will {
            let l = WillMsg::encode_len(will)?;
            len = len.and_then(|v| v.checked_add(l));
        }
        if let Some(username) = username {
            let l = MqttStr::encode_len(username)?;
            len = len.and_then(|v| v.checked_add(l));
        }
        if let Some(password) = password {
            let l = Buff::encode_len(password)?;
            len = len.and_then(|v| v.checked_add(l));
        }

        len.ok_or(Error::new(ErrorKind::Overflow, "CONNECT encode len"))
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &ConnectBuilder<'c>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        use super::common::fixed::builder::FixedHeaderBuilder;

        let rlen = Self::encode_len(value)?;

        let mut written = FixedHeaderBuilder::new(ControlPacketType::Connect)
            .remaining_length(rlen)?
            .write_sync(writer)?;

        let ConnectBuilder {
            client_id,
            flags: _,
            keepalive: _,
            will,
            username,
            password,
        } = value;

        written += ConnectHeader::write_sync(writer, value)?;

        written += ClientId::write_sync(writer, client_id)?;

        if let Some(will) = will {
            written += WillMsg::write_sync(writer, will)?;
        }

        if let Some(username) = username {
            written += MqttStr::write_sync(writer, username)?;
        }

        if let Some(password) = password {
            written += Buff::write_sync(writer, password)?;
        }

        Ok(written)
    }
}

impl<'a> Decode<'a> for Connect<'a> {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error> {
        let (fixed, rest) = FixedHeaderSlice::parse(buf)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::Connect);

        let (header, rest) = ConnectHeader::parse(rest)?;
        let (client_id, rest) = ClientId::parse(rest)?;

        let (will, rest) = if header.flags.contains(ConnectFlags::WILL_FLAG) {
            WillMsg::parse(rest).map(|(v, r)| (Some(v), r))?
        } else {
            (None, rest)
        };

        let (username, rest) = if header.flags.contains(ConnectFlags::USERNAME) {
            MqttStr::parse(rest).map(|(u, r)| (Some(u), r))?
        } else {
            (None, rest)
        };

        let (password, rest) = if header.flags.contains(ConnectFlags::PASSWORD) {
            Buff::parse(rest).map(|(u, r)| (Some(u), r))?
        } else {
            (None, rest)
        };

        let this = Self {
            fixed,
            header,
            client_id,
            will,
            username,
            password,
        };

        this.validate()?;

        Ok((this, rest))
    }
}

/// Variable header for the connect packet.
///
/// <http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718030>
#[derive(Debug, KnownLayout, Immutable, FromBytes, IntoBytes, ByteEq)]
#[repr(C)]
struct ConnectHeader {
    protocol_name: [u8; 6],
    protocol_level: u8,
    flags: ConnectFlags,
    keep_alive: KeepAlive,
}

impl<'a> Input<ConnectBuilder<'a>> for ConnectHeader {
    type Validated = Self;

    fn validate_value(value: &ConnectBuilder<'a>) -> Result<Self, Error> {
        if value.flags.contains(ConnectFlags::RESERVED) {
            return Err(Error::new(ErrorKind::Invalid, "CONNECT variable header"));
        }

        Ok(Self {
            protocol_name: Connect::PROTOCOL_NAME,
            protocol_level: Connect::PROTOCOL_LEVEL_V3_1_1,
            flags: value.flags,
            keep_alive: value.keepalive,
        })
    }
}

impl Parsed for ConnectHeader {
    fn validate(&self) -> Result<(), Error> {
        let is_valid = self.protocol_name == Connect::PROTOCOL_NAME
            && self.protocol_level == Connect::PROTOCOL_LEVEL_V3_1_1
            && !self.flags.contains(ConnectFlags::RESERVED);

        if !is_valid {
            return Err(Error::new(ErrorKind::Invalid, "CONNECT variable header"));
        }

        Ok(())
    }
}

impl<'a> Encode<ConnectBuilder<'a>> for ConnectHeader {
    fn encode_len(_value: &ConnectBuilder<'a>) -> Result<usize, Error> {
        Ok(size_of::<Self>())
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &ConnectBuilder<'a>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        let this = Self::validate_value(value)?;

        writer.write_all(this.as_bytes()).map_err(|error| {
            Error::new(ErrorKind::StdIo(error.kind()), "writing CONNECT header")
        })?;

        Ok(size_of::<Self>())
    }
}

impl<'a> Decode<'a> for ConnectHeader {
    type Out = &'a Self;

    fn parse(buf: &[u8]) -> Result<(&Self, &[u8]), Error> {
        let (this, rest) = ConnectHeader::ref_from_prefix(buf)
            .map_err(|_| Error::new(ErrorKind::NotEnoughSpace, "connect header"))?;

        this.validate()?;

        Ok((this, rest))
    }
}

/// Connect packet flags for the variable header.
#[derive(Debug, Clone, Copy, KnownLayout, Immutable, FromBytes, IntoBytes, ByteEq)]
#[repr(transparent)]
pub struct ConnectFlags(u8);

bitflags! {
    impl ConnectFlags: u8 {
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

        /// Reserved bit
        const RESERVED = 0b0000_0001;
    }
}

/// Interval in second for the client keep alive.
///
/// It is the maximum time interval that is permitted to elapse between the point at which the
/// Client finishes transmitting one Control Packet and the point it starts sending the next.
#[derive(Debug, Clone, Copy, KnownLayout, Immutable, FromBytes, IntoBytes, ByteEq)]
#[repr(transparent)]
pub struct KeepAlive {
    seconds: U16,
}

impl KeepAlive {
    /// Check if the keep alive is not `0`.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.seconds != 0
    }

    /// Read the keep alive value as a [`Duration`]
    pub fn read(&self) -> Duration {
        Duration::from_secs(self.seconds.get().into())
    }
}

impl From<u16> for KeepAlive {
    fn from(value: u16) -> Self {
        let bytes = value.to_be_bytes();

        transmute!(bytes)
    }
}

impl TryFrom<Duration> for KeepAlive {
    type Error = TryFromIntError;

    fn try_from(value: Duration) -> Result<Self, Self::Error> {
        u16::try_from(value.as_secs()).map(Self::from)
    }
}

impl Default for KeepAlive {
    fn default() -> Self {
        Self {
            seconds: U16::from(60),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WillMsg<'a> {
    topic: &'a MqttStr,
    message: &'a Buff,
}

impl<'a> Input<Will<'a>> for WillMsg<'a> {
    type Validated = ();

    fn validate_value(value: &Will) -> Result<(), Error> {
        MqttStr::validate_value(value.topic)?;
        Buff::validate_value(value.message)?;

        Ok(())
    }
}

impl<'a> Parsed for WillMsg<'a> {
    fn validate(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl<'a> Encode<Will<'a>> for WillMsg<'a> {
    fn encode_len(value: &Will<'a>) -> Result<usize, Error> {
        let topic = MqttStr::encode_len(value.topic)?;
        let message = Buff::encode_len(value.message)?;

        Ok(topic + message)
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(buf: &mut W, value: &Will<'a>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        let topic = MqttStr::write_sync(buf, value.topic)?;
        let msg = Buff::write_sync(buf, value.message)?;

        Ok(topic + msg)
    }
}

impl<'a> Decode<'a> for WillMsg<'a> {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error> {
        let (topic, rest) = MqttStr::parse(buf)?;
        let (message, rest) = Buff::parse(rest)?;

        let this = Self { topic, message };

        this.validate()?;

        Ok((this, rest))
    }
}

#[derive(Debug)]
struct ClientId<'a>(&'a MqttStr);

impl<'a> ClientId<'a> {
    /// Minimum number of bytes in the client ids
    pub const CLIENT_ID_BYTES: RangeInclusive<usize> = 1..=23;
}

impl<'a> Input<str> for ClientId<'a> {
    type Validated = ();

    fn validate_value(_value: &str) -> Result<(), Error> {
        // TODO: should we validate this?
        // if !Self::CLIENT_ID_BYTES.contains(&value.len()) {
        //     return Err(Error::new(
        //         ErrorKind::OutOfRange,
        //         "client id must be between 1 and 23 bytes long",
        //     ));
        // }

        // let is_ascii = value.chars().all(|b| b.is_ascii_alphanumeric());

        // if !is_ascii {
        //     return Err(Error::new(
        //         ErrorKind::Invalid,
        //         "client id is not ascii alphanumeric",
        //     ));
        // }

        Ok(())
    }
}

impl<'a> Parsed for ClientId<'a> {
    fn validate(&self) -> Result<(), Error> {
        if !Self::CLIENT_ID_BYTES.contains(&self.0.len()) {
            return Err(Error::new(
                ErrorKind::OutOfRange,
                "client id must be between 1 and 23 bytes long",
            ));
        }

        let is_ascii = self.0.as_str().chars().all(|b| b.is_ascii_alphanumeric());

        if !is_ascii {
            return Err(Error::new(
                ErrorKind::Invalid,
                "client id is not ascii alphanumeric",
            ));
        }

        Ok(())
    }
}

impl<'c> Encode<str> for ClientId<'c> {
    fn encode_len(value: &str) -> Result<usize, Error> {
        MqttStr::encode_len(value)
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(buf: &mut W, value: &str) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        Self::validate_value(value)?;

        MqttStr::write_sync(buf, value)
    }
}

impl<'a> Decode<'a> for ClientId<'a> {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (str, rest) = MqttStr::parse(buf)?;

        let this = ClientId(str);

        this.validate()?;

        Ok((ClientId(str), rest))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::v3::packets::Qos;

    use super::*;

    #[test]
    fn should_permit_keep_alive_zero() {
        let keep_alive = KeepAlive::from(0);

        assert_eq!(
            keep_alive,
            KeepAlive {
                seconds: U16::from(0)
            }
        );

        let keep_alive = KeepAlive::try_from(Duration::from_secs(0)).unwrap();

        assert_eq!(
            keep_alive,
            KeepAlive {
                seconds: U16::from(0)
            }
        );
    }

    #[test]
    fn should_encode_connect() {
        let mut buf = Vec::new();

        let keepalive = KeepAlive::try_from(Duration::from_secs(10)).unwrap();

        let written = Connect::write_sync(
            &mut buf,
            ConnectBuilder::create("client_id")
                .unwrap()
                .keepalive(keepalive)
                .will("/will", b"hello", Qos::AtLeastOnce, true)
                .clean_session()
                .username_password("username", b"passwd"),
        )
        .unwrap();

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

        assert_eq!(written, expect.len());
        assert_eq!(buf, expect);
    }
}
