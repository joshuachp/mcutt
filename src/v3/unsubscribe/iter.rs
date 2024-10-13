//! Iterators over a [`Unsubscribe`](super::Unsubscribe) filters.

use core::{marker::PhantomData, ops::Deref};

use super::UnsubscribeTopic;

/// Iterator over the [`Unsubscribe`](super::Unsubscribe) filters
#[derive(Debug, Clone, Copy)]
pub struct Iter<'a, I: 'a> {
    iter: I,
    // Marker to capture a lifetime iterator items
    _marker: PhantomData<&'a ()>,
}

impl<'a, I: 'a> Iter<'a, I> {
    pub(crate) fn new(iter: I) -> Self {
        Self {
            iter,
            _marker: PhantomData,
        }
    }
}

impl<'a, I, S> Iterator for Iter<'a, I>
where
    I: Iterator<Item = &'a UnsubscribeTopic<S>>,
    S: Deref<Target = str> + 'a,
{
    type Item = UnsubscribeTopic<&'a str>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|topic| topic.into())
    }
}
