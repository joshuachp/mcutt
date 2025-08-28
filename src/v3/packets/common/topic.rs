//! Utilities for topics and topic filters.

use memchr::memchr2_iter;

pub(crate) fn has_valid_wildcard(topic: &str) -> bool {
    let bytes = topic.as_bytes();

    memchr2_iter(b'+', b'#', bytes).all(|wild_idx| {
        let is_valid_pre = wild_idx
            .checked_sub(1)
            .and_then(|idx| bytes.get(idx))
            .is_none_or(|c| *c == b'/');

        let is_valid_next = wild_idx
            .checked_add(1)
            .and_then(|idx| bytes.get(idx))
            .is_none_or(|c| *c == b'/' && bytes[wild_idx] == b'+');

        is_valid_pre && is_valid_next
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("a/b")]
    #[case("/")]
    fn not_wildcard_topic(#[case] topic: &str) {
        assert!(has_valid_wildcard(topic));
    }

    #[rstest]
    #[case("#")]
    #[case("sport/#")]
    #[case("sport/tennis/#")]
    fn multi_level_wildcard(#[case] topic: &str) {
        assert!(has_valid_wildcard(topic));
    }

    #[rstest]
    #[case("+/tennis/#")]
    #[case("sport/+/player1")]
    #[case("+")]
    #[case("/+")]
    #[case("+/+")]
    fn single_level_wildcard(#[case] topic: &str) {
        assert!(has_valid_wildcard(topic));
    }

    #[rstest]
    #[case("sport/tennis#")]
    #[case("sport/tennis/#/ranking")]
    #[case("sport+")]
    fn invalid_wildcard(#[case] topic: &str) {
        assert!(!has_valid_wildcard(topic));
    }
}
