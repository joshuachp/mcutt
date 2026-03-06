//! Builder for subscribe

use std::num::NonZero;

use super::ack::SubAckCode;
use super::topic::RequestedQos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SubscribeFilter<'a> {
    pub(crate) topic: &'a str,
    // TODO: should we use a single QOS for the public interface of the crate
    pub(crate) qos: RequestedQos,
}

/// Creates a subscribe packet.
#[derive(Debug)]
pub(crate) struct SubscribeBuilder<'a> {
    pub(crate) pkid: NonZero<u16>,
    pub(crate) filters: &'a [SubscribeFilter<'a>],
}

impl<'a> SubscribeBuilder<'a> {
    pub(crate) fn new(pkid: NonZero<u16>, topic_filters: &'a [SubscribeFilter]) -> Self {
        Self {
            pkid,
            filters: topic_filters,
        }
    }

    /// Set the filter to a single topic
    pub(crate) fn with_topic(pkid: NonZero<u16>, topic: &'a SubscribeFilter) -> Self {
        Self::new(pkid, core::slice::from_ref(topic))
    }
}

#[derive(Debug)]
pub(crate) struct SubAckBuilder<'a> {
    pub(crate) pkid: NonZero<u16>,
    pub(crate) return_codes: &'a [SubAckCode],
}

impl<'a> SubAckBuilder<'a> {
    pub(crate) fn new(pkid: NonZero<u16>, return_codes: &'a [SubAckCode]) -> Self {
        Self { pkid, return_codes }
    }

    pub(crate) fn with_code(pkid: NonZero<u16>, return_code: &'a SubAckCode) -> Self {
        Self::new(pkid, core::slice::from_ref(return_code))
    }
}
