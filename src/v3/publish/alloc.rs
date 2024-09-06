//! Owned values for the [`Publish`](super::Publish).

use alloc::{string::String, vec::Vec};

use super::{ClientPublish, ClientPublishRef, Publish, PublishRef, PublishTopic, PublishTopicRef};

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

/// [`Topic`](PublishTopic) with owned data.
pub type PublishTopicOwned = PublishTopic<String>;

impl<'a> From<PublishTopicRef<'a>> for PublishTopicOwned {
    fn from(value: PublishTopicRef<'a>) -> Self {
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
