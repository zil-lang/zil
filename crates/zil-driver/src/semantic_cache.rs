//! Opt-in, explicitly trusted local frontend candidates. Integrity hashes do
//! not authenticate writers. Restored Analyze claims still pass current Verify.
use super::*;
use std::sync::OnceLock;
use zil_db::{ArtifactCache, CacheRead, QueryKey};
use zil_ir::{
    CandidatePlan, FunctionId, PassingMode, ScalarFunction, ScalarInterface, TypedModule,
    TypedProgram,
};

const MAGIC: &[u8; 8] = b"ZILSEM02";
static COMPILER: OnceLock<Option<Fingerprint>> = OnceLock::new();

pub(super) struct Store {
    root: PathBuf,
    key: Fingerprint,
    maximum: usize,
}

impl Store {
    pub(super) fn new(options: &CompileOptions, identity: &QueryKey) -> Option<Self> {
        if options.stop_after < Stage::Verify
            || options.emit_dir.is_some()
            || options.cache_dir.is_some()
        {
            return None;
        }
        if ["LD_PRELOAD", "LD_AUDIT"]
            .into_iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        {
            return None;
        }
        let root = options.semantic_cache.as_ref()?.join("semantic-v2");
        let compiler = COMPILER
            .get_or_init(|| zil_backend_llvm::installed_compiler_identity().ok())
            .as_ref()?;
        let mut bytes = b"ZIL-TRUSTED-SEMANTIC-v2\0".to_vec();
        bytes.extend_from_slice(&identity.fingerprint().0);
        bytes.extend_from_slice(&compiler.0);
        Some(Self {
            root,
            key: Fingerprint::of(&bytes),
            maximum: options.limits.max_source_bytes,
        })
    }

    pub(super) fn path(&self) -> PathBuf {
        self.root.join(format!("{}.candidate", self.key))
    }

    pub(super) fn bind_path(&mut self, path: &Path) {
        self.root = path
            .parent()
            .expect("planned file has a parent")
            .to_path_buf();
    }

    pub(super) fn load(&self, options: &CompileOptions) -> Option<(TypedProgram, CandidatePlan)> {
        if !private_directory(&self.root, false) {
            return None;
        }
        match ArtifactCache::with_limit(&self.root, self.maximum)
            .inspect_candidate(self.key)
            .ok()?
        {
            CacheRead::Candidate(bytes) => decode(&bytes, &options.limits),
            _ => None,
        }
    }

    /// Optional cache publication never turns an already completed build into
    /// a failure. No filesystem write happens for failed/cancelled requests.
    pub(super) fn save(
        &self,
        typed: &TypedProgram,
        candidate: &CandidatePlan,
        options: &CompileOptions,
        plan: &artifacts::ArtifactPlan,
    ) {
        if options.cancellation.is_cancelled() || plan.validate().is_err() {
            return;
        }
        let Some(bytes) = encode(typed, candidate, &options.limits) else {
            return;
        };
        if options.cancellation.is_cancelled()
            || plan.validate().is_err()
            || !private_directory(&self.root, true)
        {
            return;
        }
        let _ = ArtifactCache::with_limit(&self.root, self.maximum).write_candidate_cancellable(
            self.key,
            &bytes,
            &options.cancellation,
        );
    }
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

struct Writer {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Writer {
    fn put(&mut self, bytes: &[u8]) -> Option<()> {
        if self.bytes.len().checked_add(bytes.len())? > self.maximum {
            return None;
        }
        self.bytes.extend_from_slice(bytes);
        Some(())
    }
    fn number(&mut self, n: usize) -> Option<()> {
        self.put(&u32::try_from(n).ok()?.to_le_bytes())
    }
    fn text(&mut self, s: &str) -> Option<()> {
        self.number(s.len())?;
        self.put(s.as_bytes())
    }
    fn signature(&mut self, s: &ScalarFunction, module: &TypedModule) -> Option<()> {
        let f = module.functions.get(s.function.0 as usize)?;
        if f.id != s.function
            || f.parameters
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != f.parameters.len()
            || f.return_type != s.result
            || f.parameters.len() != s.parameters.len()
            || f.result_mode != s.result_mode
            || f.parameters
                .iter()
                .zip(&s.parameters)
                .any(|(local, (_, ty))| {
                    f.locals
                        .get(local.0 as usize)
                        .is_none_or(|decl| &decl.ty != ty)
                })
        {
            return None;
        }
        self.number(s.function.0 as usize)?;
        self.text(&s.name)?;
        self.put(&[mode(s.result_mode)])?;
        self.number(s.parameters.len())?;
        for (passing, _) in &s.parameters {
            self.put(&[mode(*passing)])?;
        }
        Some(())
    }
}

fn encode(program: &TypedProgram, candidate: &CandidatePlan, limits: &Limits) -> Option<Vec<u8>> {
    metadata_budget(program, limits)?;
    let wire = zil_ir::wire::encode_module_candidate(&program.module, candidate, limits).ok()?;
    let mut out = Writer {
        bytes: Vec::new(),
        maximum: limits.max_source_bytes,
    };
    out.put(MAGIC)?;
    out.number(wire.len())?;
    out.put(&wire)?;
    out.number(program.interfaces.len())?;
    for interface in &program.interfaces {
        if interface.callable_returns.is_some()
            || interface.callable_contracts.is_some()
            || interface.contract_fingerprint.is_some()
            || interface.result_origins.is_some()
        {
            return None;
        }
        out.number(interface.module.0 as usize)?;
        out.put(&interface.signature_fingerprint.0)?;
        out.put(&interface.implementation_fingerprint.0)?;
        out.number(interface.exports.len())?;
        for export in &interface.exports {
            out.signature(export, &program.module)?;
        }
    }
    out.number(program.root_functions.len())?;
    for function in &program.root_functions {
        out.signature(function, &program.module)?;
    }
    Some(out.bytes)
}

struct Reader<'a> {
    bytes: &'a [u8],
    nodes: usize,
    signatures: BTreeMap<FunctionId, usize>,
    type_bytes: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.bytes.len() {
            return None;
        }
        let (value, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Some(value)
    }
    fn number(&mut self) -> Option<usize> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize)
    }
    fn count(&mut self, maximum: usize, minimum_bytes: usize) -> Option<usize> {
        let n = self.number()?;
        if n > maximum || n.checked_mul(minimum_bytes)? > self.bytes.len() {
            return None;
        }
        self.nodes = self.nodes.checked_sub(n)?;
        Some(n)
    }
    fn text(&mut self) -> Option<String> {
        let n = self.number()?;
        Some(std::str::from_utf8(self.take(n)?).ok()?.to_owned())
    }
    fn fingerprint(&mut self) -> Option<Fingerprint> {
        Some(Fingerprint(self.take(32)?.try_into().ok()?))
    }
    fn signature(&mut self, module: &TypedModule, limits: &Limits) -> Option<ScalarFunction> {
        let function = FunctionId(u32::try_from(self.number()?).ok()?);
        let f = module.functions.get(function.0 as usize)?;
        if f.id != function {
            return None;
        }
        // An ordinary function can appear once as a module export and once as
        // a root entry. Bound reconstructed type cloning by twice the decoded
        // function tables, even for a forged repeated metadata record.
        let uses = self.signatures.entry(function).or_default();
        *uses = uses.checked_add(1)?;
        if *uses > 2
            || f.parameters
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != f.parameters.len()
        {
            return None;
        }
        let name = self.text()?;
        let result_mode = read_mode(self.take(1)?[0])?;
        if result_mode != f.result_mode {
            return None;
        }
        let n = self.count(limits.max_locals, 1)?;
        if n != f.parameters.len() {
            return None;
        }
        let mut parameters = Vec::with_capacity(n);
        for local in &f.parameters {
            let ty = &f.locals.get(local.0 as usize)?.ty;
            charge_type(
                ty,
                &mut self.nodes,
                &mut self.type_bytes,
                limits.max_call_depth,
                0,
            )?;
            parameters.push((read_mode(self.take(1)?[0])?, ty.clone()));
        }
        charge_type(
            &f.return_type,
            &mut self.nodes,
            &mut self.type_bytes,
            limits.max_call_depth,
            0,
        )?;
        Some(ScalarFunction {
            name,
            function,
            parameters,
            result: f.return_type.clone(),
            result_mode,
        })
    }
}

fn decode(bytes: &[u8], limits: &Limits) -> Option<(TypedProgram, CandidatePlan)> {
    if bytes.len() > limits.max_source_bytes {
        return None;
    }
    let mut input = Reader {
        bytes,
        nodes: limits.max_nodes,
        signatures: BTreeMap::new(),
        type_bytes: limits.max_source_bytes,
    };
    if input.take(8)? != MAGIC {
        return None;
    }
    let size = input.number()?;
    let (module, candidate) =
        zil_ir::wire::decode_module_candidate(input.take(size)?, limits).ok()?;
    let n = input.count(limits.max_nodes, 72)?;
    let mut interfaces = Vec::with_capacity(n);
    for _ in 0..n {
        let module_id = ModuleId(u32::try_from(input.number()?).ok()?);
        let signature_fingerprint = input.fingerprint()?;
        let implementation_fingerprint = input.fingerprint()?;
        let count = input.count(limits.max_functions, 13)?;
        let mut exports = Vec::with_capacity(count);
        for _ in 0..count {
            exports.push(input.signature(&module, limits)?);
        }
        interfaces.push(ScalarInterface {
            module: module_id,
            exports,
            signature_fingerprint,
            implementation_fingerprint,
            callable_returns: None,
            callable_contracts: None,
            result_origins: None,
            contract_fingerprint: None,
        });
    }
    let n = input.count(limits.max_functions, 13)?;
    let mut root_functions = Vec::with_capacity(n);
    for _ in 0..n {
        root_functions.push(input.signature(&module, limits)?);
    }
    if !input.bytes.is_empty() {
        return None;
    }
    Some((
        TypedProgram {
            module,
            interfaces,
            root_functions,
        },
        candidate,
    ))
}

fn mode(mode: PassingMode) -> u8 {
    match mode {
        PassingMode::Copy => 0,
        PassingMode::Move => 1,
        PassingMode::Describe => 2,
        PassingMode::Create => 3,
        PassingMode::Give => 4,
        PassingMode::Borrow => 5,
        PassingMode::BorrowMut => 6,
        PassingMode::Infer => 7,
    }
}
fn read_mode(n: u8) -> Option<PassingMode> {
    Some(match n {
        0 => PassingMode::Copy,
        1 => PassingMode::Move,
        2 => PassingMode::Describe,
        3 => PassingMode::Create,
        4 => PassingMode::Give,
        5 => PassingMode::Borrow,
        6 => PassingMode::BorrowMut,
        7 => PassingMode::Infer,
        _ => return None,
    })
}

// Bound expanded interface copies, not just the compact record that references
// them. A type node includes its resident enum storage and all string payloads.
fn charge_type(
    ty: &zil_ir::Type,
    nodes: &mut usize,
    bytes: &mut usize,
    maximum_depth: usize,
    depth: usize,
) -> Option<()> {
    use zil_ir::Type;
    if depth > maximum_depth {
        return None;
    }
    *nodes = nodes.checked_sub(1)?;
    *bytes = bytes.checked_sub(std::mem::size_of::<Type>())?;
    fn text(bytes: &mut usize, value: &str) -> Option<()> {
        *bytes = bytes.checked_sub(value.len().checked_add(std::mem::size_of::<String>())?)?;
        Some(())
    }
    match ty {
        Type::Named(name) | Type::Unsupported(name) => text(bytes, name)?,
        Type::Tuple(fields) => {
            for field in fields {
                charge_type(field, nodes, bytes, maximum_depth, depth + 1)?;
            }
        }
        Type::Record { name, fields } | Type::Package { name, fields } => {
            text(bytes, name)?;
            for (name, field) in fields {
                text(bytes, name)?;
                charge_type(field, nodes, bytes, maximum_depth, depth + 1)?;
            }
        }
        Type::Enum { name, variants } => {
            text(bytes, name)?;
            for (name, fields) in variants {
                *nodes = nodes.checked_sub(1)?;
                *bytes = bytes.checked_sub(std::mem::size_of::<Vec<Type>>())?;
                text(bytes, name)?;
                for field in fields {
                    charge_type(field, nodes, bytes, maximum_depth, depth + 1)?;
                }
            }
        }
        Type::Array { element, .. }
        | Type::Slice { element }
        | Type::OwnedStore { element }
        | Type::Managed(element)
        | Type::Desc(element)
        | Type::Address { inner: element, .. }
        | Type::Opaque {
            hidden: element, ..
        } => charge_type(element, nodes, bytes, maximum_depth, depth + 1)?,
        Type::ErrorUnion { error, result } => {
            charge_type(error, nodes, bytes, maximum_depth, depth + 1)?;
            charge_type(result, nodes, bytes, maximum_depth, depth + 1)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            for ty in parameters {
                charge_type(ty, nodes, bytes, maximum_depth, depth + 1)?;
            }
            charge_type(result, nodes, bytes, maximum_depth, depth + 1)?;
        }
        Type::Unit
        | Type::Bool
        | Type::I32
        | Type::I64
        | Type::U8
        | Type::U16
        | Type::ByteBuffer
        | Type::ForeignHandle
        | Type::CWideString
        | Type::Closure(_) => {}
    }
    Some(())
}

fn metadata_budget(program: &TypedProgram, limits: &Limits) -> Option<()> {
    let mut nodes = limits.max_nodes.checked_sub(program.interfaces.len())?;
    let mut bytes = limits.max_source_bytes;
    let mut uses = BTreeMap::<FunctionId, usize>::new();
    for signature in program
        .interfaces
        .iter()
        .flat_map(|i| &i.exports)
        .chain(&program.root_functions)
    {
        nodes = nodes.checked_sub(1 + signature.parameters.len())?;
        let count = uses.entry(signature.function).or_default();
        *count = count.checked_add(1)?;
        if *count > 2 {
            return None;
        }
        for (_, ty) in &signature.parameters {
            charge_type(ty, &mut nodes, &mut bytes, limits.max_call_depth, 0)?;
        }
        charge_type(
            &signature.result,
            &mut nodes,
            &mut bytes,
            limits.max_call_depth,
            0,
        )?;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::RawSnapshot;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn raw(path: &Path) -> (TypedProgram, CandidatePlan) {
        let mut session = CompilationSession::default();
        let report = session.compile_path(
            path,
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..Default::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let (key, program) = session
            .raw
            .iter()
            .find_map(|(key, value)| match value {
                RawSnapshot::ProjectTyped(program) => Some((key.clone(), (**program).clone())),
                _ => None,
            })
            .expect("current typed program");
        let snapshot = session.database.read(&key).unwrap().unwrap();
        let candidate = session
            .analysis_candidate(&snapshot)
            .expect("current adopted candidate");
        (program, (*candidate).clone())
    }

    #[test]
    fn private_format_roundtrips_all_raw_program_fields_for_real_projects() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline");
        for name in [
            "app",
            "stream",
            "slices",
            "byte_io",
            "byte_buffer",
            "byte_buffer_owned",
            "resources",
            "resource_closures",
            "error_cleanup",
            "owned_component",
            "package_ui",
            "retained_ui",
            "update_ui",
            "helper_ui",
        ] {
            let (program, candidate) = raw(&root.join(name).join("main.zil"));
            let bytes = encode(&program, &candidate, &Limits::default()).expect(name);
            assert_eq!(
                decode(&bytes, &Limits::default()),
                Some((program.clone(), candidate.clone())),
                "{name}"
            );
            for len in [0, 7, 8, bytes.len() / 2, bytes.len() - 1] {
                assert!(
                    decode(&bytes[..len], &Limits::default()).is_none(),
                    "{name}/{len}"
                );
            }
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(decode(&trailing, &Limits::default()).is_none());
            let mut published = program;
            published.interfaces[0].result_origins = Some(Vec::new());
            assert!(encode(&published, &candidate, &Limits::default()).is_none());
        }
    }

    #[test]
    fn referenced_signature_type_strings_are_bounded_before_clone() {
        let dir = std::env::temp_dir().join(format!(
            "semantic-codec-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.zil");
        fs::write(&path, "pub fn main()->i32=>0").unwrap();
        let (mut program, candidate) = raw(&path);
        let ty = zil_ir::Type::Record {
            name: "x".repeat(800),
            fields: Vec::new(),
        };
        program.module.functions[0].return_type = ty.clone();
        program.root_functions[0].result = ty.clone();
        program.interfaces[0].exports[0].result = ty;
        let bytes = encode(&program, &candidate, &Limits::default()).unwrap();
        let small = Limits {
            max_source_bytes: bytes.len() + 16,
            ..Limits::default()
        };
        assert!(
            decode(&bytes, &small).is_none(),
            "compact references cannot duplicate large names without charge"
        );
        assert!(
            encode(&program, &candidate, &small).is_none(),
            "writer must match reader eligibility"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejected_wire_candidate_rebuilds_current_source_and_keeps_actual_verification_count() {
        let dir = std::env::temp_dir().join(format!(
            "semantic-reject-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.zil");
        fs::write(&path, "pub fn main()->i32=>0").unwrap();
        let options = CompileOptions {
            semantic_cache: Some(dir.join("cache")),
            ..Default::default()
        };
        let mut session = CompilationSession::default();
        let first = session.compile_path(&path, &options);
        assert_eq!(first.status, ResultCategory::Success, "{}", first.to_json());
        let (key, typed) = session
            .raw
            .iter()
            .find_map(|(key, raw)| match raw {
                RawSnapshot::ProjectTyped(program) => Some((key.clone(), (**program).clone())),
                _ => None,
            })
            .unwrap();
        let store = Store::new(&options, &key).unwrap();
        let (_, original_candidate) = store.load(&options).unwrap();
        let mut forged = typed.clone();
        // Analyze's initialization projection does not validate a scalar
        // constant's result type; independent Verify must reject it.
        forged.module.functions[0].values[0] = zil_ir::Type::Bool;
        let mut candidate = original_candidate.clone();
        candidate.module_fingerprint = forged.module.fingerprint();
        let payload = encode(&forged, &candidate, &options.limits).unwrap();
        ArtifactCache::with_limit(&store.root, store.maximum)
            .write_candidate(store.key, &payload)
            .unwrap();
        let mut fresh = CompilationSession::default();
        let restored = fresh.compile_path(&path, &options);
        assert_eq!(
            restored.status,
            ResultCategory::Success,
            "{}",
            restored.to_json()
        );
        assert_eq!(
            restored.verification_runs, 2,
            "both rejected and accepted verification attempts are reported"
        );
        assert_eq!(
            restored
                .query_events
                .iter()
                .filter(|e| e.phase == Phase::Verify)
                .count(),
            2
        );
        assert_eq!(store.load(&options), Some((typed, original_candidate)));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejected_analysis_claim_rebuilds_source_and_repairs_the_persisted_pair() {
        let dir = std::env::temp_dir().join(format!(
            "semantic-claim-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.zil");
        fs::write(&path, "pub fn main()->i32=>0").unwrap();
        let options = CompileOptions {
            semantic_cache: Some(dir.join("cache")),
            ..Default::default()
        };
        let project = ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(
                zil_base::FileId(0),
                ModuleId(0),
                path.to_string_lossy(),
                fs::read_to_string(&path).unwrap(),
            )],
            paths: BTreeMap::new(),
        };
        let mut producer = CompilationSession::default();
        let cold = producer.compile_project(&project, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        let key = producer
            .raw
            .iter()
            .find_map(|(key, raw)| matches!(raw, RawSnapshot::ProjectTyped(_)).then(|| key.clone()))
            .unwrap();
        let store = Store::new(&options, &key).unwrap();
        let (program, candidate) = store.load(&options).unwrap();
        let mut forged = candidate.clone();
        // Keep the current module and both candidate identities intact. Only
        // the claimed initialization set is false: this function has no locals.
        assert!(program.module.functions[0].locals.is_empty());
        forged.functions[0].blocks[0]
            .initialized_out
            .push(zil_ir::LocalId(0));
        let payload = encode(&program, &forged, &options.limits).unwrap();
        assert_eq!(
            decode(&payload, &options.limits),
            Some((program.clone(), forged))
        );
        ArtifactCache::with_limit(&store.root, store.maximum)
            .write_candidate(store.key, &payload)
            .unwrap();
        let mut consumer = CompilationSession::default();
        let mut events = Vec::new();
        let restored = consumer.compile_project_with_observer(&project, &options, &mut |event| {
            events.push(event.clone())
        });
        assert_eq!(
            restored.status,
            ResultCategory::Success,
            "{}",
            restored.to_json()
        );
        assert_eq!(restored.disk_cache, DiskCacheStatus::Rebuilt);
        assert_eq!(restored.verification_runs, 2);
        assert_eq!(restored.query_events, events);
        assert_eq!(consumer.active_query_count(), 0);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.phase == Phase::Analyze)
                .map(|event| event.disposition)
                .collect::<Vec<_>>(),
            [QueryDisposition::Reused, QueryDisposition::Executed]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.phase == Phase::Verify)
                .map(|event| event.disposition)
                .collect::<Vec<_>>(),
            [QueryDisposition::Executed, QueryDisposition::Executed]
        );
        assert_eq!(store.load(&options), Some((program, candidate)));
        fs::remove_dir_all(dir).unwrap();
    }
}
