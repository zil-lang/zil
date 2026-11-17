//! Explicit trusted semantic-cache raw emission data. Never object-cache trust.
//! Current Verify, complete LIR binding, representation, admission and ordered
//! reservation precede consumption. Integrity selects data, never permission.
use super::raw_codec::{self, MAX_BYTES, Reader, Writer};
use super::*;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::{Fingerprint, Limits};

const MAX_RECORDS: usize = 16_384;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pending {
    directory: PathBuf,
    manifest: String,
    bytes: Vec<u8>,
    maximum: usize,
}
impl Pending {
    /// Manifest is replaced last, only after enclosing native publication.
    /// Failures are optional-cache misses and never revoke a successful build.
    pub(crate) fn commit(mut self, root: &Path) {
        let Some(namespace) = self.directory.file_name().map(|name| name.to_os_string()) else {
            return;
        };
        if !private_directory(root, false) {
            return;
        }
        self.directory = root.join(namespace);
        let _ = self.store();
    }
    fn store(self) -> Option<()> {
        if !private_directory(&self.directory, true) {
            return None;
        }
        let previous = read(&self.directory.join(&self.manifest), 44).and_then(|bytes| {
            let mut input = Reader::new(&bytes, 8)?;
            if input.take(8)? != b"ZILRAM01" {
                return None;
            }
            let digest = Fingerprint(input.take(32)?.try_into().ok()?);
            let length = input.word()?;
            input.finish().then_some((digest, length))
        });
        let digest = Fingerprint::of(&self.bytes);
        let payload_name = format!("{}.raw", digest.to_hex());
        if previous == Some((digest, self.bytes.len()))
            && read(&self.directory.join(&payload_name), self.maximum)
                .is_some_and(|current| current == self.bytes)
        {
            return Some(());
        }
        atomic(&self.directory, &payload_name, &self.bytes)?;
        let mut manifest = Writer::new();
        manifest.put(b"ZILRAM01")?;
        manifest.put(&digest.0)?;
        manifest.word(self.bytes.len())?;
        atomic(&self.directory, &self.manifest, &manifest.0)?;
        if let Some((previous, _)) = previous
            && previous != digest
        {
            // Only this protocol's formerly selected immutable payload. A
            // racing reader either already owns its bytes or gets a miss.
            let _ = fs::remove_file(self.directory.join(format!("{}.raw", previous.to_hex())));
        }
        Some(())
    }
}

#[derive(Clone)]
struct RawRecord {
    input: Arc<[u8]>,
    facts: Arc<[u8]>,
    output: Arc<[u8]>,
}
type Inventory = (Vec<RawRecord>, BTreeMap<FunctionId, usize>);
pub(crate) struct Request {
    directory: PathBuf,
    manifest: String,
    context: Vec<u8>,
    limits: Limits,
    old: Vec<RawRecord>,
    aliases: BTreeMap<FunctionId, usize>,
    decoded: BTreeMap<usize, Option<(TypedFunction, FunctionBatch)>>,
    accepted: BTreeMap<FunctionId, (usize, FunctionBatch)>,
    decode_nodes: usize,
    current: Vec<RawRecord>,
    current_aliases: BTreeMap<FunctionId, usize>,
    owners: std::collections::HashMap<scalar_bodies::Key, usize>,
    bytes: usize,
    enabled: bool,
    maximum: usize,
    pub(crate) hits: usize,
    pub(crate) misses: usize,
}
impl Request {
    pub(crate) fn new(root: &Path, target: &TargetProfile) -> Option<Self> {
        if target.triple != "x86_64-unknown-linux-gnu" {
            return None;
        }
        if ["LD_PRELOAD", "LD_AUDIT"]
            .iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        {
            return None;
        }
        let config = zil_llvm_api::Config {
            triple: &target.triple,
            optimization: zil_llvm_api::Optimization::O2,
        };
        let (version, library) = zil_llvm_api::cache_runtime(&config).ok()?;
        let mut identity = Writer::new();
        identity.put(b"trusted-raw-emission-v1")?;
        identity.put(&crate::object_cache::installed_compiler_identity().ok()?.0)?;
        identity.put(&crate::object_cache::installed_identity(&library).ok()?.0)?;
        identity.text(&version)?;
        let directory = root.join(Fingerprint::of(&identity.0).to_hex());
        Some(Self {
            directory,
            manifest: String::new(),
            context: vec![],
            limits: Limits::default(),
            old: vec![],
            aliases: BTreeMap::new(),
            decoded: BTreeMap::new(),
            accepted: BTreeMap::new(),
            decode_nodes: 0,
            current: vec![],
            current_aliases: BTreeMap::new(),
            owners: Default::default(),
            bytes: 0,
            enabled: false,
            maximum: MAX_BYTES,
            hits: 0,
            misses: 0,
        })
    }
    pub(super) fn bind(
        &mut self,
        context: Option<Vec<u8>>,
        limits: &Limits,
        options: incremental::WholeOptions,
        collected: Option<&BTreeSet<FunctionId>>,
    ) {
        let Some(context) = context else {
            return;
        };
        let incremental::WholeOptions {
            emit,
            scalar_policy,
            closed_collection,
            requested_jobs,
        } = options;
        let EmitOptions { entry } = emit;
        self.maximum = MAX_BYTES.min(limits.max_source_bytes);
        let mut out = Writer::bounded(self.maximum);
        if out
            .blob(&context)
            .and_then(|_| out.word(usize::from(closed_collection)))
            .and_then(|_| out.word(requested_jobs))
            .and_then(|_| out.word(usize::from(scalar_policy)))
            .is_none()
        {
            return;
        }
        if (|| {
            match entry {
                EntryPoint::None => out.word(0)?,
                EntryPoint::Main => out.word(1)?,
                EntryPoint::ReturnValue(id) => {
                    out.word(2)?;
                    out.word(id.0 as usize)?;
                }
                EntryPoint::Test(id) => {
                    out.word(3)?;
                    out.word(id.0 as usize)?;
                }
            }
            out.word(usize::from(collected.is_some()))?;
            if let Some(collected) = collected {
                out.word(collected.len())?;
                for id in collected {
                    out.word(id.0 as usize)?;
                }
            }
            Some(())
        })()
        .is_none()
        {
            return;
        }
        self.context = out.0;
        // One bounded successful epoch per installed compiler/DSO namespace.
        // Full current context bytes, rather than this selector, admit reads.
        self.manifest = "current.manifest".into();
        self.limits = limits.clone();
        self.decode_nodes = limits.max_nodes.min(1_000_000);
        let Some(bytes) = self
            .context
            .len()
            .checked_add(20)
            .filter(|bytes| *bytes <= self.maximum)
        else {
            return;
        };
        self.bytes = bytes;
        self.enabled = true;
        if let Some((records, aliases)) = self.load() {
            self.old = records;
            self.aliases = aliases;
        }
    }
    fn load(&self) -> Option<Inventory> {
        if !private_directory(&self.directory, false) {
            return None;
        }
        let manifest = read(&self.directory.join(&self.manifest), 44)?;
        let mut header = Reader::new(&manifest, 8)?;
        if header.take(8)? != b"ZILRAM01" {
            return None;
        }
        let digest = Fingerprint(header.take(32)?.try_into().ok()?);
        let length = header.word()?;
        if !header.finish() || length > self.maximum {
            return None;
        }
        let bytes = read(
            &self.directory.join(format!("{}.raw", digest.to_hex())),
            self.maximum,
        )?;
        if bytes.len() != length || Fingerprint::of(&bytes) != digest {
            return None;
        }
        let mut input = Reader::new(&bytes, self.limits.max_nodes.min(1_000_000))?;
        if input.take(8)? != b"ZILRAE01" || input.blob()? != self.context {
            return None;
        }
        let count = input.count()?;
        if count > self.limits.max_functions.min(MAX_RECORDS) {
            return None;
        }
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            records.push(RawRecord {
                input: input.blob()?.into(),
                facts: input.blob()?.into(),
                output: input.blob()?.into(),
            });
        }
        let count = input.count()?;
        if count > self.limits.max_functions.min(MAX_RECORDS) {
            return None;
        }
        let mut aliases = BTreeMap::new();
        let mut previous = None;
        for _ in 0..count {
            let id = FunctionId(input.word()? as u32);
            let record = input.word()?;
            if previous.is_some_and(|old| old >= id) || record >= records.len() {
                return None;
            }
            previous = Some(id);
            aliases.insert(id, record);
        }
        input.finish().then_some((records, aliases))
    }
    pub(super) fn lookup(
        &mut self,
        function: &TypedFunction,
        summaries: Option<&BTreeMap<FunctionId, Constant>>,
        functions: &BTreeMap<FunctionId, &TypedFunction>,
        target: &TargetProfile,
    ) -> Option<FunctionBatch> {
        if !self.enabled {
            return None;
        }
        let found = self.lookup_inner(function, summaries, functions, target);
        if found.is_some() {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        found
    }
    fn lookup_inner(
        &mut self,
        function: &TypedFunction,
        summaries: Option<&BTreeMap<FunctionId, Constant>>,
        functions: &BTreeMap<FunctionId, &TypedFunction>,
        target: &TargetProfile,
    ) -> Option<FunctionBatch> {
        let &index = self.aliases.get(&function.id)?;
        let record = self.old.get(index)?;
        let current_facts = facts(function, summaries)?;
        if record.facts.as_ref() != current_facts.as_slice() {
            return None;
        }
        if !self.decoded.contains_key(&index) {
            let decoded = (|| {
                let decoded = zil_ir::wire::decode_raw_function_shared_owner(
                    record.input.clone(),
                    &self.limits,
                    self.decode_nodes,
                );
                let (raw, encoding) = match decoded {
                    Ok(decoded) => decoded,
                    Err(_) => {
                        self.decode_nodes = 0;
                        return None;
                    }
                };
                self.decode_nodes = self.decode_nodes.checked_sub(encoding.node_cost())?;
                let output =
                    match raw_codec::decode(&record.output, &self.limits, &mut self.decode_nodes) {
                        Some(output) => output,
                        None => {
                            self.decode_nodes = 0;
                            return None;
                        }
                    };
                let [m::Item::Function(machine)] = output.body.as_slice() else {
                    return None;
                };
                if machine.name != symbol(raw.id)
                    || !scalar_bodies::cacheable(&raw)
                    || !valid_machine(machine, &raw, functions, target, &output.intrinsics)
                {
                    return None;
                }
                Some((raw.as_ref().clone(), output))
            })();
            self.decoded.insert(index, decoded);
        }
        let (raw, output) = self.decoded.get(&index)?.as_ref()?;
        if !incremental::same_body_input(raw, function) {
            return None;
        }
        let output = scalar_bodies::rebind(output, function.id);
        self.accepted.insert(function.id, (index, output.clone()));
        Some(output)
    }
    pub(super) fn stage(
        &mut self,
        function: &TypedFunction,
        output: &FunctionBatch,
        key: scalar_bodies::Key,
        summaries: Option<&BTreeMap<FunctionId, Constant>>,
    ) {
        if !self.enabled
            || self.current_aliases.contains_key(&function.id)
            || self.current_aliases.len() >= self.limits.max_functions.min(MAX_RECORDS)
            || self
                .bytes
                .checked_add(8)
                .is_none_or(|bytes| bytes > self.maximum)
        {
            return;
        }
        if let Some(&index) = self.owners.get(&key) {
            self.current_aliases.insert(function.id, index);
            self.bytes += 8;
            return;
        }
        if self.stage_inner(function, output, key, summaries).is_some() {
            self.bytes += 8;
        }
    }
    fn stage_inner(
        &mut self,
        function: &TypedFunction,
        output: &FunctionBatch,
        key: scalar_bodies::Key,
        summaries: Option<&BTreeMap<FunctionId, Constant>>,
    ) -> Option<()> {
        if self.current.len() >= self.limits.max_functions.min(MAX_RECORDS)
            || self.current_aliases.len() >= self.limits.max_functions.min(MAX_RECORDS)
        {
            return None;
        }
        let [m::Item::Function(machine)] = output.body.as_slice() else {
            return None;
        };
        if machine.name != symbol(function.id) {
            return None;
        }
        let retained = self
            .accepted
            .get(&function.id)
            .filter(|(_, accepted)| *accepted == *output)
            .and_then(|(index, _)| self.old.get(*index))
            .cloned();
        let record = if let Some(record) = retained {
            record
        } else {
            if !scalar_bodies::cacheable(function) {
                return None;
            }
            let input = zil_ir::wire::encode_raw_function(function, &self.limits)
                .ok()?
                .shared_bytes();
            let facts = facts(function, summaries)?;
            let remaining = self
                .maximum
                .checked_sub(self.bytes)?
                .checked_sub(input.len())?
                .checked_sub(facts.len())?
                .checked_sub(20)?;
            let output = raw_codec::encode_bounded(output, remaining)?;
            RawRecord {
                input,
                facts: facts.into(),
                output: output.into(),
            }
        };
        let bytes = self
            .bytes
            .checked_add(record.input.len())?
            .checked_add(record.output.len())?
            .checked_add(record.facts.len())?
            .checked_add(12)?;
        if bytes.checked_add(8)? > self.maximum {
            return None;
        }
        self.bytes = bytes;
        let index = self.current.len();
        self.current.push(record);
        self.owners.insert(key, index);
        self.current_aliases.insert(function.id, index);
        Some(())
    }
    pub(crate) fn pending(self) -> Option<Pending> {
        if !self.enabled || self.current.is_empty() {
            return None;
        }
        let mut out = Writer::bounded(self.maximum);
        out.put(b"ZILRAE01")?;
        out.blob(&self.context)?;
        out.word(self.current.len())?;
        for record in self.current {
            out.blob(&record.input)?;
            out.blob(&record.facts)?;
            out.blob(&record.output)?;
        }
        out.word(self.current_aliases.len())?;
        for (id, record) in self.current_aliases {
            out.word(id.0 as usize)?;
            out.word(record)?;
        }
        Some(Pending {
            directory: self.directory,
            manifest: self.manifest,
            bytes: out.0,
            maximum: self.maximum,
        })
    }
}

fn primitive_signature(source: &TypedFunction) -> Option<m::Signature> {
    Some(m::Signature::new(
        llvm_type(&source.return_type).ok()?,
        source
            .parameters
            .iter()
            .filter_map(|id| source.locals.get(id.0 as usize))
            .filter(|local| local.ty != Type::Unit)
            .map(|local| Some(m::Param::unnamed(llvm_type(&local.ty).ok()?)))
            .collect::<Option<Vec<_>>>()?,
    ))
}

fn valid_machine(
    function: &m::Function,
    source: &TypedFunction,
    functions: &BTreeMap<FunctionId, &TypedFunction>,
    target: &TargetProfile,
    intrinsics: &BTreeSet<(&'static str, u32)>,
) -> bool {
    // Reject malformed raw models as a cache miss before any reservation is
    // charged. These local declarations check structure only; the mandatory
    // COMPLETE current module preflight later binds actual current ABIs.
    if primitive_signature(source).is_none_or(|signature| !signature.same_abi(&function.signature))
    {
        return false;
    }
    let mut permitted = BTreeMap::new();
    permitted.insert("llvm.trap".into(), m::Signature::new(m::Ty::Void, vec![]));
    for (operation, bits) in intrinsics {
        if !matches!(
            *operation,
            "sadd" | "ssub" | "smul" | "uadd" | "usub" | "umul"
        ) {
            return false;
        }
        permitted.insert(
            format!("llvm.{operation}.with.overflow.i{bits}"),
            m::Signature::new(
                m::Ty::Struct(vec![m::Ty::Int(*bits), m::Ty::Int(1)]),
                vec![m::Param::unnamed(m::Ty::Int(*bits)); 2],
            ),
        );
    }
    for block in &source.blocks {
        for instruction in &block.instructions {
            if let InstructionKind::Call { function: id, .. } = &instruction.kind {
                let Some(callee) = functions.get(id) else {
                    return false;
                };
                if callee.foreign.is_some() {
                    let Some(items) = foreign::declaration(callee, target)
                        .ok()
                        .and_then(|code| code.into_items().ok())
                    else {
                        return false;
                    };
                    for item in items {
                        if let m::Item::Function(declaration) = item {
                            permitted.insert(declaration.name, declaration.signature);
                        } else {
                            return false;
                        }
                    }
                } else {
                    let Some(signature) = primitive_signature(callee) else {
                        return false;
                    };
                    permitted.insert(symbol(callee.id), signature);
                }
            }
        }
    }
    let mut declarations = BTreeMap::<String, m::Signature>::new();
    for block in function.blocks.iter().flatten() {
        for instruction in &block.instructions {
            if let m::Op::Call {
                result,
                result_attrs,
                callee,
                args,
                vararg,
            } = &instruction.op
            {
                let m::Val::Global(name) = callee else {
                    return false;
                };
                if vararg.is_some() {
                    return false;
                }
                let signature = m::Signature {
                    result: result.clone(),
                    result_attrs: result_attrs.clone(),
                    params: args
                        .iter()
                        .map(|arg| m::Param {
                            ty: arg.ty.clone(),
                            name: None,
                            attrs: arg.attrs.clone(),
                        })
                        .collect(),
                    variadic: false,
                };
                if permitted
                    .get(name)
                    .is_none_or(|current: &m::Signature| !current.same_abi(&signature))
                {
                    return false;
                }
                if let Some(old) = declarations.insert(name.clone(), signature.clone())
                    && old != signature
                {
                    return false;
                }
            }
        }
    }
    let mut items = vec![m::Item::Function(function.clone())];
    for (name, signature) in declarations {
        if name != function.name {
            items.push(m::Item::Function(m::Function {
                name,
                linkage: m::Linkage::External,
                signature,
                attrs: vec![],
                blocks: None,
            }));
        }
    }
    zil_llvm_api::validate_module(&m::Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: String::new(),
        items: items.into(),
    })
    .is_ok()
}

fn facts(
    function: &TypedFunction,
    summaries: Option<&BTreeMap<FunctionId, Constant>>,
) -> Option<Vec<u8>> {
    let mut out = Writer::new();
    out.word(usize::from(summaries.is_some()))?;
    if let Some(summaries) = summaries {
        // The own summary is an actual input, but its diagnostic outer ID is
        // not a callee identity. Equal sibling bodies can share this fact.
        if summaries.contains_key(&function.id) {
            return None;
        }
        let mut reads = BTreeSet::new();
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let InstructionKind::Call { function, .. } = &instruction.kind {
                    reads.insert(*function);
                }
            }
        }
        out.word(reads.len())?;
        for id in reads {
            out.word(id.0 as usize)?;
            let Some(value) = summaries.get(&id) else {
                out.word(0)?;
                continue;
            };
            match value {
                Constant::Unit => out.word(1)?,
                Constant::Bool(value) => {
                    out.word(2)?;
                    out.word(usize::from(*value))?;
                }
                Constant::I32(value) => {
                    out.word(3)?;
                    out.put(&value.to_le_bytes())?;
                }
                Constant::I64(value) => {
                    out.word(4)?;
                    out.put(&value.to_le_bytes())?;
                }
                Constant::U8(value) => {
                    out.word(5)?;
                    out.put(&value.to_le_bytes())?;
                }
                Constant::U16(value) => {
                    out.word(6)?;
                    out.put(&value.to_le_bytes())?;
                }
                _ => return None,
            }
        }
    }
    Some(out.0)
}

fn private_directory(path: &Path, create: bool) -> bool {
    if create {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        if builder.create(path).is_err() {
            return false;
        }
    }
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return false;
        }
    }
    true
}
fn read(path: &Path, maximum: usize) -> Option<Vec<u8>> {
    let before = fs::symlink_metadata(path).ok()?;
    if !before.is_file() || before.file_type().is_symlink() || before.len() > maximum as u64 {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if before.permissions().mode() & 0o022 != 0 {
            return None;
        }
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0o400000);
    }
    let mut file = options.open(path).ok()?;
    let opened = file.metadata().ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (before.dev(), before.ino()) != (opened.dev(), opened.ino()) {
            return None;
        }
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > maximum || bytes.len() != opened.len() as usize {
        return None;
    }
    Some(bytes)
}
fn atomic(directory: &Path, name: &str, bytes: &[u8]) -> Option<()> {
    if bytes.len() > MAX_BYTES || !private_directory(directory, false) {
        return None;
    }
    let path = directory.join(name);
    if let Ok(meta) = fs::symlink_metadata(&path)
        && (!meta.is_file() || meta.file_type().is_symlink())
    {
        return None;
    }
    let temporary = directory.join(format!(
        ".raw-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file: File = options.open(&temporary).ok()?;
        file.write_all(bytes).ok()?;
        file.sync_all().ok()?;
        if !private_directory(directory, false) {
            return None;
        }
        fs::rename(&temporary, &path).ok()?;
        Some(())
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(test)]
#[path = "raw_products_tests.rs"]
mod tests;
