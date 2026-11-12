//! In-memory final-link values with conservative discovery of the actual Linux
//! clang/ELF linker inputs. An unrecognized flag, script or dependency disables
//! reuse; no guessed platform-library identity is accepted.
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zil_base::{Fingerprint, FingerprintBuilder};

const MAX_FILE: u64 = 256 * 1024 * 1024;
const MAX_FILES: usize = 512;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Product {
    pub(crate) key: Fingerprint,
    pub(crate) bytes: Vec<u8>,
    pub(crate) mode: u32,
}
fn words(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut started = false;
    for ch in text.chars() {
        if escaped {
            word.push(ch);
            escaped = false;
            started = true;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
            started = true;
        } else if ch.is_whitespace() && !quoted {
            if started {
                words.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(ch);
            started = true;
        }
    }
    if quoted || escaped {
        return None;
    }
    if started {
        words.push(word);
    }
    Some(words)
}
fn tool(path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        return fs::canonicalize(path).ok();
    }
    if path.components().count() != 1 {
        return fs::canonicalize(path).ok();
    }
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| {
        let candidate = dir.join(path);
        candidate
            .is_file()
            .then(|| fs::canonicalize(candidate).ok())
            .flatten()
    })
}
fn probe(command: &mut Command, commands: &mut Vec<Vec<String>>) -> Option<std::process::Output> {
    let mut recorded = vec![command.get_program().to_string_lossy().into_owned()];
    recorded.extend(
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned()),
    );
    commands.push(recorded);
    command.output().ok()
}
struct Inputs {
    hash: FingerprintBuilder,
    seen: BTreeSet<PathBuf>,
    searches: Vec<PathBuf>,
    needed: BTreeSet<String>,
}
impl Inputs {
    fn field(&mut self, bytes: &[u8]) {
        self.hash.update(&(bytes.len() as u64).to_le_bytes());
        self.hash.update(bytes);
    }
    fn directory(&mut self, path: &Path) -> Option<()> {
        let path = self.inventory(path)?;
        self.searches.push(path);
        Some(())
    }
    fn inventory(&mut self, path: &Path) -> Option<PathBuf> {
        self.field(path.as_os_str().as_encoded_bytes());
        if !path.exists() {
            self.field(b"ABSENT-SEARCH-DIRECTORY");
            return Some(path.to_path_buf());
        }
        let path = fs::canonicalize(path).ok()?;
        let mut names = fs::read_dir(&path)
            .ok()?
            .map(|entry| entry.ok().map(|e| e.file_name()))
            .collect::<Option<Vec<_>>>()?;
        if names.len() > 65_536 {
            return None;
        }
        names.sort();
        self.field(path.as_os_str().as_encoded_bytes());
        for name in names {
            self.field(name.as_encoded_bytes());
        }
        Some(path)
    }
    fn library(&mut self, name: &str) -> Option<()> {
        if name.is_empty() || name.contains('/') {
            return None;
        }
        let candidates = self
            .searches
            .iter()
            .flat_map(|directory| {
                ["so", "a"].map(|suffix| directory.join(format!("lib{name}.{suffix}")))
            })
            .collect::<Vec<_>>();
        let mut found = false;
        for path in candidates {
            if path.is_file() {
                found = true;
                // GNU ld can skip incompatible earlier candidates. Bind the
                // entire possible resolution set, not our guess at its choice.
                // An unsupported candidate disables reuse conservatively.
                self.file(&path, true)?;
            }
        }
        found.then_some(())
    }
    fn file(&mut self, original: &Path, script: bool) -> Option<()> {
        let path = fs::canonicalize(original).ok()?;
        self.field(original.as_os_str().as_encoded_bytes());
        self.field(path.as_os_str().as_encoded_bytes());
        if !self.seen.insert(path.clone()) {
            return Some(());
        }
        if self.seen.len() > MAX_FILES {
            return None;
        }
        let meta = fs::metadata(&path).ok()?;
        if !meta.is_file() || (script && meta.len() > MAX_FILE) {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            self.field(&meta.permissions().mode().to_le_bytes());
        }
        if !script {
            // Match the object cache's explicit trusted-installed-tool model:
            // GNU build ID plus complete file metadata, content SHA fallback.
            // Link libraries and scripts below still use full content bytes.
            self.field(&crate::object_cache::installed_identity(&path).ok()?.0);
            return Some(());
        }
        let bytes = fs::read(&path).ok()?;
        self.field(&bytes);
        if bytes.starts_with(b"\x7fELF") {
            self.needed.extend(elf_needed(&bytes)?);
            return Some(());
        }
        if bytes.starts_with(b"!<arch>\n") {
            native_archive(&bytes)?;
            return Some(());
        }
        let text = std::str::from_utf8(&bytes).ok()?;
        // The supported glibc linker scripts contain only these directives and
        // absolute INPUT/GROUP members. Reject SEARCH_DIR/INCLUDE and all other
        // syntax rather than guessing hidden lookup roots.
        let mut without_comments = String::new();
        let mut rest = text;
        while let Some(start) = rest.find("/*") {
            without_comments.push_str(&rest[..start]);
            let end = rest[start + 2..].find("*/")? + start + 4;
            rest = &rest[end..];
        }
        without_comments.push_str(rest);
        let tokens = without_comments
            .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ';' | ','))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let mut format = false;
        for token in tokens {
            if format {
                if token != "elf64-x86-64" {
                    return None;
                }
                format = false;
                continue;
            }
            match token {
                "OUTPUT_FORMAT" => format = true,
                "INPUT" | "GROUP" | "AS_NEEDED" => {}
                token if token.starts_with('/') => self.file(Path::new(token), true)?,
                token if token.starts_with("-l") => self.library(&token[2..])?,
                token
                    if !token.contains('/') && !token.contains(':') && token.starts_with("lib") =>
                {
                    let script_directory = path.parent()?.to_path_buf();
                    let current_directory = std::env::current_dir().ok()?;
                    let candidates = std::iter::once(&script_directory)
                        .chain(std::iter::once(&current_directory))
                        .chain(self.searches.iter())
                        .map(|directory| directory.join(token))
                        .collect::<Vec<_>>();
                    let mut selected = Vec::new();
                    for candidate in candidates {
                        // Only this precise basename is searched in cwd. A new
                        // staging directory must not masquerade as a library
                        // lookup change, while a newly added earlier file must.
                        self.field(candidate.as_os_str().as_encoded_bytes());
                        let present = candidate.is_file();
                        self.field(&[u8::from(present)]);
                        if present {
                            selected.push(candidate);
                        }
                    }
                    if selected.is_empty() {
                        return None;
                    }
                    for candidate in selected {
                        self.file(&candidate, true)?;
                    }
                }
                _ => return None,
            }
        }
        (!format).then_some(())
    }
    fn dynamic_dependencies(
        &mut self,
        executable: &Path,
        commands: &mut Vec<Vec<String>>,
    ) -> Option<()> {
        let ldd = Path::new("/usr/bin/ldd");
        self.file(ldd, false)?;
        let output = probe(Command::new(ldd).arg(executable), commands)?;
        if !output.status.success() {
            return None;
        }
        let text = std::str::from_utf8(&output.stdout).ok()?;
        for line in text.lines() {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.first().is_some_and(|s| s.starts_with("linux-vdso.")) {
                continue;
            }
            let path = if fields.get(1) == Some(&"=>") {
                *fields.get(2)?
            } else {
                *fields.first()?
            };
            if !path.starts_with('/') {
                return None;
            }
            self.file(Path::new(path), false)?;
        }
        Some(())
    }
}
/// Discover each time: the current driver command and search-directory inventory
/// prevent a newly installed earlier library from silently preserving the key.
pub(crate) fn key(
    clang: &Path,
    target: &str,
    optimization: &str,
    objects: &[PathBuf],
    output: &Path,
    threads: bool,
    commands: &mut Vec<Vec<String>>,
) -> Option<Fingerprint> {
    if target != "x86_64-unknown-linux-gnu"
        || objects.is_empty()
        || ["LD_PRELOAD", "LD_AUDIT"]
            .iter()
            .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty()))
    {
        return None;
    }
    let command_clang = clang;
    let clang = tool(clang)?;
    let mut command = Command::new(command_clang);
    command
        .arg("-###")
        .arg(format!("--target={target}"))
        .arg(optimization)
        .args(objects)
        .arg("-o")
        .arg(output);
    if threads {
        command.arg("-pthread");
    }
    let result = probe(&mut command, commands)?;
    if !result.status.success() {
        return None;
    }
    let text = std::str::from_utf8(&result.stderr).ok()?;
    let candidates = text
        .lines()
        .filter(|line| line.trim_start().starts_with('"'))
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return None;
    }
    let args = words(candidates[0])?;
    let linker = Path::new(args.first()?);
    if !linker.is_absolute() || !linker.file_name()?.to_str()?.starts_with("ld") {
        return None;
    }
    let mut input = Inputs {
        hash: FingerprintBuilder::new(),
        seen: BTreeSet::new(),
        searches: Vec::new(),
        needed: BTreeSet::new(),
    };
    input.field(b"ZIL-SESSION-FINAL-LINK-v1");
    let mut environment = std::env::vars_os().collect::<Vec<(OsString, OsString)>>();
    environment.sort();
    for (name, value) in environment {
        input.field(name.as_encoded_bytes());
        input.field(value.as_encoded_bytes());
    }
    input.file(&clang, false)?;
    input.dynamic_dependencies(&clang, commands)?;
    input.file(linker, false)?;
    input.dynamic_dependencies(linker, commands)?;
    // The built-in GNU linker script adds search roots beyond clang's -L.
    // Capture the complete script and every root, including currently absent
    // directories whose later creation can change resolution.
    let default_script = probe(Command::new(linker).arg("--verbose"), commands)?;
    if !default_script.status.success() {
        return None;
    }
    let default_script = std::str::from_utf8(&default_script.stdout).ok()?;
    if !default_script.contains("SEARCH_DIR(") {
        return None;
    }
    input.field(default_script.as_bytes());
    let mut default_roots = Vec::new();
    let mut script = default_script;
    while let Some(start) = script.find("SEARCH_DIR(\"") {
        script = &script[start + "SEARCH_DIR(\"".len()..];
        let end = script.find("\")")?;
        let root = script[..end].strip_prefix('=').unwrap_or(&script[..end]);
        if !root.starts_with('/') {
            return None;
        }
        default_roots.push(PathBuf::from(root));
        script = &script[end + 2..];
    }
    // Resolve all search directories before any -l or linker-script members.
    for arg in args.iter().skip(1) {
        if let Some(directory) = arg.strip_prefix("-L") {
            input.directory(Path::new(directory))?;
        }
    }
    for root in default_roots {
        input.directory(&root)?;
    }
    let mut args = args.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-o" {
            if Path::new(args.next()?) != output {
                return None;
            }
            input.field(b"OUTPUT");
        } else if let Some(index) = objects
            .iter()
            .position(|object| object.as_os_str() == std::ffi::OsStr::new(arg))
        {
            input.field(b"OBJECT");
            input.field(&(index as u64).to_le_bytes());
            let bytes = fs::read(&objects[index]).ok()?;
            input.field(&bytes);
        } else if arg.starts_with("-L") {
            input.field(arg.as_bytes());
        } else if let Some(name) = arg.strip_prefix("-l") {
            input.field(arg.as_bytes());
            input.library(name)?;
        } else if arg.starts_with('/') {
            input.file(Path::new(arg), true)?;
        } else if matches!(
            arg.as_str(),
            "-m" | "-dynamic-linker" | "-plugin" | "-plugin-opt" | "--hash-style"
        ) {
            input.field(arg.as_bytes());
            let value = args.next()?;
            input.field(value.as_bytes());
            match arg.as_str() {
                "-dynamic-linker" | "-plugin" => input.file(Path::new(value), false)?,
                "-m" if value != "elf_x86_64" => return None,
                "-plugin-opt" | "--hash-style" => return None,
                _ => {}
            }
        } else if matches!(
            arg.as_str(),
            "-pie"
                | "--eh-frame-hdr"
                | "--build-id"
                | "--as-needed"
                | "--no-as-needed"
                | "--start-group"
                | "--end-group"
                | "-Bdynamic"
                | "--enable-new-dtags"
                | "--hash-style=gnu"
        ) {
            input.field(arg.as_bytes());
        } else {
            return None;
        }
    }
    // The supported glibc/GCC native link has only these dynamic link-time
    // dependencies, already present as explicit resolved inputs. More complex
    // DSO closures require a separate resolution proof and use the normal link.
    for needed in &input.needed {
        if !matches!(needed.as_str(), "libc.so.6" | "ld-linux-x86-64.so.2")
            || !input.seen.iter().any(|path| {
                path.file_name()
                    .is_some_and(|name| name == std::ffi::OsStr::new(needed))
            })
        {
            return None;
        }
    }
    Some(input.hash.digest())
}
fn elf_needed(bytes: &[u8]) -> Option<Vec<String>> {
    fn u16at(b: &[u8], at: usize) -> Option<u16> {
        Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
    }
    fn u32at(b: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
    }
    fn u64at(b: &[u8], at: usize) -> Option<u64> {
        Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
    }
    if bytes.get(..6)? != b"\x7fELF\x02\x01" || u16at(bytes, 18)? != 62 {
        return None;
    }
    if u16at(bytes, 16)? == 1 {
        return Some(Vec::new());
    }
    if !matches!(u16at(bytes, 16)?, 2 | 3) {
        return None;
    }
    let offset = usize::try_from(u64at(bytes, 32)?).ok()?;
    let width = usize::from(u16at(bytes, 54)?);
    let count = usize::from(u16at(bytes, 56)?);
    if width < 56 || count > 4096 {
        return None;
    }
    let mut loads = Vec::new();
    let mut dynamic = None;
    for index in 0..count {
        let at = offset.checked_add(index.checked_mul(width)?)?;
        let file_offset = usize::try_from(u64at(bytes, at + 8)?).ok()?;
        let address = u64at(bytes, at + 16)?;
        let size = usize::try_from(u64at(bytes, at + 32)?).ok()?;
        match u32at(bytes, at)? {
            1 => loads.push((address, file_offset, size)),
            2 if dynamic.replace((file_offset, size)).is_some() => return None,
            _ => {}
        }
    }
    let Some((offset, size)) = dynamic else {
        return Some(Vec::new());
    };
    if size > 1024 * 1024 {
        return None;
    }
    let data = bytes.get(offset..offset.checked_add(size)?)?;
    let mut strings = None;
    let mut string_size = None;
    let mut needed = Vec::new();
    for row in data.as_chunks::<16>().0 {
        match u64at(row, 0)? {
            0 => break,
            1 => needed.push(usize::try_from(u64at(row, 8)?).ok()?),
            5 => strings = Some(u64at(row, 8)?),
            10 => string_size = Some(usize::try_from(u64at(row, 8)?).ok()?),
            // Filters, auxiliary DSOs and auditing can load extra link inputs.
            0x7fff_ffff | 0x7fff_fffd | 0x6fff_fefb | 0x6fff_fefc => return None,
            _ => {}
        }
    }
    if needed.is_empty() {
        return Some(Vec::new());
    }
    let address = strings?;
    let size = string_size?;
    let (base, file_offset, _) = loads.iter().find(|(base, _, len)| {
        address >= *base
            && address
                .checked_sub(*base)
                .is_some_and(|delta| delta < *len as u64)
    })?;
    let start = file_offset.checked_add(usize::try_from(address - *base).ok()?)?;
    let strings = bytes.get(start..start.checked_add(size)?)?;
    needed
        .into_iter()
        .map(|offset| {
            let value = strings.get(offset..)?;
            let end = value.iter().position(|byte| *byte == 0)?;
            Some(std::str::from_utf8(&value[..end]).ok()?.to_owned())
        })
        .collect()
}
fn native_archive(bytes: &[u8]) -> Option<()> {
    let mut offset = 8usize;
    while offset < bytes.len() {
        let header = bytes.get(offset..offset.checked_add(60)?)?;
        if &header[58..] != b"`\n" {
            return None;
        }
        let size: usize = std::str::from_utf8(&header[48..58])
            .ok()?
            .trim()
            .parse()
            .ok()?;
        let start = offset.checked_add(60)?;
        let member = bytes.get(start..start.checked_add(size)?)?;
        let name = std::str::from_utf8(&header[..16]).ok()?.trim();
        if !matches!(name, "/" | "//" | "/SYM64/") && !elf_needed(member)?.is_empty() {
            return None;
        }
        offset = start.checked_add(size)?.checked_add(size % 2)?;
    }
    (offset == bytes.len()).then_some(())
}
impl Product {
    pub(crate) fn capture(key: Fingerprint, executable: &Path) -> Option<Self> {
        let meta = fs::symlink_metadata(executable).ok()?;
        if !meta.is_file() || meta.len() > MAX_FILE {
            return None;
        }
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode()
        };
        #[cfg(not(unix))]
        let mode = 0;
        Some(Self {
            key,
            bytes: fs::read(executable).ok()?,
            mode,
        })
    }
    pub(crate) fn restore(&self, executable: &Path) -> std::io::Result<()> {
        fs::write(executable, &self.bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(executable, fs::Permissions::from_mode(self.mode))?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn driver_quoting_preserves_paths_and_rejects_incomplete_words() {
        assert_eq!(
            words(" \"/a b/ld\" \"-o\" \"/tmp/a\\\"b\""),
            Some(vec!["/a b/ld".into(), "-o".into(), "/tmp/a\"b".into()])
        );
        assert!(words("\"unterminated").is_none());
        assert!(words("trailing\\").is_none());
    }
}

#[cfg(test)]
mod dependency_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "zil-link-inputs-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn elf(marker: u8) -> Vec<u8> {
        let mut bytes = vec![0; 64];
        bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
        bytes[16] = 1;
        bytes[18] = 62;
        bytes[63] = marker;
        bytes
    }
    fn discover(first: &Path, second: &Path) -> Fingerprint {
        let mut inputs = Inputs {
            hash: FingerprintBuilder::new(),
            seen: BTreeSet::new(),
            searches: Vec::new(),
            needed: BTreeSet::new(),
        };
        inputs.directory(first).unwrap();
        inputs.directory(second).unwrap();
        inputs.library("sample").unwrap();
        inputs.hash.digest()
    }
    #[test]
    fn library_contents_and_new_earlier_resolution_invalidate_link_value() {
        let directory = Directory::new();
        let first = directory.0.join("first");
        let second = directory.0.join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        fs::write(second.join("libsample.so"), elf(1)).unwrap();
        let original = discover(&first, &second);
        fs::write(second.join("libsample.so"), elf(2)).unwrap();
        let changed = discover(&first, &second);
        assert_ne!(original, changed);
        fs::write(first.join("libsample.so"), elf(2)).unwrap();
        assert_ne!(changed, discover(&first, &second));
    }
    #[test]
    fn linker_script_dependencies_are_recursive_and_unknown_directives_fall_back() {
        let directory = Directory::new();
        let script = directory.0.join("libsample.so");
        let member = directory.0.join("libsample.so.1");
        fs::write(&script, b"/* library script */ GROUP ( libsample.so.1 )").unwrap();
        fs::write(&member, elf(1)).unwrap();
        let original = discover(&directory.0, &directory.0);
        fs::write(&member, elf(2)).unwrap();
        assert_ne!(original, discover(&directory.0, &directory.0));
        fs::write(&script, b"INCLUDE arbitrary-script").unwrap();
        let mut inputs = Inputs {
            hash: FingerprintBuilder::new(),
            seen: BTreeSet::new(),
            searches: Vec::new(),
            needed: BTreeSet::new(),
        };
        assert!(inputs.file(&script, true).is_none());
    }
}
