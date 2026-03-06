//! [`ConnAck`] packet in response to the [`Connect`](super::Connect).

use core::fmt::Display;

use bitflags::bitflags;
use zerocopy::*;

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};
use crate::v3::packets::common::fixed::{ControlPacketType, FixedHeaderArray};

/// Is the server response to the [`Connect`](super::Connect) packet.
///
/// It's the first packet sent from the client.
#[derive(Debug, Clone, Copy, KnownLayout, Immutable, TryFromBytes, IntoBytes, ByteEq)]
#[repr(C)]
pub struct ConnAck {
    /// Flags for the connack packet
    flags: ConnAckFlags,
    /// Return code value.
    return_code: ReturnCode,
}

impl ConnAck {
    /// Creates a CONNACK packet
    pub fn new(flags: ConnAckFlags, return_code: ReturnCode) -> Self {
        Self { flags, return_code }
    }

    /// Flag to indicate if the session is present on the Server.
    #[must_use]
    pub fn session_present(&self) -> bool {
        self.flags.contains(ConnAckFlags::SESSION_PRESENT)
    }

    /// The return code of the CONNACK
    #[must_use]
    pub fn return_code(&self) -> ReturnCode {
        self.return_code
    }

    /// The return code of the CONNACK
    pub fn error_for_code(self) -> Result<Self, Error> {
        match self.return_code {
            ReturnCode::Accepted => Ok(self),
            ReturnCode::ProtocolVersion => Err(Error::new(ErrorKind::Invalid, "protocl version")),
            ReturnCode::IdentifierRejected => Err(Error::new(ErrorKind::Invalid, "client id")),
            ReturnCode::ServerUnavailable => {
                Err(Error::new(ErrorKind::Invalid, "server not available"))
            }
            ReturnCode::BadUsernamePassword => {
                Err(Error::new(ErrorKind::Invalid, "username or password"))
            }
            ReturnCode::NotAuthorized => Err(Error::new(ErrorKind::Invalid, "not authorized")),
        }
    }
}

impl Display for ConnAck {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} session({}) {}",
            ControlPacketType::ConnAck,
            self.session_present(),
            self.return_code
        )
    }
}

impl Input<Self> for ConnAck {
    type Validated = ();

    fn validate_value(value: &Self) -> Result<(), Error> {
        value.validate()?;

        Ok(())
    }
}

impl Parsed for ConnAck {
    fn validate(&self) -> Result<(), Error> {
        if !(self.flags & !ConnAckFlags::SESSION_PRESENT).is_empty() {
            return Err(Error::new(ErrorKind::Invalid, "connack flags reserved"));
        }

        Ok(())
    }
}

impl Encode<Self> for ConnAck {
    fn encode_len(_value: &Self) -> Result<usize, Error> {
        Ok(core::mem::size_of::<Self>())
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &Self) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        Self::validate_value(value)?;

        writer.write_all(value.as_bytes()).map_err(|error| {
            Error::new(ErrorKind::StdIo(error.kind()), "writing CONNACK packet")
        })?;

        Ok(size_of::<Self>())
    }
}

impl<'a> Decode<'a> for ConnAck {
    type Out = &'a Self;

    fn parse(buff: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error> {
        let (fixed, rest) = FixedHeaderArray::<1>::parse(buff)?;

        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::ConnAck);

        let (this, rest) = ConnAck::try_ref_from_prefix(rest).map_err(|error| {
            let kind = match error {
                ConvertError::Alignment(_) | ConvertError::Size(_) => ErrorKind::NotEnoughSpace,
                ConvertError::Validity(_) => ErrorKind::Invalid,
            };

            Error::new(kind, "CONNACK packet")
        })?;

        this.validate()?;

        Ok((this, rest))
    }
}

/// Connection return code in the [`ConnAck`] packet.
#[derive(Debug, Clone, Copy, KnownLayout, Immutable, TryFromBytes, IntoBytes, ByteEq)]
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
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let code = match value {
            0 => ReturnCode::Accepted,
            1 => ReturnCode::ProtocolVersion,
            2 => ReturnCode::IdentifierRejected,
            3 => ReturnCode::ServerUnavailable,
            4 => ReturnCode::BadUsernamePassword,
            5 => ReturnCode::NotAuthorized,
            6.. => return Err(Error::new(ErrorKind::Invalid, "CONNACK return code")),
        };

        Ok(code)
    }
}

/// Byte 1 is the "Connect Acknowledge Flags".
///
/// Bits 7-1 are reserved and MUST be set to 0.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718033>
#[derive(Debug, Clone, Copy, KnownLayout, Immutable, FromBytes, IntoBytes, ByteEq)]
#[repr(transparent)]
pub struct ConnAckFlags(u8);

bitflags! {
    impl ConnAckFlags: u8 {
        /// Session is already present on the server.
        ///
        /// If the Server accepts a connection with CleanSession set to 1, the Server MUST set
        /// Session Present to 0 in the CONNACK packet in addition to setting a zero return code in
        /// the CONNACK packet
        const SESSION_PRESENT = 0b0000_0001;
    }
}
