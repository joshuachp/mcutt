use crate::bytes::{Encode, Error, ErrorKind};

use super::{ControlPacketType, TypeFlags};

#[derive(Debug)]
pub(crate) struct FixedHeaderBuilder {
    pub(crate) packet_type: ControlPacketType,
    pub(crate) flags: TypeFlags,
    pub(crate) remaining_length: u32,
}

impl FixedHeaderBuilder {
    pub(crate) fn new(packet_type: ControlPacketType) -> Self {
        let flags = Self::default_flags(packet_type);

        Self::with_flags(packet_type, flags)
    }

    pub(crate) fn with_flags(packet_type: ControlPacketType, flags: TypeFlags) -> Self {
        Self {
            packet_type,
            flags,
            remaining_length: 0,
        }
    }

    pub(crate) fn remaining_length(mut self, remaining_length: usize) -> Result<Self, Error> {
        let remaining_length = u32::try_from(remaining_length)
            .map_err(|_| Error::new(ErrorKind::OutOfRange, "remaining length"))?;

        self.remaining_length = remaining_length;

        Ok(self)
    }

    #[cfg(feature = "std")]
    pub(crate) fn write_sync<W>(self, writer: &mut W) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        super::FixedHeaderSlice::write_sync(writer, &self)
    }

    fn default_flags(packet_type: ControlPacketType) -> TypeFlags {
        match packet_type {
            ControlPacketType::Connect
            | ControlPacketType::ConnAck
            | ControlPacketType::Publish
            | ControlPacketType::PubAck
            | ControlPacketType::PubRec
            | ControlPacketType::PubComp
            | ControlPacketType::SubAck
            | ControlPacketType::UnsubAck
            | ControlPacketType::PingReq
            | ControlPacketType::PingResp
            | ControlPacketType::Disconnect => TypeFlags::empty(),
            ControlPacketType::PubRel => TypeFlags::PUBREL,
            ControlPacketType::Subscribe => TypeFlags::SUBSCRIBE,
            ControlPacketType::Unsubscribe => TypeFlags::UNSUBSCRIBE,
        }
    }
}
