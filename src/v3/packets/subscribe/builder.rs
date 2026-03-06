//! Builder for subscribe

use std::num::NonZero;

use super::ack::SubAckCode;
use super::topic::RequestedQos;

/// Topic to subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscribeFilter<'a> {
    /// Topic to subscribe on
    pub topic: &'a str,
    // TODO: should we use a single QOS for the public interface of the crate
    /// Requested quality of service for the topic
    pub qos: RequestedQos,
}

/// Creates a subscribe packet.
#[derive(Debug)]
pub struct SubscribeBuilder<'a> {
    pub(crate) pkid: NonZero<u16>,
    pub(crate) filters: &'a [SubscribeFilter<'a>],
}

impl<'a> SubscribeBuilder<'a> {
    /// Creates a new subscribe filter
    pub fn new(pkid: NonZero<u16>, topic_filters: &'a [SubscribeFilter]) -> Self {
        Self {
            pkid,
            filters: topic_filters,
        }
    }

    /// Set the filter to a single topic
    pub fn with_topic(pkid: NonZero<u16>, topic: &'a SubscribeFilter) -> Self {
        Self::new(pkid, core::slice::from_ref(topic))
    }
}

#[derive(Debug)]
pub(crate) struct SubAckBuilder<'a> {
    pub(crate) pkid: NonZero<u16>,
    pub(crate) return_codes: &'a [SubAckCode],
}
