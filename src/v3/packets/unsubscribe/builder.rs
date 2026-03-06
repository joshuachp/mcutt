//! Builder for UNSUBSCRIBE packets.

use std::borrow::Borrow;
use std::num::NonZero;

/// Creates a subscribe packet.
#[derive(Debug)]
pub(crate) struct UnsubscribeBuilder<T> {
    pub(crate) pkid: NonZero<u16>,
    pub(crate) filters: T,
}

impl<T> UnsubscribeBuilder<T> {
    pub(crate) fn new<S>(pkid: NonZero<u16>, topic_filters: T) -> Self
    where
        for<'a> &'a T: IntoIterator<Item = &'a S>,
        S: Borrow<str>,
    {
        Self {
            pkid,
            filters: topic_filters,
        }
    }
}

impl<'a> UnsubscribeBuilder<[&'a str; 1]> {
    pub(crate) fn with_topic(pkid: NonZero<u16>, topic: &'a str) -> Self {
        Self::new(pkid, [topic])
    }
}
