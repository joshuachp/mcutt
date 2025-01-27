//! Owned values for the Fixed Header.

use alloc::string::String;

use super::{Str, StrRef};

/// [`Str`] with owned data.
pub type StrOwned = Str<String>;

impl<'a> From<StrRef<'a>> for StrOwned {
    fn from(value: StrRef<'a>) -> Self {
        Self(value.0.into())
    }
}
