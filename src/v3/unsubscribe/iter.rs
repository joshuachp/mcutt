//! Iterators over a [`Unsubscribe`](super::Unsubscribe) filters.

use core::ops::Deref;

use super::UnsubscribeTopic;

/// Iterator over the [`Unsubscribe`](super::Unsubscribe) filters
#[derive(Debug, Clone, Copy)]
pub struct Iter<I> {
    iter: I,
}

impl<'a, I: 'a> Iter<I> {
    pub(crate) fn new<T>(into: T) -> Self
    where
        T: IntoIterator<IntoIter = I>,
    {
        Self {
            iter: into.into_iter(),
        }
    }
}

impl<'a, I, S> Iterator for Iter<I>
where
    I: Iterator<Item = &'a UnsubscribeTopic<S>>,
    S: Deref<Target = str> + 'a,
{
    type Item = UnsubscribeTopic<&'a str>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|topic| topic.into())
    }
}
