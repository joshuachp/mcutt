//! Store for the packets with QoS is 1 or 2.
//!
//! This will permit to store the values and resend them when reconnecting with a clean session.

/// Structure to store the published packets
///
// TODO: use the lest recently used free entry, we could fil the Slab until we have to re-allocate
// and then reuse the free entries, if full re-allocate
#[derive(Debug, Clone)]
pub struct Slab<S> {
    max_items: u16,
    next_free: Option<u16>,
    store: S,
}

impl<S> Slab<S> {
    /// Create a new slab from the given store.
    #[must_use]
    pub fn new(max_items: u16) -> Self
    where
        S: Default,
    {
        Self {
            max_items,
            next_free: None,
            store: S::default(),
        }
    }
}

pub trait Access<T> {
    /// Insert a new value in the first free element if there is still space in the slab.
    fn insert<F, O>(&mut self, f: F) -> Option<O>
    where
        F: FnOnce(u16) -> (T, O);

    /// Insert a new value in the first free element if there is still space in the slab.
    ///
    /// # Errors
    ///
    /// If the closures returns an error.
    fn try_insert<F, O, E>(&mut self, f: F) -> Result<Option<O>, E>
    where
        F: FnOnce(u16) -> Result<(T, O), E>;

    /// Removes an element from the slab given the index.
    fn remove(&mut self, idx: u16) -> Option<T>;

    /// Returns a reference to an occupied element given the index.
    fn get(&self, idx: u16) -> Option<&T>;

    /// Returns a mutable reference to an occupied element given the index.
    fn get_mut(&mut self, idx: u16) -> Option<&mut T>;
}

#[cfg(feature = "alloc")]
impl<T> Slab<Vec<Entry<T>>> {
    /// Insert a new value in the first free element if there is still space in the slab.
    pub fn insert<F, O>(&mut self, f: F) -> Option<O>
    where
        F: FnOnce(u16) -> (T, O),
    {
        let idx = self.reserve_next()?;

        let (value, out) = (f)(idx);

        self.next_free = self.store.get_mut(usize::from(idx))?.store(value);

        Some(out)
    }

    /// Insert a new value in the first free element if there is still space in the slab.
    ///
    /// # Errors
    ///
    /// If the closures returns an error.
    pub fn try_insert<F, O, E>(&mut self, f: F) -> Result<Option<O>, E>
    where
        F: FnOnce(u16) -> Result<(T, O), E>,
    {
        let Some(idx) = self.reserve_next() else {
            return Ok(None);
        };

        let (value, out) = (f)(idx)?;

        // check to prevent panics, but should never happen
        let Some(entry) = self.store.get_mut(usize::from(idx)) else {
            return Ok(None);
        };

        self.next_free = entry.store(value);

        Ok(Some(out))
    }

    /// Returns the next free index or reserve a new empty one.
    fn reserve_next(&mut self) -> Option<u16> {
        if let Some(next_free) = self.next_free {
            return Some(next_free);
        }

        let idx = u16::try_from(self.store.len());
        debug_assert!(idx.is_ok(), "the length is bigger than u16::MAX");
        let idx = idx.ok()?;

        if idx >= self.max_items {
            return None;
        }

        if self.store.capacity() == self.store.len() {
            let additional = self
                .store
                .capacity()
                .saturating_mul(2)
                .min(self.max_items.into())
                .saturating_sub(self.store.len());

            self.store.reserve_exact(additional);
        }

        self.store.push(Entry::Free { next_free: None });

        Some(idx)
    }

    /// Removes an element from the slab given the index.
    pub fn remove(&mut self, idx: u16) -> Option<T> {
        let item = self.store.get_mut(usize::from(idx))?;

        let prev = core::mem::replace(
            item,
            Entry::Free {
                next_free: self.next_free,
            },
        );

        match prev {
            Entry::Free { .. } => {
                *item = prev;

                None
            }
            Entry::Occupied(value) => Some(value),
        }
    }

    /// Returns a reference to an occupied element given the index.
    pub fn get(&self, idx: u16) -> Option<&T> {
        self.store
            .get(usize::from(idx))
            .and_then(Entry::as_occupied)
    }

    /// Returns a mutable reference to an occupied element given the index.
    pub fn get_mut(&mut self, idx: u16) -> Option<&mut T> {
        self.store
            .get_mut(usize::from(idx))
            .and_then(Entry::as_occupied_mut)
    }
}

/// Slab element
#[derive(Debug)]
pub enum Entry<T> {
    /// A free element
    Free {
        /// The next free element of the slab
        next_free: Option<u16>,
    },
    /// A occupied  element
    Occupied(T),
}

impl<T> Entry<T> {
    /// Returns `true` if the entry is [`Free`].
    ///
    /// [`Free`]: Entry::Free
    #[must_use]
    fn is_free(&self) -> bool {
        matches!(self, Self::Free { .. })
    }

    /// Stores the value returning the next free entry.
    fn store(&mut self, value: T) -> Option<u16> {
        debug_assert!(self.is_free(), "BUG: storing in an already occupied entry");

        match core::mem::replace(self, Entry::Occupied(value)) {
            Entry::Free { next_free } => next_free,
            Entry::Occupied(_) => None,
        }
    }

    /// Returns a reference to the value of an [`Occupied`](Entry::Occupied) entry.
    pub fn as_occupied(&self) -> Option<&T> {
        if let Self::Occupied(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Returns a mutable reference to the value of an [`Occupied`](Entry::Occupied) entry.
    pub fn as_occupied_mut(&mut self) -> Option<&mut T> {
        if let Self::Occupied(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
