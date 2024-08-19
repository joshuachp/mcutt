//! Owned values for the [`Publish`](super::Publish).

use alloc::{string::String, vec::Vec};

use super::{ClientPublish, ClientPublishRef, Publish, PublishRef, Topic, TopicRef};

/// [`Publish`] with Owned data.
pub type PublishOwned = Publish<String, Vec<u8>>;

impl<'a> From<PublishRef<'a>> for PublishOwned {
    fn from(value: PublishRef<'a>) -> Self {
        Self {
            qos: value.qos,
            retain: value.retain,
            topic: value.topic.into(),
            payload: value.payload.into(),
        }
    }
}

/// [`Topic`] with owned data.
pub type TopicOwned = Topic<String>;

impl<'a> From<TopicRef<'a>> for TopicOwned {
    fn from(value: TopicRef<'a>) -> Self {
        Self(value.0.into())
    }
}

/// [`ClientPublish`] with owned data.
pub type ClientPublishOwned = ClientPublish<String, Vec<u8>>;

impl<'a> From<ClientPublishRef<'a>> for ClientPublishOwned {
    fn from(value: ClientPublishRef<'a>) -> Self {
        Self {
            retain: value.retain,
            topic: value.topic.into(),
            payload: value.payload.into(),
        }
    }
}
