//! Ordinary, versioned ZIL library sources. These bytes confer no proof authority.
pub(super) fn source(path: &[String]) -> Option<&'static str> {
    match path {
        [root, name] if root == "std" => match name.as_str() {
            "string" => Some(include_str!("../../../lib/std/string.zil")),
            "utf8" => Some(include_str!("../../../lib/std/utf8.zil")),
            "text_edit" => Some(include_str!("../../../lib/std/text_edit.zil")),
            "store" => Some(include_str!("../../../lib/std/store.zil")),
            "iter" => Some(include_str!("../../../lib/std/iter.zil")),
            "io" => Some(include_str!("../../../lib/std/io.zil")),
            "collections" => Some(include_str!("../../../lib/std/collections.zil")),
            "string_map" => Some(include_str!("../../../lib/std/string_map.zil")),
            "pod_hash_map" => Some(include_str!("../../../lib/std/pod_hash_map.zil")),
            "pod_hash_set" => Some(include_str!("../../../lib/std/pod_hash_set.zil")),
            "owning_entries" => Some(include_str!("../../../lib/std/owning_entries.zil")),
            "owning_hash_map" => Some(include_str!("../../../lib/std/owning_hash_map.zil")),
            "owning_hash_set" => Some(include_str!("../../../lib/std/owning_hash_set.zil")),
            "owning_map" => Some(include_str!("../../../lib/std/owning_map.zil")),
            "i64_hash_map" => Some(include_str!("../../../lib/std/i64_hash_map.zil")),
            "int" => Some(include_str!("../../../lib/std/int.zil")),
            "i64_hash_set" => Some(include_str!("../../../lib/std/i64_hash_set.zil")),
            "graph" => Some(include_str!("../../../lib/std/graph.zil")),
            "ui_layout" => Some(include_str!("../../../lib/std/ui_layout.zil")),
            "ui_raster" => Some(include_str!("../../../lib/std/ui_raster.zil")),
            "ui_input" => Some(include_str!("../../../lib/std/ui_input.zil")),
            _ => None,
        },
        _ => None,
    }
}
