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
/// Parse the same closed ldd row domain for both single and multi-input probes.
/// Paths returned here remain observations: Inputs::file still binds each
/// current canonical owner and complete installed-tool identity below.
fn dependency_rows(text: &str) -> Option<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if text.trim().is_empty() {
        return None;
    }
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.first().is_some_and(|s| s.starts_with("linux-vdso.")) {
            continue;
        }
        let path = if fields.get(1) == Some(&"=>") {
            *fields.get(2)?
        } else {
            *fields.first()?
        };
        if !path.starts_with('/') || paths.len() >= MAX_FILES {
            return None;
        }
        paths.push(PathBuf::from(path));
    }
    Some(paths)
}
/// Multi-file ldd prefixes each result with its exact input spelling. Preserve
/// input ORDER, including duplicate paths; a map would silently lose a section.
/// Unknown/missing/extra/reordered headers cannot prove either executable's
/// dependencies and cause the caller to repeat the two original single probes.
fn dependency_pair_rows(text: &str, executables: [&Path; 2]) -> Option<[Vec<PathBuf>; 2]> {
    let mut headers = Vec::with_capacity(2);
    for executable in executables {
        let path = executable.to_str()?;
        if path.contains(['\n', '\r']) {
            return None;
        }
        headers.push(format!("{path}:"));
    }
    let mut bodies = [String::new(), String::new()];
    let mut next = 0;
    for line in text.lines() {
        if next < headers.len() && line == headers[next] {
            if next > 0 && bodies[next - 1].trim().is_empty() {
                return None;
            }
            next += 1;
        } else if line.trim().is_empty() {
            continue;
        } else {
            // Header-like data is never interpreted as a dependency path.
            // A legitimate unusual filename can still use single-input ldd.
            if next == 0 || line.trim_end().ends_with(':') {
                return None;
            }
            bodies[next - 1].push_str(line);
            bodies[next - 1].push('\n');
        }
    }
    if next != 2 {
        return None;
    }
    Some([dependency_rows(&bodies[0])?, dependency_rows(&bodies[1])?])
}
fn probe_dependency_pair(
    executables: [&Path; 2],
    mut run: impl FnMut(&[&Path]) -> Option<std::process::Output>,
) -> Option<[Vec<PathBuf>; 2]> {
    if let Some(output) = run(&executables)
        && output.status.success()
        && let Ok(text) = std::str::from_utf8(&output.stdout)
        && let Some(paths) = dependency_pair_rows(text, executables)
    {
        return Some(paths);
    }
    // Keep the complete original fallback for unsupported ldd variants. No
    // partial multi-probe paths enter the input hash or the file identity set.
    let mut paths = [Vec::new(), Vec::new()];
    for (index, executable) in executables.iter().enumerate() {
        let output = run(&[*executable])?;
        if !output.status.success() {
            return None;
        }
        paths[index] = dependency_rows(std::str::from_utf8(&output.stdout).ok()?)?;
    }
    Some(paths)
}
struct Inputs<'a> {
    hash: FingerprintBuilder,
    seen: BTreeSet<(PathBuf, bool)>,
    searches: Vec<PathBuf>,
    needed: BTreeSet<String>,
    tool_identities: Option<&'a mut crate::object_cache::InstalledToolIdentities>,
    #[cfg(test)]
    content_bytes_hashed: usize,
}
impl Inputs<'_> {
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
        if !self.seen.insert((path.clone(), script)) {
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
            let identity = if let Some(tools) = self.tool_identities.as_deref_mut() {
                tools.identity(&path).ok()?
            } else {
                crate::object_cache::installed_identity(&path).ok()?
            };
            self.field(&identity.0);
            return Some(());
        }
        let bytes = fs::read(&path).ok()?;
        self.field(&bytes);
        #[cfg(test)]
        {
            self.content_bytes_hashed += bytes.len();
        }
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
    fn dynamic_dependency_pair(
        &mut self,
        executables: [&Path; 2],
        commands: &mut Vec<Vec<String>>,
    ) -> Option<()> {
        let ldd = Path::new("/usr/bin/ldd");
        self.file(ldd, false)?;
        let paths = probe_dependency_pair(executables, |executables| {
            probe(Command::new(ldd).args(executables), commands)
        })?;
        for group in paths {
            for path in group {
                self.file(&path, false)?;
            }
        }
        Some(())
    }
}
pub(crate) struct DiscoveryContext<'a> {
    pub(crate) commands: &'a mut Vec<Vec<String>>,
    pub(crate) tools: &'a mut crate::object_cache::InstalledToolIdentities,
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
    context: &mut DiscoveryContext<'_>,
) -> Option<Fingerprint> {
    let DiscoveryContext { commands, tools } = context;
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
        tool_identities: Some(&mut **tools),
        #[cfg(test)]
        content_bytes_hashed: 0,
    };
    input.field(b"ZIL-SESSION-FINAL-LINK-v2");
    let mut environment = std::env::vars_os().collect::<Vec<(OsString, OsString)>>();
    environment.sort();
    for (name, value) in environment {
        input.field(name.as_encoded_bytes());
        input.field(value.as_encoded_bytes());
    }
    input.file(&clang, false)?;
    input.file(linker, false)?;
    input.dynamic_dependency_pair([&clang, linker], commands)?;
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
            || !input.seen.iter().any(|(path, content_bound)| {
                *content_bound
                    && path
                        .file_name()
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
        native_relocatable_sections(bytes)?;
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
/// A native ELF container can also carry fat LTO and cause GNU ld/BFD to
/// discover an automatic plugin. Such a plugin is not in the parsed driver
/// argv. Admit only the understood native section schema; unknown section
/// formats deliberately fall back to an ordinary current link.
fn native_relocatable_sections(bytes: &[u8]) -> Option<()> {
    fn u16at(bytes: &[u8], at: usize) -> Option<u16> {
        Some(u16::from_le_bytes(
            bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
        ))
    }
    fn u32at(bytes: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
        ))
    }
    fn u64at(bytes: &[u8], at: usize) -> Option<u64> {
        Some(u64::from_le_bytes(
            bytes.get(at..at.checked_add(8)?)?.try_into().ok()?,
        ))
    }
    if bytes.get(..7)? != b"\x7fELF\x02\x01\x01"
        || u16at(bytes, 16)? != 1
        || u16at(bytes, 18)? != 62
        || u32at(bytes, 20)? != 1
        || u32at(bytes, 48)? != 0
        || u16at(bytes, 52)? != 64
        || u16at(bytes, 56)? != 0
        || u16at(bytes, 58)? != 64
    {
        return None;
    }
    let offset = usize::try_from(u64at(bytes, 40)?).ok()?;
    let count = usize::from(u16at(bytes, 60)?);
    let names_index = usize::from(u16at(bytes, 62)?);
    // Extended numbering and absent names are outside this finite admission.
    if offset < 64 || count == 0 || names_index == 0 || names_index >= count {
        return None;
    }
    let table = bytes.get(offset..offset.checked_add(count.checked_mul(64)?)?)?;
    if table.get(..64)?.iter().any(|byte| *byte != 0) {
        return None;
    }
    let sections = table.as_chunks::<64>().0;
    let names_header = sections.get(names_index)?;
    if u32at(names_header, 4)? != 3 {
        return None;
    }
    let names_offset = usize::try_from(u64at(names_header, 24)?).ok()?;
    let names_size = usize::try_from(u64at(names_header, 32)?).ok()?;
    let names = bytes.get(names_offset..names_offset.checked_add(names_size)?)?;
    if names.first() != Some(&0) || names.last() != Some(&0) {
        return None;
    }
    let mut name_work = 1024 * 1024usize;
    for (index, section) in sections.iter().enumerate() {
        if index == 0 {
            continue;
        }
        let name_offset = usize::try_from(u32at(section, 0)?).ok()?;
        let name = names.get(name_offset..)?;
        let name_end = name[..name.len().min(name_work)]
            .iter()
            .position(|byte| *byte == 0)?;
        name_work = name_work.checked_sub(name_end.checked_add(1)?)?;
        let name = std::str::from_utf8(&name[..name_end]).ok()?;
        if name.starts_with(".gnu.lto_")
            || name.starts_with(".gnu.debuglto_")
            || name.starts_with(".gnu.offload")
            || name.starts_with(".llvm")
            || name.starts_with(".llvmbc")
            || name == ".gnu_object_only"
            || name == ".gnu.object_only"
            || name.contains("bitcode")
            || name.contains("plugin")
        {
            return None;
        }
        let kind = u32at(section, 4)?;
        // NULL is admitted only at index zero above. No OS/processor-specific
        // types or dynamic metadata are inferred to be plugin-independent.
        if !matches!(kind, 1..=4 | 7..=9 | 14..=18) {
            return None;
        }
        let flags = u64at(section, 8)?;
        const NATIVE_FLAGS: u64 =
            0x1 | 0x2 | 0x4 | 0x10 | 0x20 | 0x40 | 0x80 | 0x200 | 0x400 | 0x0020_0000 | 0x8000_0000;
        if flags & !NATIVE_FLAGS != 0 {
            return None;
        }
        let file_offset = usize::try_from(u64at(section, 24)?).ok()?;
        let size = usize::try_from(u64at(section, 32)?).ok()?;
        if kind == 8 && file_offset > bytes.len() {
            return None;
        }
        if kind != 8 {
            bytes.get(file_offset..file_offset.checked_add(size)?)?;
        }
        let link = usize::try_from(u32at(section, 40)?).ok()?;
        let info = usize::try_from(u32at(section, 44)?).ok()?;
        if link >= count {
            return None;
        }
        let alignment = u64at(section, 48)?;
        if alignment != 0 && !alignment.is_power_of_two() {
            return None;
        }
        let entry_size = usize::try_from(u64at(section, 56)?).ok()?;
        match kind {
            2 if entry_size != 24
                || !size.is_multiple_of(24)
                || u32at(sections.get(link)?, 4)? != 3 =>
            {
                return None;
            }
            4 | 9
                if entry_size != if kind == 4 { 24 } else { 16 }
                    || !size.is_multiple_of(entry_size)
                    || info >= count
                    || u32at(sections.get(link)?, 4)? != 2 =>
            {
                return None;
            }
            14..=16 if entry_size != 8 || !size.is_multiple_of(8) => return None,
            17 | 18
                if entry_size != 4
                    || !size.is_multiple_of(4)
                    || u32at(sections.get(link)?, 4)? != 2 =>
            {
                return None;
            }
            _ => {}
        }
    }
    Some(())
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
    fn relocatable(name: &str, kind: u32, marker: u8) -> Vec<u8> {
        let mut names = b"\0.shstrtab\0".to_vec();
        let name_offset = names.len() as u32;
        names.extend_from_slice(name.as_bytes());
        names.push(0);
        let mut bytes = vec![0; 64 + 3 * 64];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        bytes[16..18].copy_from_slice(&1u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        bytes[40..48].copy_from_slice(&64u64.to_le_bytes());
        bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
        bytes[58..60].copy_from_slice(&64u16.to_le_bytes());
        bytes[60..62].copy_from_slice(&3u16.to_le_bytes());
        bytes[62..64].copy_from_slice(&1u16.to_le_bytes());
        let strings = 64 + 64;
        bytes[strings..strings + 4].copy_from_slice(&1u32.to_le_bytes());
        bytes[strings + 4..strings + 8].copy_from_slice(&3u32.to_le_bytes());
        bytes[strings + 24..strings + 32].copy_from_slice(&256u64.to_le_bytes());
        bytes[strings + 32..strings + 40].copy_from_slice(&(names.len() as u64).to_le_bytes());
        bytes[strings + 48..strings + 56].copy_from_slice(&1u64.to_le_bytes());
        let payload = 64 + 2 * 64;
        bytes[payload..payload + 4].copy_from_slice(&name_offset.to_le_bytes());
        bytes[payload + 4..payload + 8].copy_from_slice(&kind.to_le_bytes());
        bytes[payload + 24..payload + 32]
            .copy_from_slice(&(256u64 + names.len() as u64).to_le_bytes());
        bytes[payload + 32..payload + 40].copy_from_slice(&1u64.to_le_bytes());
        bytes[payload + 48..payload + 56].copy_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&names);
        bytes.push(marker);
        bytes
    }
    fn elf(marker: u8) -> Vec<u8> {
        relocatable(".text", 1, marker)
    }
    fn archive(member: &[u8]) -> Vec<u8> {
        let mut bytes = b"!<arch>\n".to_vec();
        let header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "probe.o/",
            0,
            0,
            0,
            "100644",
            member.len()
        );
        assert_eq!(header.len(), 60);
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(member);
        if !member.len().is_multiple_of(2) {
            bytes.push(b'\n');
        }
        bytes
    }
    #[test]
    fn native_sections_reject_plugin_inputs_in_direct_objects_and_archives() {
        let native = elf(1);
        assert_eq!(elf_needed(&native), Some(Vec::new()));
        assert_eq!(native_archive(&archive(&native)), Some(()));
        for name in [
            ".gnu.lto_.opts",
            ".gnu.lto_.symtab",
            ".gnu.debuglto_.debug_info",
            ".gnu.offload_lto_.opts",
            ".llvmbc",
            ".llvmcmd",
            ".llvm.lto",
            ".gnu_object_only",
            ".gnu.object_only",
            ".plugin",
            ".bitcode",
        ] {
            let object = relocatable(name, 1, 1);
            assert!(elf_needed(&object).is_none(), "direct {name}");
            assert!(
                native_archive(&archive(&object)).is_none(),
                "archive {name}"
            );
        }
        for kind in [0, 6, 0x6000_0000, 0x6fff_4c03, 0x7000_0000] {
            let object = relocatable(".payload", kind, 1);
            assert!(
                elf_needed(&object).is_none(),
                "direct section type {kind:x}"
            );
            assert!(
                native_archive(&archive(&object)).is_none(),
                "archive section type {kind:x}"
            );
        }
        assert!(native_archive(&archive(b"BC\xc0\xde")).is_none());
    }
    #[test]
    fn direct_crt_and_archive_file_inputs_decline_embedded_lto() {
        let directory = Directory::new();
        let crt = directory.0.join("crtbeginS.o");
        let library = directory.0.join("libsample.a");
        let native = elf(1);
        fs::write(&crt, &native).unwrap();
        fs::write(&library, archive(&native)).unwrap();
        let mut inputs = Inputs {
            hash: FingerprintBuilder::new(),
            seen: BTreeSet::new(),
            searches: Vec::new(),
            needed: BTreeSet::new(),
            tool_identities: None,
            content_bytes_hashed: 0,
        };
        assert!(inputs.file(&crt, true).is_some());
        assert!(inputs.file(&library, true).is_some());
        let lto = relocatable(".gnu.lto_.opts", 1, 1);
        fs::write(&crt, &lto).unwrap();
        fs::write(&library, archive(&lto)).unwrap();
        for path in [&crt, &library] {
            let mut inputs = Inputs {
                hash: FingerprintBuilder::new(),
                seen: BTreeSet::new(),
                searches: Vec::new(),
                needed: BTreeSet::new(),
                tool_identities: None,
                content_bytes_hashed: 0,
            };
            assert!(inputs.file(path, true).is_none());
        }
    }
    #[test]
    fn missing_truncated_or_malformed_section_names_cannot_admit_a_link_product() {
        let native = elf(1);
        let mut missing_names = native.clone();
        missing_names[62..64].copy_from_slice(&0u16.to_le_bytes());
        let mut invalid_name = native.clone();
        invalid_name[192..196].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut invalid_range = native.clone();
        invalid_range[192 + 24..192 + 32].copy_from_slice(&u64::MAX.to_le_bytes());
        let mut truncated = native.clone();
        truncated.truncate(200);
        let mut unknown_flags = native.clone();
        unknown_flags[192 + 8..192 + 16].copy_from_slice(&0x100u64.to_le_bytes());
        for object in [
            missing_names,
            invalid_name,
            invalid_range,
            truncated,
            unknown_flags,
        ] {
            assert!(elf_needed(&object).is_none());
            assert!(native_archive(&archive(&object)).is_none());
        }
    }
    fn build_id_elf() -> Vec<u8> {
        let mut bytes = vec![0; 64 + 56 + 36];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        bytes[64..68].copy_from_slice(&4u32.to_le_bytes());
        bytes[72..80].copy_from_slice(&120u64.to_le_bytes());
        bytes[96..104].copy_from_slice(&36u64.to_le_bytes());
        bytes[120..124].copy_from_slice(&4u32.to_le_bytes());
        bytes[124..128].copy_from_slice(&20u32.to_le_bytes());
        bytes[128..132].copy_from_slice(&3u32.to_le_bytes());
        bytes[132..136].copy_from_slice(b"GNU\0");
        bytes[136..156].fill(1);
        bytes
    }
    #[test]
    fn tool_role_dedup_cannot_admit_an_unsupported_link_input() {
        let directory = Directory::new();
        let path = directory.0.join("tool-and-library");
        fs::write(&path, b"unsupported library script").unwrap();
        let mut tools = crate::object_cache::InstalledToolIdentities::default();
        let mut input = Inputs {
            hash: FingerprintBuilder::new(),
            seen: BTreeSet::new(),
            searches: Vec::new(),
            needed: BTreeSet::new(),
            tool_identities: Some(&mut tools),
            content_bytes_hashed: 0,
        };
        assert!(input.file(&path, false).is_some());
        assert!(input.file(&path, true).is_none());
        assert_eq!(
            input.content_bytes_hashed,
            b"unsupported library script".len()
        );
    }
    #[test]
    fn tool_build_id_memo_never_absorbs_link_library_full_content_hashing() {
        let directory = Directory::new();
        let path = directory.0.join("tool-and-library");
        let bytes = build_id_elf();
        fs::write(&path, &bytes).unwrap();
        let mut tools = crate::object_cache::InstalledToolIdentities::default();
        for _ in 0..2 {
            let mut input = Inputs {
                hash: FingerprintBuilder::new(),
                seen: BTreeSet::new(),
                searches: Vec::new(),
                needed: BTreeSet::new(),
                tool_identities: Some(&mut tools),
                content_bytes_hashed: 0,
            };
            assert!(input.file(&path, false).is_some());
            assert_eq!(input.content_bytes_hashed, 0);
            assert!(input.file(&path, true).is_some());
            assert_eq!(input.content_bytes_hashed, bytes.len());
        }
        assert_eq!(tools.summary.full_identity_executions, 1);
        assert_eq!(tools.summary.metadata_reuses, 1);
        assert_eq!(tools.summary.hashed_fallback_bytes, 0);
    }
    fn discover(first: &Path, second: &Path) -> Fingerprint {
        let mut inputs = Inputs {
            hash: FingerprintBuilder::new(),
            seen: BTreeSet::new(),
            searches: Vec::new(),
            needed: BTreeSet::new(),
            tool_identities: None,
            content_bytes_hashed: 0,
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
            tool_identities: None,
            content_bytes_hashed: 0,
        };
        assert!(inputs.file(&script, true).is_none());
    }
}

#[cfg(test)]
mod paired_dependency_tests {
    use super::*;
    const LEFT: &str = "/toolchain/clang";
    const RIGHT: &str = "/usr/bin/ld";
    const BODY_A: &str = "\tlinux-vdso.so.1 (0x1000)\n\tlibc.so.6 => /lib/libc.so.6 (0x2000)\n\t/lib64/ld-linux-x86-64.so.2 (0x3000)\n";
    const BODY_B: &str =
        "\tlibbfd.so => /lib/libbfd.so (0x4000)\n\t/lib64/ld-linux-x86-64.so.2 (0x5000)\n";

    #[test]
    fn paired_ldd_sections_preserve_all_paths_and_duplicate_input_order() {
        let pair = [Path::new(LEFT), Path::new(RIGHT)];
        let text = format!("{LEFT}:\n{BODY_A}\n{RIGHT}:\n{BODY_B}");
        let expected = [
            dependency_rows(BODY_A).unwrap(),
            dependency_rows(BODY_B).unwrap(),
        ];
        assert_eq!(dependency_pair_rows(&text, pair), Some(expected.clone()));
        // Repeated exact paths are two observations, not a map entry overwrite.
        let duplicates = format!("{LEFT}:\n{BODY_A}{LEFT}:\n{BODY_B}");
        assert_eq!(
            dependency_pair_rows(&duplicates, [Path::new(LEFT), Path::new(LEFT)]),
            Some(expected)
        );
        for text in [
            format!("{BODY_A}{RIGHT}:\n{BODY_B}"), // missing first header
            format!("{RIGHT}:\n{BODY_B}{LEFT}:\n{BODY_A}"), // wrong order
            format!("{LEFT}:\n{RIGHT}:\n{BODY_B}"), // empty first section
            format!("{LEFT}:\n{BODY_A}{RIGHT}:\n"), // empty final section
            format!("{LEFT}:\n{BODY_A}/wrong/path:\n{BODY_B}"),
            format!("{LEFT}:\n{BODY_A}  {RIGHT}:\n{BODY_B}"),
            format!("{LEFT}:\n{BODY_A}{RIGHT}:\n{BODY_B}{RIGHT}:\n{BODY_B}"),
            format!("{LEFT}:\n{BODY_A}{RIGHT}:\n\tlibmissing.so => not found\n"),
            format!("{LEFT}:\n{BODY_A}{RIGHT}:\n\tstatically linked\n"),
        ] {
            assert!(dependency_pair_rows(&text, pair).is_none(), "{text}");
        }
        assert!(
            dependency_pair_rows(
                &format!("{LEFT}:\n{BODY_A}{RIGHT}:\n{BODY_B}"),
                [Path::new("/tool\nchain/clang"), Path::new(RIGHT)]
            )
            .is_none()
        );
    }

    #[cfg(unix)]
    fn output(success: bool, text: impl AsRef<[u8]>) -> std::process::Output {
        use std::os::unix::process::ExitStatusExt;
        std::process::Output {
            status: std::process::ExitStatus::from_raw(if success { 0 } else { 256 }),
            stdout: text.as_ref().to_vec(),
            stderr: Vec::new(),
        }
    }
    #[test]
    #[cfg(unix)]
    fn paired_probe_uses_one_process_or_the_two_complete_original_probes() {
        let pair = [Path::new(LEFT), Path::new(RIGHT)];
        let expected = [
            dependency_rows(BODY_A).unwrap(),
            dependency_rows(BODY_B).unwrap(),
        ];
        let mut calls = Vec::new();
        let paths = probe_dependency_pair(pair, |arguments| {
            calls.push(
                arguments
                    .iter()
                    .map(|p| p.to_path_buf())
                    .collect::<Vec<_>>(),
            );
            Some(output(true, format!("{LEFT}:\n{BODY_A}{RIGHT}:\n{BODY_B}")))
        });
        assert_eq!(paths, Some(expected.clone()));
        assert_eq!(calls, [vec![PathBuf::from(LEFT), PathBuf::from(RIGHT)]]);
        // An unfamiliar combined format, failed status, invalid UTF-8 or absent
        // result must not leak partial paths into the authoritative file walk.
        for combined in [
            Some(output(
                true,
                format!("{LEFT}:\n{BODY_A}not-a-header\n{BODY_B}"),
            )),
            Some(output(
                false,
                format!("{LEFT}:\n{BODY_A}{RIGHT}:\n{BODY_B}"),
            )),
            Some(output(true, vec![0xff])),
            None,
        ] {
            let mut combined = combined;
            let mut calls = Vec::new();
            let paths = probe_dependency_pair(pair, |arguments| {
                calls.push(
                    arguments
                        .iter()
                        .map(|p| p.to_path_buf())
                        .collect::<Vec<_>>(),
                );
                if arguments.len() == 2 {
                    combined.take()
                } else if arguments[0] == pair[0] {
                    Some(output(true, BODY_A))
                } else {
                    Some(output(true, BODY_B))
                }
            });
            assert_eq!(paths, Some(expected.clone()));
            assert_eq!(
                calls,
                [
                    vec![pair[0].to_path_buf(), pair[1].to_path_buf()],
                    vec![pair[0].to_path_buf()],
                    vec![pair[1].to_path_buf()]
                ]
            );
        }
        let mut calls = 0;
        assert!(
            probe_dependency_pair(pair, |_| {
                calls += 1;
                Some(output(false, "failed"))
            })
            .is_none()
        );
        assert_eq!(calls, 2); // failed first single probe disables link reuse
    }
}
