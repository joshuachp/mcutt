//! Iterators over a [`Subscribe`](super::Subscribe) filters.

use core::{marker::PhantomData, ops::Deref, slice};

use super::{SubAckCode, SubAckCodeCursor, SubscribeTopic};

/// Iterator over the [`Subscribe`](super::Subscribe) filters
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
    I: Iterator<Item = &'a SubscribeTopic<S>>,
    S: Deref<Target = str> + 'a,
{
    type Item = SubscribeTopic<&'a str>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|topic| topic.into())
    }
}

/// Iterator of the [`SubAckCode`] for the [`SubAckCodeCursor`]
#[derive(Debug, Clone)]
pub struct SubAckCodeIter<'a> {
    iter: slice::Iter<'a, u8>,
}

impl<'a> SubAckCodeIter<'a> {
    pub(crate) fn new(cursor: &'a SubAckCodeCursor<'a>) -> Self {
        Self {
            iter: cursor.bytes.iter(),
        }
    }
}

impl<'a> Iterator for SubAckCodeIter<'a> {
    type Item = SubAckCode;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.iter.next()?;

        unwrap_return_code(next)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.iter.count()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let next = self.iter.nth(n)?;

        unwrap_return_code(next)
    }
}

impl<'a> ExactSizeIterator for SubAckCodeIter<'a> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

fn unwrap_return_code(next: &u8) -> Option<SubAckCode> {
    match SubAckCode::try_from(*next) {
        Ok(code) => Some(code),
        Err(err) => {
            // We checked the validity during construction of the cursor
            unreachable!("the cursor must be a valid filter: {err}")
        }
    }
}
