//! Subscribe packet structs when allocations are enabled.

use core::ops::Deref;

use alloc::{string::String, vec::Vec};

use super::{SubscribeCursor, SubscribeTopic};

impl SubscribeCursor<'_> {
    /// Converts the filter to an owned value.
    pub fn to_vec(&self) -> Vec<SubscribeTopic<String>> {
        self.into_iter().map(|f| f.into()).collect()
    }
}

impl<S> PartialEq<Vec<SubscribeTopic<S>>> for SubscribeCursor<'_>
where
    S: Deref<Target = str>,
{
    fn eq(&self, other: &Vec<SubscribeTopic<S>>) -> bool {
        self.into_iter().eq(other.iter().map(|s| s.into()))
    }
}

impl<'a> From<SubscribeTopic<&'a str>> for SubscribeTopic<String> {
    fn from(value: SubscribeTopic<&'a str>) -> Self {
        Self {
            topic: value.topic.into(),
            qos: value.qos,
        }
    }
}
