//! Optional derivation from a fully compiled, trusted scalar object. A snapshot
//! is raw dependency metadata; the caller must validate the complete current
//! model and independently compile the patch before using derived bytes.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::OsString;
use std::fs::{self, File};
use std::hash::Hash;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::{Fingerprint, FingerprintBuilder};
use zil_llvm_api::model::{Arg, Function, Item, Linkage, Module, Op, Ty, Val};

const MAGIC: &[u8; 8] = b"ZILPAT01";
const MAX_RECORD: usize = 2 * 1024 * 1024;
const MAX_OBJECT: usize = 128 * 1024 * 1024;
const MAX_FUNCTIONS: usize = 16_384;
const MAX_PATCH: usize = 16;
const PRIVATE_PREFIX: &str = "__zil_patch_old_";
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Policy {
    pub(crate) identity: Fingerprint,
    objcopy: PathBuf,
    linker: PathBuf,
}
impl Policy {
    pub(crate) fn discover(clang: &Path) -> Option<Self> {
        if !cfg!(target_os = "linux")
            || ["LD_PRELOAD", "LD_AUDIT"]
                .into_iter()
                .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
        {
            return None;
        }
        let clang = if clang.components().count() > 1 || clang.is_absolute() {
            fs::canonicalize(clang).ok()?
        } else {
            std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|directory| directory.join(clang))
                .find(|path| path.is_file())?
                .canonicalize()
                .ok()?
        };
        // Keep the ld.lld invocation name: its sibling may be a symlink to the
        // generic lld driver, which dispatches according to argv[0].
        let linker = clang.parent()?.join("ld.lld");
        let objcopy = fs::canonicalize("/usr/bin/objcopy").ok()?;
        let mut identity = FingerprintBuilder::new();
        "ZIL-FULL-OBJECT-PATCH-1".hash(&mut identity);
        crate::object_cache::installed_identity(&objcopy)
            .ok()?
            .hash(&mut identity);
        crate::object_cache::installed_identity(&linker)
            .ok()?
            .hash(&mut identity);
        Some(Self {
            identity: identity.digest(),
            objcopy,
            linker,
        })
    }
}

struct Work {
    nodes: usize,
    bytes: usize,
}
impl Work {
    fn take(&mut self, count: usize) -> Option<()> {
        self.nodes = self.nodes.checked_sub(count)?;
        Some(())
    }
    fn text(&mut self, text: &str) -> Option<()> {
        self.bytes = self.bytes.checked_sub(text.len())?;
        Some(())
    }
    fn atom(&mut self, value: &Val) -> Option<()> {
        self.take(1)?;
        match value {
            Val::Local(name) => self.text(name),
            Val::Int(_) | Val::Bool(_) | Val::Zero | Val::Undef | Val::Poison => Some(()),
            _ => None,
        }
    }
    fn argument(&mut self, argument: &Arg) -> Option<()> {
        integer(&argument.ty).then_some(())?;
        self.atom(&argument.value)
    }
}
fn integer(ty: &Ty) -> bool {
    matches!(ty, Ty::Int(1 | 8 | 16 | 32 | 64))
}
fn pair(ty: &Ty) -> bool {
    matches!(ty, Ty::Struct(fields) if fields.len() == 2 && fields.iter().all(integer))
}
fn signature(function: &Function, definition: bool) -> bool {
    function.linkage == Linkage::External
        && !function.signature.variadic
        && (integer(&function.signature.result)
            || function.signature.result == Ty::Void
            || (!definition
                && function.name.starts_with("llvm.")
                && pair(&function.signature.result)))
        && function.signature.params.len() <= 64
        && function.signature.result_attrs.len() <= 2
        && function.attrs.len() <= 2
        && function
            .signature
            .params
            .iter()
            .all(|parameter| integer(&parameter.ty) && parameter.attrs.len() <= 2)
}

fn references(
    function: &Function,
    definitions: &BTreeMap<&str, usize>,
    symbols: &BTreeSet<&str>,
    work: &mut Work,
) -> Option<Vec<usize>> {
    let mut references = BTreeSet::new();
    for block in function.blocks.as_ref()? {
        work.take(1)?;
        work.text(&block.name)?;
        for instruction in &block.instructions {
            work.take(1)?;
            if let Some(result) = &instruction.result {
                work.text(result)?;
            }
            match &instruction.op {
                Op::Call {
                    result,
                    result_attrs,
                    callee: Val::Global(callee),
                    args,
                    vararg: None,
                } => {
                    work.text(callee)?;
                    if !symbols.contains(callee.as_str())
                        || result_attrs.len() > 2
                        || args.len() > 64
                        || !(integer(result)
                            || *result == Ty::Void
                            || (callee.starts_with("llvm.") && pair(result)))
                    {
                        return None;
                    }
                    if let Some(&id) = definitions.get(callee.as_str()) {
                        references.insert(id);
                    }
                    for argument in args {
                        work.argument(argument)?;
                    }
                }
                Op::Binary {
                    opcode,
                    ty,
                    left,
                    right,
                } => {
                    integer(ty).then_some(())?;
                    work.text(opcode)?;
                    work.atom(left)?;
                    work.atom(right)?;
                }
                Op::Compare {
                    predicate,
                    ty,
                    left,
                    right,
                } => {
                    integer(ty).then_some(())?;
                    work.text(predicate)?;
                    work.atom(left)?;
                    work.atom(right)?;
                }
                Op::Cast { opcode, arg, to } => {
                    integer(to).then_some(())?;
                    work.text(opcode)?;
                    work.argument(arg)?;
                }
                Op::Extract { arg, index } => {
                    (pair(&arg.ty) && *index < 2).then_some(())?;
                    work.atom(&arg.value)?;
                }
                Op::Freeze(arg) | Op::Return(Some(arg)) => work.argument(arg)?,
                Op::Phi { ty, incoming } => {
                    integer(ty).then_some(())?;
                    for (value, label) in incoming {
                        work.atom(value)?;
                        work.text(label)?;
                    }
                }
                Op::Branch(label) => work.text(label)?,
                Op::CondBranch { condition, yes, no } => {
                    work.atom(condition)?;
                    work.text(yes)?;
                    work.text(no)?;
                }
                Op::Return(None) | Op::Unreachable => {}
                _ => return None,
            }
        }
    }
    Some(references.into_iter().collect())
}

struct FunctionState {
    item: usize,
    name: String,
    body: Fingerprint,
    references: Vec<usize>,
    cost: usize,
}
pub(crate) struct Snapshot<'a> {
    pub(crate) index_key: Fingerprint,
    module: &'a Module,
    functions: Vec<FunctionState>,
    total_cost: usize,
}
impl<'a> Snapshot<'a> {
    pub(crate) fn new(module: &'a Module) -> Option<Self> {
        if module.triple != "x86_64-unknown-linux-gnu" || module.items.len() > MAX_FUNCTIONS + 64 {
            return None;
        }
        let mut work = Work {
            nodes: 1_000_000,
            bytes: MAX_RECORD,
        };
        let mut definitions = BTreeMap::new();
        let mut symbols = BTreeSet::new();
        let mut declarations = 0usize;
        let mut skeleton = FingerprintBuilder::new();
        "ZIL-OBJECT-PATCH-SKELETON-1".hash(&mut skeleton);
        module.triple.hash(&mut skeleton);
        module.items.len().hash(&mut skeleton);
        for item in &module.items {
            work.take(1)?;
            let Item::Function(function) = item else {
                return None;
            };
            work.text(&function.name)?;
            if function.name.starts_with(PRIVATE_PREFIX)
                || (function.blocks.is_none()
                    && (function.name.starts_with("zil.fn.")
                        || function.name.starts_with("zil.import.")))
                || function.name.is_empty()
                || !function
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._$-".contains(&b))
                || !symbols.insert(function.name.as_str())
                || !signature(function, function.blocks.is_some())
            {
                return None;
            }
            for parameter in &function.signature.params {
                work.take(1)?;
                if let Some(name) = &parameter.name {
                    work.text(name)?;
                }
            }
            function.name.hash(&mut skeleton);
            function.linkage.hash(&mut skeleton);
            function.signature.hash(&mut skeleton);
            function.attrs.hash(&mut skeleton);
            function.blocks.is_some().hash(&mut skeleton);
            if function.blocks.is_some() {
                let id = definitions.len();
                definitions.insert(function.name.as_str(), id);
            } else {
                declarations += 1;
            }
        }
        if !(64..=MAX_FUNCTIONS).contains(&definitions.len()) || declarations > 64 {
            return None;
        }
        let mut functions = Vec::with_capacity(definitions.len());
        let mut total_cost = 0usize;
        let mut record_size = 8usize + 32 + 32 + 32 + 4 + 32;
        for (item, value) in module.items.iter().enumerate() {
            let Item::Function(function) = value else {
                return None;
            };
            if function.blocks.is_none() {
                continue;
            }
            let references = references(function, &definitions, &symbols, &mut work)?;
            record_size = record_size.checked_add(36 + references.len().checked_mul(4)?)?;
            if record_size > MAX_RECORD {
                return None;
            }
            let mut hash = FingerprintBuilder::new();
            function.hash(&mut hash);
            let cost = function.cost();
            total_cost = total_cost.checked_add(cost)?;
            functions.push(FunctionState {
                item,
                name: function.name.clone(),
                body: hash.digest(),
                references,
                cost,
            });
        }
        Some(Self {
            index_key: skeleton.digest(),
            module,
            functions,
            total_cost,
        })
    }

    /// The caller publishes this only alongside a successful FULL compilation.
    /// Derived objects must never replace the full-base index with a patch chain.
    pub(crate) fn encode_base(
        &self,
        full_object_key: Fingerprint,
        full_object_digest: Fingerprint,
    ) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&self.index_key.0);
        bytes.extend_from_slice(&full_object_key.0);
        bytes.extend_from_slice(&full_object_digest.0);
        bytes.extend_from_slice(&(self.functions.len() as u32).to_le_bytes());
        for function in &self.functions {
            bytes.extend_from_slice(&function.body.0);
            bytes.extend_from_slice(&(function.references.len() as u32).to_le_bytes());
            for &reference in &function.references {
                bytes.extend_from_slice(&(reference as u32).to_le_bytes());
            }
        }
        bytes.extend_from_slice(&Fingerprint::of(&bytes).0);
        bytes
    }

    pub(crate) fn plan(&self, module: &Module, record: &[u8]) -> Option<Plan> {
        if !std::ptr::eq(module, self.module) || record.len() > MAX_RECORD || record.len() < 140 {
            return None;
        }
        let (payload, digest) = record.split_at(record.len() - 32);
        if Fingerprint::of(payload).0 != digest {
            return None;
        }
        let mut input = Reader(payload);
        if input.take(8)? != MAGIC || input.fingerprint()? != self.index_key {
            return None;
        }
        let base_key = input.fingerprint()?;
        let base_digest = input.fingerprint()?;
        let count = input.number()?;
        if count != self.functions.len() {
            return None;
        }
        let mut reverse = vec![Vec::new(); count];
        let mut changed = BTreeSet::new();
        let mut edges_left = 1_000_000usize;
        for (id, current) in self.functions.iter().enumerate() {
            if input.fingerprint()? != current.body {
                changed.insert(id);
                if changed.len() > MAX_PATCH {
                    return None;
                }
            }
            let n = input.number()?;
            if n > count {
                return None;
            }
            let mut previous = None;
            for _ in 0..n {
                edges_left = edges_left.checked_sub(1)?;
                let reference = input.number()?;
                if reference >= count || previous.is_some_and(|p| p >= reference) {
                    return None;
                }
                previous = Some(reference);
                reverse[reference].push(id);
            }
            for &reference in &current.references {
                edges_left = edges_left.checked_sub(1)?;
                reverse[reference].push(id);
            }
        }
        if !input.0.is_empty() || changed.is_empty() {
            return None;
        }
        let mut pending = changed.iter().copied().collect::<VecDeque<_>>();
        while let Some(id) = pending.pop_front() {
            for &caller in &reverse[id] {
                edges_left = edges_left.checked_sub(1)?;
                if changed.insert(caller) {
                    if changed.len() > MAX_PATCH {
                        return None;
                    }
                    pending.push_back(caller);
                }
            }
        }
        let mut cost = 0usize;
        for &id in &changed {
            cost = cost.checked_add(self.functions[id].cost)?;
            if cost > 4096
                || self.functions[id]
                    .references
                    .iter()
                    .any(|id| !changed.contains(id))
            {
                return None;
            }
        }
        if cost.checked_mul(16)? > self.total_cost {
            return None;
        }
        let items = changed
            .iter()
            .map(|&id| self.functions[id].item)
            .collect::<BTreeSet<_>>();
        let patch = Module {
            triple: module.triple.clone(),
            fingerprint: module.fingerprint.clone(),
            items: module
                .items
                .iter()
                .enumerate()
                .filter_map(|(id, item)| {
                    let Item::Function(function) = item else {
                        return None;
                    };
                    (function.blocks.is_none() || items.contains(&id)).then(|| item.clone())
                })
                .collect(),
        };
        Some(Plan {
            base_key,
            base_digest,
            module: patch,
            replacements: changed
                .iter()
                .map(|&id| self.functions[id].name.clone())
                .collect(),
            reused_functions: count - changed.len(),
            canonical: self
                .functions
                .iter()
                .map(|function| function.name.clone())
                .collect(),
            declarations: module
                .items
                .iter()
                .filter_map(|item| {
                    let Item::Function(function) = item else {
                        return None;
                    };
                    function.blocks.is_none().then(|| function.name.clone())
                })
                .collect(),
        })
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(count)?;
        self.0 = tail;
        Some(head)
    }
    fn number(&mut self) -> Option<usize> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize)
    }
    fn fingerprint(&mut self) -> Option<Fingerprint> {
        Some(Fingerprint(self.take(32)?.try_into().ok()?))
    }
}

pub(crate) struct Plan {
    pub(crate) base_key: Fingerprint,
    pub(crate) base_digest: Fingerprint,
    pub(crate) module: Module,
    pub(crate) replacements: Vec<String>,
    pub(crate) reused_functions: usize,
    canonical: BTreeSet<String>,
    declarations: BTreeSet<String>,
}
pub(crate) struct Attempt {
    pub(crate) bytes: Option<Vec<u8>>,
    pub(crate) commands: Vec<Vec<String>>,
    pub(crate) reason: Option<String>,
}
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn object(bytes: &[u8]) -> bool {
    bytes.len() >= 64
        && bytes.len() <= MAX_OBJECT
        && &bytes[..6] == b"\x7fELF\x02\x01"
        && bytes[16..20] == [1, 0, 62, 0]
}

// Read only the symbol information needed to validate the external tools'
// result. GNU objcopy and ld.lld remain responsible for ELF transformations.
struct Symbols<'a> {
    global: BTreeSet<&'a str>,
    local: BTreeMap<&'a str, (usize, u8)>,
    undefined: BTreeSet<&'a str>,
}
fn invalid_object() -> io::Error {
    io::Error::other("object patch ELF symbols are outside the admitted domain")
}
fn symbols(bytes: &[u8]) -> Option<Symbols<'_>> {
    if !object(bytes) {
        return None;
    }
    let u16_at = |offset: usize| -> Option<usize> {
        Some(
            u16::from_le_bytes(bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?)
                as usize,
        )
    };
    let u32_at = |offset: usize| -> Option<usize> {
        Some(
            u32::from_le_bytes(bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?)
                as usize,
        )
    };
    let u64_at = |offset: usize| -> Option<usize> {
        usize::try_from(u64::from_le_bytes(
            bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
        ))
        .ok()
    };
    if bytes[6] != 1
        || u32_at(20)? != 1
        || u16_at(52)? != 64
        || u16_at(56)? != 0
        || u16_at(58)? != 64
    {
        return None;
    }
    let offset = u64_at(40)?;
    let count = u16_at(60)?;
    if offset < 64 || !(1..=1024).contains(&count) || u16_at(62)? >= count {
        return None;
    }
    bytes.get(offset..offset.checked_add(count.checked_mul(64)?)?)?;
    let mut table = None;
    for id in 0..count {
        let section = offset + id * 64;
        let kind = u32_at(section + 4)?;
        if kind == 2 {
            if table.replace(section).is_some() {
                return None;
            }
        } else if matches!(kind, 11 | 17 | 18) {
            // Dynamic symbols, COMDAT groups and extended symbol indices need
            // additional identity rules; none are produced by this scalar path.
            return None;
        }
        if kind != 8 {
            let start = u64_at(section + 24)?;
            let length = u64_at(section + 32)?;
            bytes.get(start..start.checked_add(length)?)?;
        }
    }
    let table = table?;
    let string_id = u32_at(table + 40)?;
    if string_id >= count || u64_at(table + 56)? != 24 {
        return None;
    }
    let string_section = offset + string_id * 64;
    if u32_at(string_section + 4)? != 3 {
        return None;
    }
    let start = u64_at(string_section + 24)?;
    let length = u64_at(string_section + 32)?;
    if length > MAX_RECORD {
        return None;
    }
    let strings = bytes.get(start..start.checked_add(length)?)?;
    let start = u64_at(table + 24)?;
    let length = u64_at(table + 32)?;
    if length % 24 != 0 || length / 24 > 262_144 {
        return None;
    }
    let locals = u32_at(table + 44)?;
    if locals > length / 24 {
        return None;
    }
    let mut output = Symbols {
        global: BTreeSet::new(),
        local: BTreeMap::new(),
        undefined: BTreeSet::new(),
    };
    let mut name_bytes = MAX_RECORD;
    for id in 0..length / 24 {
        let symbol = start.checked_add(id.checked_mul(24)?)?;
        let name_offset = u32_at(symbol)?;
        let tail = strings.get(name_offset..)?;
        let name_length = tail.iter().take(name_bytes).position(|&byte| byte == 0)?;
        name_bytes = name_bytes.checked_sub(name_length + 1)?;
        let name = std::str::from_utf8(tail.get(..name_length)?).ok()?;
        let info = *bytes.get(symbol + 4)?;
        let binding = info >> 4;
        let kind = info & 15;
        let section = u16_at(symbol + 6)?;
        if binding > 1 || (binding == 0) != (id < locals) {
            return None;
        }
        if section >= count && section != 0xfff1 {
            return None;
        }
        if name.is_empty() {
            if binding != 0 {
                return None;
            }
            continue;
        }
        if section == 0 {
            if binding != 1 || !output.undefined.insert(name) {
                return None;
            }
        } else if binding == 1 {
            if kind != 2 || section == 0xfff1 || !output.global.insert(name) {
                return None;
            }
        } else {
            let entry = output.local.entry(name).or_insert((0, kind));
            entry.0 += 1;
        }
    }
    Some(output)
}
fn check_symbols(
    bytes: &[u8],
    expected: &BTreeSet<&str>,
    canonical: &BTreeSet<String>,
    declarations: &BTreeSet<String>,
    retired: &[String],
) -> io::Result<()> {
    let symbols = symbols(bytes).ok_or_else(invalid_object)?;
    if symbols.global != *expected
        || symbols.local.keys().any(|name| canonical.contains(*name))
        || symbols
            .undefined
            .iter()
            .any(|name| !declarations.contains(*name))
        || retired
            .iter()
            .any(|name| symbols.local.get(name.as_str()) != Some(&(1, 2)))
        || symbols
            .global
            .iter()
            .chain(symbols.local.keys())
            .chain(&symbols.undefined)
            .any(|name| {
                name.starts_with(PRIVATE_PREFIX) && !retired.iter().any(|wanted| wanted == *name)
            })
    {
        return Err(invalid_object());
    }
    Ok(())
}
fn read_object(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_OBJECT as u64 {
        return Err(io::Error::other("derived object exceeds byte bound"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_OBJECT as u64 + 1).read_to_end(&mut bytes)?;
    if !object(&bytes) {
        return Err(invalid_object());
    }
    Ok(bytes)
}
fn run(
    program: &Path,
    args: &[OsString],
    staging: &Path,
    commands: &mut Vec<Vec<String>>,
) -> io::Result<()> {
    commands.push(
        std::iter::once(program.as_os_str())
            .chain(args.iter().map(|s| s.as_os_str()))
            .map(|s| s.to_string_lossy().into_owned())
            .collect(),
    );
    let error_path = staging.join("tool.stderr");
    let status = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(File::create(&error_path)?)
        .status()?;
    if status.success() {
        return Ok(());
    }
    let mut message = Vec::new();
    File::open(error_path)?
        .take(4096)
        .read_to_end(&mut message)?;
    Err(io::Error::other(format!(
        "{} exited {status}: {}",
        program.display(),
        String::from_utf8_lossy(&message)
    )))
}
impl Plan {
    pub(crate) fn derive(
        &self,
        policy: &Policy,
        basebytes: &[u8],
        patchbytes: &[u8],
        output_path: &Path,
        current_key: Fingerprint,
    ) -> Attempt {
        let mut commands = Vec::new();
        let result = (|| -> io::Result<Vec<u8>> {
            if basebytes.len() > MAX_OBJECT || Fingerprint::of(basebytes) != self.base_digest {
                return Err(io::Error::other("object patch full-base digest mismatch"));
            }
            if !object(basebytes)
                || !object(patchbytes)
                || self.replacements.is_empty()
                || self.replacements.len() > MAX_PATCH
            {
                return Err(io::Error::other(
                    "object patch inputs are outside the admitted bounds",
                ));
            }
            let canonical = self
                .canonical
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            let replacements = self
                .replacements
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            check_symbols(
                basebytes,
                &canonical,
                &self.canonical,
                &self.declarations,
                &[],
            )?;
            check_symbols(
                patchbytes,
                &replacements,
                &self.canonical,
                &self.declarations,
                &[],
            )?;
            let parent = output_path
                .parent()
                .ok_or_else(|| io::Error::other("object output has no parent"))?;
            let directory = parent.join(format!(
                ".zil-object-patch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&directory)?;
            let staging = Staging(directory);
            let base = staging.0.join("base.o");
            let patch = staging.0.join("patch.o");
            let renamed = staging.0.join("retired.o");
            let combined = staging.0.join("combined.o");
            fs::write(&base, basebytes)?;
            fs::write(&patch, patchbytes)?;
            let mut args = Vec::<OsString>::new();
            let mut retired = Vec::with_capacity(self.replacements.len());
            for (id, name) in self.replacements.iter().enumerate() {
                let private = format!("{PRIVATE_PREFIX}{}_{}_{id}", self.base_key, current_key);
                args.push(format!("--redefine-sym={name}={private}").into());
                args.push(format!("--localize-symbol={private}").into());
                retired.push(private);
            }
            args.push(base.into_os_string());
            args.push(renamed.as_os_str().to_owned());
            run(&policy.objcopy, &args, &staging.0, &mut commands)?;
            let unchanged = canonical.difference(&replacements).copied().collect();
            check_symbols(
                &read_object(&renamed)?,
                &unchanged,
                &self.canonical,
                &self.declarations,
                &retired,
            )?;
            let args = vec![
                OsString::from("-r"),
                OsString::from("--build-id=none"),
                renamed.into_os_string(),
                patch.into_os_string(),
                OsString::from("-o"),
                combined.as_os_str().to_owned(),
            ];
            run(&policy.linker, &args, &staging.0, &mut commands)?;
            let bytes = read_object(&combined)?;
            check_symbols(
                &bytes,
                &canonical,
                &self.canonical,
                &self.declarations,
                &retired,
            )?;
            fs::write(output_path, &bytes)?;
            Ok(bytes)
        })();
        match result {
            Ok(bytes) => Attempt {
                bytes: Some(bytes),
                commands,
                reason: None,
            },
            Err(error) => Attempt {
                bytes: None,
                commands,
                reason: Some(error.to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Block, Inst, Signature};
    fn function(name: &str, call: Option<&str>, value: i128) -> Item {
        let instructions = if let Some(callee) = call {
            vec![
                Inst::new(
                    Some("v".into()),
                    Op::Call {
                        result: Ty::Int(32),
                        result_attrs: vec![],
                        callee: Val::Global(callee.into()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                Inst::new(
                    None,
                    Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("v".into())))),
                ),
            ]
        } else {
            vec![Inst::new(
                None,
                Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(value)))),
            )]
        };
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
            attrs: vec![],
            blocks: Some(vec![Block {
                name: "entry".into(),
                instructions,
            }]),
        })
    }
    fn sample() -> Module {
        let mut items = (0..128)
            .map(|id| function(&format!("f{id}"), None, 1))
            .collect::<Vec<_>>();
        items[1] = function("f1", Some("f0"), 0);
        items.push(function("main", Some("f1"), 0));
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "current".into(),
            items,
        }
    }
    #[test]
    fn plan_rebuilds_reverse_closure_and_rejects_outgoing_body_dependencies() {
        let old = sample();
        let key = Fingerprint::of(b"full object");
        let record = Snapshot::new(&old)
            .unwrap()
            .encode_base(key, Fingerprint::ZERO);
        let mut current = old.clone();
        current.items[0] = function("f0", None, 2);
        let snapshot = Snapshot::new(&current).unwrap();
        let plan = snapshot.plan(&current, &record).unwrap();
        assert_eq!(plan.base_key, key);
        assert_eq!(plan.replacements, ["f0", "f1", "main"]);
        assert_eq!(plan.reused_functions, 126);
        zil_llvm_api::validate_module(&plan.module).unwrap();
        assert!(snapshot.plan(&old, &record).is_none());
        current.items[0] = function("f0", Some("f2"), 0);
        assert!(
            Snapshot::new(&current)
                .unwrap()
                .plan(&current, &record)
                .is_none()
        );
        assert!(Snapshot::new(&old).unwrap().plan(&old, &record).is_none());
    }
    #[test]
    fn records_are_bounded_and_bind_declarations_order_and_signature() {
        let old = sample();
        let record = Snapshot::new(&old)
            .unwrap()
            .encode_base(Fingerprint::ZERO, Fingerprint::ZERO);
        let mut current = old.clone();
        current.items[0] = function("f0", None, 3);
        let snapshot = Snapshot::new(&current).unwrap();
        for length in [0, 8, 76, record.len() - 1] {
            assert!(snapshot.plan(&current, &record[..length]).is_none());
        }
        let mut corrupt = record.clone();
        corrupt[100] ^= 1;
        assert!(snapshot.plan(&current, &corrupt).is_none());
        assert!(snapshot.plan(&current, &vec![0; MAX_RECORD + 1]).is_none());
        current.items.swap(2, 3);
        assert!(
            Snapshot::new(&current)
                .unwrap()
                .plan(&current, &record)
                .is_none()
        );
        current.items.swap(2, 3);
        let Item::Function(f) = &mut current.items[2] else {
            unreachable!()
        };
        f.signature.result = Ty::Int(64);
        assert!(
            Snapshot::new(&current)
                .unwrap()
                .plan(&current, &record)
                .is_none()
        );
        let mut declaration = function("external", None, 0);
        let Item::Function(f) = &mut declaration else {
            unreachable!()
        };
        f.blocks = None;
        current = old;
        current.items.push(declaration);
        assert!(
            Snapshot::new(&current)
                .unwrap()
                .plan(&current, &record)
                .is_none()
        );
    }
    #[test]
    fn addresses_indirect_calls_and_large_reverse_closures_decline() {
        let mut current = sample();
        let Item::Function(f) = &mut current.items[0] else {
            unreachable!()
        };
        f.blocks.as_mut().unwrap()[0].instructions[0].op =
            Op::Return(Some(Arg::new(Ty::Int(32), Val::Global("f2".into()))));
        assert!(Snapshot::new(&current).is_none());
        current = sample();
        let Item::Function(f) = &mut current.items[1] else {
            unreachable!()
        };
        let Op::Call { callee, .. } = &mut f.blocks.as_mut().unwrap()[0].instructions[0].op else {
            unreachable!()
        };
        *callee = Val::Local("indirect".into());
        assert!(Snapshot::new(&current).is_none());
        let mut old = sample();
        for id in 1..17 {
            old.items[id] = function(&format!("f{id}"), Some("f0"), 0);
        }
        let record = Snapshot::new(&old)
            .unwrap()
            .encode_base(Fingerprint::ZERO, Fingerprint::ZERO);
        current = old;
        current.items[0] = function("f0", None, 2);
        assert!(
            Snapshot::new(&current)
                .unwrap()
                .plan(&current, &record)
                .is_none()
        );
    }

    #[test]
    fn source_declarations_and_replaced_full_bytes_decline_before_tools() {
        for name in ["zil.fn.123", "zil.import.foreign"] {
            let mut current = sample();
            let mut declaration = function(name, None, 0);
            let Item::Function(function) = &mut declaration else {
                unreachable!()
            };
            function.blocks = None;
            current.items.push(declaration);
            assert!(Snapshot::new(&current).is_none());
        }
        let old = sample();
        let digest = Fingerprint::of(b"original full object bytes");
        let record = Snapshot::new(&old)
            .unwrap()
            .encode_base(Fingerprint::ZERO, digest);
        let mut current = old.clone();
        current.items[0] = function("f0", None, 2);
        let plan = Snapshot::new(&current)
            .unwrap()
            .plan(&current, &record)
            .unwrap();
        assert_eq!(plan.base_digest, digest);
        let output = std::env::temp_dir().join(format!(
            "zil-object-patch-replaced-{}-{}.o",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(!output.exists());
        let policy = Policy {
            identity: Fingerprint::ZERO,
            objcopy: "/tool-must-not-run".into(),
            linker: "/tool-must-not-run".into(),
        };
        let result = plan.derive(
            &policy,
            b"derived bytes under the same key",
            &[],
            &output,
            Fingerprint::ZERO,
        );
        assert!(result.bytes.is_none());
        assert!(result.commands.is_empty());
        assert!(result.reason.unwrap().contains("full-base digest mismatch"));
        assert!(!output.exists());
    }
}
