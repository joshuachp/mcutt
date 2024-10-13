//! Subscribe packet structs when allocations are enabled.

use core::ops::Deref;

use alloc::{string::String, vec::Vec};

use super::{UnsubscribeCursor, UnsubscribeTopic};

impl<'a> UnsubscribeCursor<'a> {
    /// Converts the filter to an owned value.
    pub fn to_vec(&self) -> Vec<UnsubscribeTopic<String>> {
        self.into_iter().map(|f| f.into()).collect()
    }
}

impl<'a, S> PartialEq<Vec<UnsubscribeTopic<S>>> for UnsubscribeCursor<'a>
where
    S: Deref<Target = str>,
{
    fn eq(&self, other: &Vec<UnsubscribeTopic<S>>) -> bool {
        self.into_iter().eq(other.iter().map(|s| s.into()))
    }
}

impl<'a> From<UnsubscribeTopic<&'a str>> for UnsubscribeTopic<String> {
    fn from(value: UnsubscribeTopic<&'a str>) -> Self {
        Self {
            topic: value.topic.into(),
        }
    }
}
