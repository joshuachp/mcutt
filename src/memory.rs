use std::collections::btree_map::Entry;

use crate::slab::Slab;

struct InMemory {
    slab: Slab<Vec<Entry>>,
}
