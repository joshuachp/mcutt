//! Subscribe packet structs when allocations are enabled.

use core::ops::Deref;

use alloc::{string::String, vec::Vec};

use super::{FilterCursor, TopicFilter};

impl<'a> FilterCursor<'a> {
    /// Converts the filter to an owned value.
    pub fn to_vec(&self) -> Vec<TopicFilter<String>> {
        self.into_iter().map(|f| f.into()).collect()
    }
}

impl<'a, S> PartialEq<Vec<TopicFilter<S>>> for FilterCursor<'a>
where
    S: Deref<Target = str>,
{
    fn eq(&self, other: &Vec<TopicFilter<S>>) -> bool {
        self.into_iter().eq(other.iter().map(|s| s.into()))
    }
}

impl<'a> From<TopicFilter<&'a str>> for TopicFilter<String> {
    fn from(value: TopicFilter<&'a str>) -> Self {
        Self {
            topic: value.topic.into(),
            qos: value.qos,
        }
    }
}
