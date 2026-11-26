//! http(s) destinations in a line of text, shared by the terminal and the
//! editors.

use std::ops::Range;

/// A destination found in text: the bytes that spell it and where it leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextLink {
    pub range: Range<usize>,
    pub uri: String,
}

/// The normalized `uri` when it is an http(s) URL with a host and no control
/// characters.
pub fn http_destination(uri: &str) -> Option<String> {
    if uri.chars().any(char::is_control) {
        return None;
    }
    let url = url::Url::parse(uri).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    Some(url.to_string())
}

/// The http(s) URLs written in `text`, trailing sentence punctuation and
/// unbalanced closing brackets left out. With `clipped`, a URL that runs to
/// the end of `text` is skipped, since the rest of it is not shown.
pub fn find_links(text: &str, clipped: bool) -> Vec<TextLink> {
    let bytes = text.as_bytes();
    let mut links = Vec::new();
    let mut resume = 0;
    for (start, _) in text.char_indices() {
        if start < resume {
            continue;
        }
        let tail = &text[start..];
        if !(tail
            .get(..7)
            .is_some_and(|s| s.eq_ignore_ascii_case("http://"))
            || tail
                .get(..8)
                .is_some_and(|s| s.eq_ignore_ascii_case("https://")))
        {
            continue;
        }
        if text[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let length = tail
            .find(|c: char| {
                c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | '`')
            })
            .unwrap_or(tail.len());
        let mut end = start + length;
        if clipped && end == text.len() {
            continue;
        }
        while end > start {
            let last = bytes[end - 1] as char;
            let unmatched = match last {
                ')' => Some(('(', ')')),
                ']' => Some(('[', ']')),
                '}' => Some(('{', '}')),
                _ => None,
            }
            .is_some_and(|(open, close)| {
                text[start..end].matches(close).count() > text[start..end].matches(open).count()
            });
            if ".,;:!?".contains(last) || unmatched {
                end -= 1;
            } else {
                break;
            }
        }
        if let Some(uri) = http_destination(&text[start..end]) {
            links.push(TextLink {
                range: start..end,
                uri,
            });
            resume = end;
        }
    }
    links
}

/// The URL in `line` that covers byte `offset`.
pub fn link_at(line: &str, offset: usize) -> Option<TextLink> {
    find_links(line, false)
        .into_iter()
        .find(|link| link.range.contains(&offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uris(text: &str) -> Vec<String> {
        find_links(text, false)
            .into_iter()
            .map(|link| link.uri)
            .collect()
    }

    #[test]
    fn rejects_other_schemes_controls_and_hostless_urls() {
        for uri in [
            "file:///tmp/a",
            "javascript:alert(1)",
            "https://test/\nfoo",
            "https://",
        ] {
            assert!(http_destination(uri).is_none(), "{uri:?}");
        }
    }

    #[test]
    fn trims_sentence_punctuation_and_unbalanced_delimiters() {
        assert_eq!(
            uris(
                "(https://test/a(b)). https://[::1]/x], HTTPS://test/end! wordhttps://ignored.test"
            ),
            ["https://test/a(b)", "https://[::1]/x", "https://test/end"]
        );
    }

    #[test]
    fn clipped_text_skips_a_url_running_to_its_end() {
        assert!(find_links("see https://example.test/long", true).is_empty());
        assert_eq!(find_links("https://example.test/a b", true).len(), 1);
    }

    #[test]
    fn ranges_are_byte_offsets_into_the_text() {
        let text = "界 see https://test/界 now";
        let links = find_links(text, false);
        assert_eq!(links.len(), 1);
        assert_eq!(&text[links[0].range.clone()], "https://test/界");
        assert_eq!(links[0].uri, "https://test/%E7%95%8C");
    }

    #[test]
    fn link_at_finds_only_the_url_under_the_offset() {
        let line = "a https://one.test b https://two.test";
        assert_eq!(link_at(line, 2).unwrap().uri, "https://one.test/");
        assert_eq!(link_at(line, 25).unwrap().uri, "https://two.test/");
        assert!(link_at(line, 0).is_none());
        assert!(link_at(line, 19).is_none());
        assert!(link_at(line, line.len()).is_none());
    }
}
