#![forbid(unsafe_code)]

//! Stable, dependency-free identities, diagnostics, limits, and fingerprints.

mod sha256;
mod source_map;
pub use source_map::*;

use std::fmt;

pub const RULE_VERSION: &str = "ZIL-LANG-1.3";
pub const SCHEMA_VERSION: u32 = 17;

// Keep identities untrusted and trivially constructible. They confer no authority.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u32);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId(pub u32);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(file: FileId, start: usize, end: usize) -> Self {
        Self { file, start, end }
    }
    pub fn join(self, other: Self) -> Self {
        if self.file != other.file {
            return self;
        }
        Self::new(
            self.file,
            self.start.min(other.start),
            self.end.max(other.end),
        )
    }
    pub fn is_valid_for(self, source: &SourceUnit) -> bool {
        self.file == source.file_id
            && self.start <= self.end
            && self.end <= source.text.len()
            && source.text.is_char_boundary(self.start)
            && source.text.is_char_boundary(self.end)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUnit {
    pub file_id: FileId,
    pub module_id: ModuleId,
    pub path: String,
    pub text: String,
}

impl SourceUnit {
    pub fn new(
        file_id: FileId,
        module_id: ModuleId,
        path: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            file_id,
            module_id,
            path: path.into(),
            text: text.into(),
        }
    }
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of(self.text.as_bytes())
    }
    pub fn span(&self) -> Span {
        Span::new(self.file_id, 0, self.text.len())
    }
    pub fn line_column(&self, offset: usize) -> Option<(usize, usize)> {
        if offset > self.text.len() || !self.text.is_char_boundary(offset) {
            return None;
        }
        let prefix = &self.text[..offset];
        Some((
            prefix.bytes().filter(|&x| x == b'\n').count() + 1,
            prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    Lex,
    Parse,
    Expand,
    Types,
    Analyze,
    Verify,
    Lower,
    Codegen,
    Link,
    Run,
    Ct,
    Driver,
}
impl Phase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lex => "lex",
            Self::Parse => "parse",
            Self::Expand => "expand",
            Self::Types => "types",
            Self::Analyze => "analyze",
            Self::Verify => "verify",
            Self::Lower => "lower",
            Self::Codegen => "codegen",
            Self::Link => "link",
            Self::Run => "run",
            Self::Ct => "ct",
            Self::Driver => "driver",
        }
    }
}
impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResultCategory {
    Success,
    SafeReject,
    Unsupported,
    ResourceLimit,
    InfraError,
    Cancelled,
}
impl ResultCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::SafeReject => "SafeReject",
            Self::Unsupported => "Unsupported",
            Self::ResourceLimit => "ResourceLimit",
            Self::InfraError => "InfraError",
            Self::Cancelled => "Cancelled",
        }
    }
}
impl fmt::Display for ResultCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub phase: Phase,
    pub category: ResultCategory,
    pub code: String,
    pub message: String,
    pub span: Option<Span>,
    pub notes: Vec<String>,
}
impl Diagnostic {
    pub fn new(
        phase: Phase,
        category: ResultCategory,
        code: impl Into<String>,
        message: impl Into<String>,
        span: Option<Span>,
    ) -> Self {
        Self {
            phase,
            category,
            code: code.into(),
            message: message.into(),
            span,
            notes: Vec::new(),
        }
    }
    pub fn error(
        phase: Phase,
        code: impl Into<String>,
        span: Option<Span>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(phase, ResultCategory::SafeReject, code, message, span)
    }
    pub fn unsupported(phase: Phase, feature: impl Into<String>, span: Option<Span>) -> Self {
        Self::new(
            phase,
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
            feature,
            span,
        )
    }
    pub fn limit(phase: Phase, message: impl Into<String>, span: Option<Span>) -> Self {
        Self::new(
            phase,
            ResultCategory::ResourceLimit,
            "E-RESOURCE-LIMIT",
            message,
            span,
        )
    }
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
    pub fn to_json(&self) -> String {
        let span = self.span.map_or_else(
            || "null".into(),
            |s| {
                format!(
                    "{{\"file\":{},\"start\":{},\"end\":{}}}",
                    s.file.0, s.start, s.end
                )
            },
        );
        format!(
            "{{\"phase\":{},\"category\":{},\"code\":{},\"message\":{},\"span\":{},\"notes\":[{}]}}",
            json_string(self.phase.as_str()),
            json_string(self.category.as_str()),
            json_string(&self.code),
            json_string(&self.message),
            span,
            self.notes
                .iter()
                .map(|n| json_string(n))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.phase, self.category, self.code, self.message
        )
    }
}
pub type CompileResult<T> = Result<T, Vec<Diagnostic>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_source_bytes: usize,
    pub max_tokens: usize,
    pub max_nodes: usize,
    pub max_functions: usize,
    pub max_blocks: usize,
    pub max_instructions: usize,
    pub max_locals: usize,
    pub max_values: usize,
    pub max_analysis_steps: usize,
    pub max_ct_steps: u64,
    pub max_call_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_source_bytes: 16 * 1024 * 1024,
            max_tokens: 1_000_000,
            max_nodes: 1_000_000,
            max_functions: 10_000,
            max_blocks: 100_000,
            max_instructions: 1_000_000,
            max_locals: 100_000,
            max_values: 1_000_000,
            max_analysis_steps: 10_000_000,
            max_ct_steps: 1_000_000,
            max_call_depth: 256,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Endian {
    Little,
    Big,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetProfile {
    pub id: String,
    pub triple: String,
    pub pointer_width: u16,
    pub endian: Endian,
    pub backend: String,
    pub rule_version: String,
    pub checked_arithmetic: bool,
}
impl TargetProfile {
    pub fn windows_x64_llvm() -> Self {
        Self {
            id: "win64-llvm".into(),
            triple: "x86_64-pc-windows-msvc".into(),
            pointer_width: 64,
            endian: Endian::Little,
            backend: "llvm".into(),
            rule_version: RULE_VERSION.into(),
            checked_arithmetic: true,
        }
    }
    pub fn is_supported_x64_llvm(&self) -> bool {
        matches!(
            self.triple.as_str(),
            "x86_64-unknown-linux-gnu" | "x86_64-pc-windows-msvc"
        ) && self.pointer_width == 64
            && self.endian == Endian::Little
            && self.backend == "llvm"
            && self.rule_version == RULE_VERSION
            && self.checked_arithmetic
    }
    pub fn host_llvm() -> Self {
        let triple = match (std::env::consts::ARCH, std::env::consts::OS) {
            ("x86_64", "linux") => "x86_64-unknown-linux-gnu".into(),
            ("aarch64", "linux") => "aarch64-unknown-linux-gnu".into(),
            ("x86_64", "windows") => "x86_64-pc-windows-msvc".into(),
            ("aarch64", "macos") => "aarch64-apple-darwin".into(),
            (arch, os) => format!("{arch}-unknown-{os}"),
        };
        Self {
            id: "native-llvm".into(),
            triple,
            pointer_width: usize::BITS as u16,
            endian: if cfg!(target_endian = "little") {
                Endian::Little
            } else {
                Endian::Big
            },
            backend: "llvm".into(),
            rule_version: RULE_VERSION.into(),
            checked_arithmetic: true,
        }
    }
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of(self.to_json().as_bytes())
    }
    pub fn to_json(&self) -> String {
        format!(
            "{{\"id\":{},\"triple\":{},\"pointer_width\":{},\"endian\":{},\"backend\":{},\"rule_version\":{},\"checked_arithmetic\":{}}}",
            json_string(&self.id),
            json_string(&self.triple),
            self.pointer_width,
            json_string(if self.endian == Endian::Little {
                "little"
            } else {
                "big"
            }),
            json_string(&self.backend),
            json_string(&self.rule_version),
            self.checked_arithmetic
        )
    }
}
impl Default for TargetProfile {
    fn default() -> Self {
        Self::host_llvm()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(pub [u8; 32]);
impl Fingerprint {
    pub const ZERO: Self = Self([0; 32]);
    pub fn of(bytes: &[u8]) -> Self {
        Self(sha256::digest(bytes))
    }
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(64);
        for b in self.0 {
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
        out
    }
    pub fn from_hex(text: &str) -> Option<Self> {
        if text.len() != 64 {
            return None;
        }
        let mut bytes = [0; 32];
        for (i, pair) in text.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let digit = |x: u8| match x {
                b'0'..=b'9' => Some(x - b'0'),
                b'a'..=b'f' => Some(x - b'a' + 10),
                b'A'..=b'F' => Some(x - b'A' + 10),
                _ => None,
            };
            bytes[i] = digit(pair[0])? * 16 + digit(pair[1])?;
        }
        Some(Self(bytes))
    }
}
impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Return a complete JSON string literal, including quotes. UTF-8 stays UTF-8.
pub fn json_string(text: &str) -> String {
    use fmt::Write;
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\u{0}'..='\u{1f}' => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}
pub fn json_escape(text: &str) -> String {
    let quoted = json_string(text);
    quoted[1..quoted.len() - 1].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sha256_known_answers() {
        assert_eq!(
            Fingerprint::of(b"").to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            Fingerprint::of(b"abc").to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            Fingerprint::of(&vec![b'a'; 1_000_000]).to_hex(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
    #[test]
    fn json_controls_and_unicode() {
        assert_eq!(json_string("\"\\\n\0汉"), "\"\\\"\\\\\\n\\u0000汉\"");
    }
    #[test]
    fn span_utf8_bounds() {
        let s = SourceUnit::new(FileId(1), ModuleId(1), "test", "中\nx");
        assert!(Span::new(FileId(1), 0, 3).is_valid_for(&s));
        assert!(!Span::new(FileId(1), 1, 3).is_valid_for(&s));
        assert_eq!(s.line_column(4), Some((2, 1)));
    }
}
