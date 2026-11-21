//! What Everything is asked for a search: the query typed, within the filter chosen, narrowed to some names.
use crate::state::narrow_words;

/// What Everything is asked: the query narrowed by the filter's, each in `< >` so an `|` in one does not
/// reach into the other. With only spaces typed, the filter asks for all it lets through.
pub fn everything_query(query: &str, filter: Option<&str>) -> String {
    match filter {
        None => query.to_owned(),
        Some(filter) if query.trim().is_empty() => format!("<{}>", closed(filter)),
        Some(filter) => format!("<{}> <{}>", closed(filter), closed(query)),
    }
}

/// The search `query` narrowed to the names holding each word of `text` as it is, in any order, as the
/// columns Colet narrows itself are: not the folders they are in, and with no wildcards.
pub fn narrowed_query(query: &str, text: &str) -> String {
    let words: String = narrow_words(text).iter().map(|word| format!(" nopath:nowildcards:\"{word}\"")).collect();
    format!("<{}>{words}", closed(query))
}

/// `text` with a quote left open closed, as Everything closes it at the end of the text: inside `< >` it
/// would run on over the `>` and take it for a letter, finding nothing while `"my doc` is being typed.
fn closed(text: &str) -> std::borrow::Cow<'_, str> {
    if text.matches('"').count() % 2 == 1 { format!("{text}\"").into() } else { text.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_everything_for_the_query_within_the_filter() {
        assert_eq!(everything_query("cat", None), "cat");
        assert_eq!(everything_query("cat | dog", Some("ext:jpg;png")), "<ext:jpg;png> <cat | dog>");
        assert_eq!(everything_query(" ", Some("ext:jpg")), "<ext:jpg>");
    }

    #[test]
    fn closes_a_quote_left_open_inside_its_group() {
        assert_eq!(everything_query("\"my doc", Some("ext:txt")), "<ext:txt> <\"my doc\">");
        assert_eq!(everything_query("\"my doc\"", Some("ext:txt")), "<ext:txt> <\"my doc\">");
        assert_eq!(everything_query("cat", Some("path:\"D:\\My Pictures")), "<path:\"D:\\My Pictures\"> <cat>");
        assert_eq!(everything_query("", Some("path:\"D:\\a")), "<path:\"D:\\a\">");
        // Without a filter Everything is asked what was typed, and closes the quote itself.
        assert_eq!(everything_query("\"my doc", None), "\"my doc");
    }

    #[test]
    fn narrows_the_search_to_names_holding_the_text_as_it_is() {
        assert_eq!(narrowed_query("cat | dog", "a\u{3000}b"), "<cat | dog> nopath:nowildcards:\"a\" nopath:nowildcards:\"b\"");
        assert_eq!(narrowed_query("\"my doc", "*"), "<\"my doc\"> nopath:nowildcards:\"*\"");
        assert_eq!(narrowed_query("cat", "\"a b\" c|d"), "<cat> nopath:nowildcards:\"a b\" nopath:nowildcards:\"c|d\"");
    }
}
