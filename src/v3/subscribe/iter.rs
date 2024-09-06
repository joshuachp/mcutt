//! Iterators over a [`Subscribe`](super::Subscribe) filters.

use core::{marker::PhantomData, ops::Deref, slice};

use crate::v3::Decode;

use super::{FilterCursor, ReturnCode, ReturnCodeCursor, TopicFilter};

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
    I: Iterator<Item = &'a TopicFilter<S>>,
    S: Deref<Target = str> + 'a,
{
    type Item = TopicFilter<&'a str>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|topic| topic.into())
    }
}

/// Iterator of the [`TopicFilter`] for the [`FilterCursor`]
#[derive(Debug, Clone, Copy)]
pub struct FilterIter<'a> {
    cursor: &'a FilterCursor<'a>,
    idx: usize,
}

impl<'a> FilterIter<'a> {
    pub(crate) fn new(cursor: &'a FilterCursor<'a>) -> Self {
        Self { cursor, idx: 0 }
    }
}

impl<'a> Iterator for FilterIter<'a> {
    type Item = TopicFilter<&'a str>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx == self.cursor.bytes.len() {
            return None;
        }

        let bytes = &self.cursor.bytes[self.idx..];

        let (filter, bytes) = match TopicFilter::parse(bytes) {
            Ok(res) => res,
            Err(err) => {
                // We checked the validity during construction of the cursor
                unreachable!("the cursor must be a valid filter: {err}")
            }
        };

        self.idx = self.cursor.bytes.len().saturating_sub(bytes.len());

        Some(filter)
    }
}

/// Iterator of the [`ReturnCode`] for the [`ReturnCodeCursor`]
#[derive(Debug, Clone)]
pub struct ReturnCodeIter<'a> {
    iter: slice::Iter<'a, u8>,
}

impl<'a> ReturnCodeIter<'a> {
    pub(crate) fn new(cursor: &'a ReturnCodeCursor<'a>) -> Self {
        Self {
            iter: cursor.bytes.iter(),
        }
    }
}

impl<'a> Iterator for ReturnCodeIter<'a> {
    type Item = ReturnCode;

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

impl<'a> ExactSizeIterator for ReturnCodeIter<'a> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

fn unwrap_return_code(next: &u8) -> Option<ReturnCode> {
    match ReturnCode::try_from(*next) {
        Ok(code) => Some(code),
        Err(err) => {
            // We checked the validity during construction of the cursor
            unreachable!("the cursor must be a valid filter: {err}")
        }
    }
}
