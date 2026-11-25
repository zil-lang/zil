// Text statistics (lines, words, bytes) for UTF-8 files.
import std.string

pub struct FileStats {
    pub lines: i64;
    pub words: i64;
    pub bytes: i64;
}

pub struct TotalStats {
    pub files: i64;
    pub lines: i64;
    pub words: i64;
    pub bytes: i64;
}

pub fn empty_totals() -> TotalStats {
    return TotalStats {
        files: 0,
        lines: 0,
        words: 0,
        bytes: 0,
    }
}

fn is_ascii_space(byte: u8) -> bool {
    return byte == 32u8 || byte == 9u8 || byte == 13u8 || byte == 10u8
}

fn count_words_in_slice(view: &[u8]) -> i64 {
    var words: i64 = 0
    var in_word = false
    var at: i64 = 0
    while at < view.len() {
        if is_ascii_space(view[at]) {
            in_word = false
        } else if !in_word {
            words += 1
            in_word = true
        }
        at += 1
    }
    return words
}

pub fn count_text(content: &string.String, byte_limit: i64) -> string.Error!FileStats {
    let raw = string.as_bytes(content)
    let split_lines = string.lines(content, byte_limit)?
    let line_count = store_len(split_lines)
    var word_count: i64 = 0
    var idx: i64 = 0
    while idx < line_count {
        let line = store_at(split_lines, idx)
        word_count += count_words_in_slice(string.as_bytes(line))
        idx += 1
    }
    return FileStats {
        lines: line_count,
        words: word_count,
        bytes: raw.len(),
    }
}

pub fn add_file_stats(total: &mut TotalStats, one: FileStats) {
    total.files += 1
    total.lines += one.lines
    total.words += one.words
    total.bytes += one.bytes
}
