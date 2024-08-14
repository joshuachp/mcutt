//! Connect and ConnAck packages.

use core::{mem, num::TryFromIntError, ops::Deref, time::Duration};

use bitflags::bitflags;

use super::{
    header::{
        BytesBuf, ControlPacketType, FixedHeader, RemainingLength, RemainingLengthError, Str,
        TypeFlags,
    },
    Encode, EncodeError,
};

/// First message sent by the Client to the Server.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718028>
#[derive(Debug, Clone, Copy)]
pub struct Connect<'a> {
    flags: ConnectFlags,
    keep_alive: KeepAlive,
    client_id: Str<'a>,
    will: Option<Will<'a>>,
    username: Option<Str<'a>>,
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
    pub fn new(client_id: Str<'a>, keep_alive: KeepAlive) -> Self {
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

    pub fn clean_session(&mut self) -> &mut Self {
        self.flags |= ConnectFlags::CLEAN_SESSION;

        self
    }

    pub fn will(
        &mut self,
        topic: Str<'a>,
        message: BytesBuf<'a>,
        qos: WillQos,
        retain: bool,
    ) -> &mut Self {
        self.will = Some(Will { topic, message });

        self.flags |= ConnectFlags::WILL_FLAG;

        self.flags &= ConnectFlags::WILL_QOS_RESET;
        match qos {
            WillQos::Qos0 => {}
            WillQos::Qos1 => self.flags |= ConnectFlags::WILL_QOS_1,
            WillQos::Qos2 => self.flags |= ConnectFlags::WILL_QOS_2,
        };

        if retain {
            self.flags |= ConnectFlags::WILL_RETAIN;
        }

        self
    }

    pub fn username(&mut self, username: Str<'a>) -> &mut Self {
        self.flags |= ConnectFlags::USERNAME;

        self.username = Some(username);

        self
    }

    pub fn username_password(&mut self, username: Str<'a>, password: BytesBuf<'a>) -> &mut Self {
        self.flags |= ConnectFlags::USERNAME | ConnectFlags::PASSWORD;

        self.username = Some(username);
        self.password = Some(password);

        self
    }

    /// Add all the sizes together and return it in
    /// we use a saturating add since the [`usize::MAX`] will return
    /// an [``]
    fn remaining_length(&self) -> Result<RemainingLength, RemainingLengthError> {
        let variable_headers = Self::PROTOCOL_NAME
            .len()
            .saturating_add(mem::size_of_val(&Self::PROTOCOL_LEVEL_V3_1_1))
            .saturating_add(mem::size_of::<ConnectFlags>())
            .saturating_add(mem::size_of::<KeepAlive>());

        let mut payload = mem::size_of::<u16>().saturating_add(self.client_id.len_as_bytes());
        if let Some(will) = self.will {
            payload = payload
                .saturating_add(mem::size_of::<u16>())
                .saturating_add(will.topic.len_as_bytes())
                .saturating_add(mem::size_of::<u16>())
                .saturating_add(will.message.len());
        }

        if let Some(username) = self.username {
            payload = payload
                .saturating_add(mem::size_of::<u16>())
                .saturating_add(username.len_as_bytes());
        }

        if let Some(password) = self.password {
            payload = payload
                .saturating_add(mem::size_of::<u16>())
                .saturating_add(password.len());
        }

        RemainingLength::try_from(variable_headers.saturating_add(payload))
    }

    fn fixed_header(&self) -> Result<FixedHeader, RemainingLengthError> {
        self.remaining_length().map(|remaining_length| {
            FixedHeader::new(
                ControlPacketType::Connect,
                TypeFlags::empty(),
                remaining_length,
            )
        })
    }
}

impl<'a> Encode for Connect<'a> {
    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let fixed_header = self.fixed_header()?;

        let mut count = fixed_header
            .write(writer)?
            .saturating_add(Self::PROTOCOL_NAME.len())
            .saturating_add(mem::size_of_val(&Self::PROTOCOL_LEVEL_V3_1_1))
            .saturating_add(mem::size_of::<ConnectFlags>())
            .saturating_add(mem::size_of::<KeepAlive>());

        writer
            .write_all(&Self::PROTOCOL_NAME)
            .and_then(|()| writer.write_u8(Self::PROTOCOL_LEVEL_V3_1_1))
            .and_then(|()| writer.write_u8(self.flags.bits()))
            .and_then(|()| writer.write_u16(*self.keep_alive))
            .map_err(EncodeError::Write)?;

        count = count.saturating_add(self.client_id.write(writer)?);

        if let Some(will) = self.will {
            let written = will.write(writer)?;

            count = count.saturating_add(written);
        }

        if let Some(username) = self.username {
            let written = username.write(writer)?;

            count = count.saturating_add(written);
        }

        if let Some(password) = self.password {
            let written = password.write(writer)?;

            count = count.saturating_add(written);
        }

        Ok(count)
    }
}

bitflags! {
    /// Connect packet flags for the variable header.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConnectFlags: u8 {
        const USERNAME = 0b10000000;
        const PASSWORD = 0b01000000;

        const WILL_RETAIN = 0b00100000;

        const WILL_QOS_1 = 0b00001000;
        const WILL_QOS_2 = 0b00010000;

        /// Inverse mask to resets the Will QOS flags.
        const WILL_QOS_RESET = 0b11100111;

        const WILL_FLAG = 0b00000100;

        const CLEAN_SESSION = 0b00000010;
    }
}

/// Interval in second for the client keep alive.
///
/// It is the maximum time interval that is permitted to elapse between the point at which the
/// Client finishes transmitting one Control Packet and the point it starts sending the next.
#[derive(Debug, Clone, Copy)]
pub struct KeepAlive(u16);

impl KeepAlive {
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

#[derive(Debug, Clone, Copy)]
pub struct Will<'a> {
    topic: Str<'a>,
    message: BytesBuf<'a>,
}

impl<'a> Encode for Will<'a> {
    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let topic = self.topic.write(writer)?;
        let msg = self.message.write(writer)?;

        Ok(topic.saturating_add(msg))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WillQos {
    Qos0,
    Qos1,
    Qos2,
}

#[cfg(test)]
mod tests {

    use crate::v3::tests::TestWriter;

    use super::*;

    #[test]
    fn should_set_clean_session_on_empty() {
        let client_id = Str::new();
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

        connect
            .clean_session()
            .will(
                Str::try_from("/will").unwrap(),
                BytesBuf::try_from(b"hello".as_slice()).unwrap(),
                WillQos::Qos1,
                true,
            )
            .username_password(
                Str::try_from("username").unwrap(),
                BytesBuf::try_from(b"passwd".as_slice()).unwrap(),
            );

        let mut writer = TestWriter::new();

        let written = connect.write(&mut writer).unwrap();

        let expect = [
            0b00010000, // head flags
            53,         // remaining length
            0, 4, b'M', b'Q', b'T', b'T',       // protocol name
            4,          // Protocol level
            0b11101110, // con flags
            0, 10, // keep alive
            0, 9, b'c', b'l', b'i', b'e', b'n', b't', b'_', b'i', b'd', // client id
            0, 5, b'/', b'w', b'i', b'l', b'l', // will topic
            0, 5, b'h', b'e', b'l', b'l', b'o', // will msg
            0, 8, b'u', b's', b'e', b'r', b'n', b'a', b'm', b'e', // username
            0, 6, b'p', b'a', b's', b's', b'w', b'd', // password
        ];

        pretty_assertions::assert_eq!(written, expect.len());
        pretty_assertions::assert_eq!(expect, writer.buf[..55]);
    }
}