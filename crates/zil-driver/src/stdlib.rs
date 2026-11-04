//! Ordinary, versioned ZIL library sources. These bytes confer no proof authority.
pub(super) fn source(path: &[String]) -> Option<&'static str> {
    match path {
        [root, name] if root == "std" => match name.as_str() {
            "string" => Some(include_str!("../../../lib/std/string.zil")),
            "utf8" => Some(include_str!("../../../lib/std/utf8.zil")),
            "text_edit" => Some(include_str!("../../../lib/std/text_edit.zil")),
            "store" => Some(include_str!("../../../lib/std/store.zil")),
            _ => None,
        },
        _ => None,
    }
}
