//! Explicit trusted-local memoization, never a serialized verification permit.
//! Installed compiler and private cache producers are trusted, as for the object
//! cache. Digests detect damage; they do not authenticate an adversarial writer.
use super::*;
use std::hash::Hash;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::FingerprintBuilder;
use zil_db::QueryKey;
use zil_ir::{CandidatePlan, CoreImport, TypedProgram, wire};

const MAGIC: &[u8; 8] = b"ZILQCP01";
const HEADER: usize = 80;
const MAX_PAYLOAD: usize = 128 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct QueryCacheReport {
    pub requested: bool,
    pub state: &'static str,
    pub key: Option<Fingerprint>,
    pub reason: Option<String>,
    pub stored: bool,
    pub write_reason: Option<String>,
    pub ct_steps: u64,
    pub ct_nodes: usize,
    pub ct_functions: usize,
}
impl Default for QueryCacheReport {
    fn default() -> Self {
        Self {
            requested: false,
            state: "disabled",
            key: None,
            reason: None,
            stored: false,
            write_reason: None,
            ct_steps: 0,
            ct_nodes: 0,
            ct_functions: 0,
        }
    }
}
impl QueryCacheReport {
    pub(super) fn requested(options: &CompileOptions, reason: &str) -> Self {
        if options.query_cache.is_none() {
            return Self::default();
        }
        Self {
            requested: true,
            state: "bypass",
            reason: Some(reason.into()),
            ..Self::default()
        }
    }
    pub(super) fn to_json(&self) -> String {
        let optional =
            |value: &Option<String>| value.as_deref().map_or_else(|| "null".into(), json_string);
        format!(
            "{{\"requested\":{},\"state\":{},\"key\":{},\"reason\":{},\"stored\":{},\"write_reason\":{},\"ct_steps\":{},\"ct_nodes\":{},\"ct_functions\":{}}}",
            self.requested,
            json_string(self.state),
            self.key
                .map_or_else(|| "null".into(), |k| json_string(&k.to_hex())),
            optional(&self.reason),
            self.stored,
            optional(&self.write_reason),
            self.ct_steps,
            self.ct_nodes,
            self.ct_functions
        )
    }
}

/// Original sequential project CT charges, retained even when VM work is reused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CtUsage {
    pub module: ModuleId,
    pub steps: u64,
    pub nodes: usize,
    pub functions: usize,
}
pub(super) struct Checkpoint {
    pub program: TypedProgram,
    pub candidate: CandidatePlan,
}
pub(super) struct Prepared {
    directory: PathBuf,
    path: PathBuf,
    key: Fingerprint,
    maximum: usize,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("query cache root must be a real private directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(invalid(
                "query cache directory must not be group/other writable",
            ));
        }
    }
    Ok(())
}

fn action_key(
    producer: Fingerprint,
    identity: &QueryKey,
    imports: &[Vec<CoreImport>],
    options: &CompileOptions,
) -> Fingerprint {
    let mut state = FingerprintBuilder::new();
    "ZIL-TRUSTED-FRONTEND-CHECKPOINT-CP12-v1".hash(&mut state);
    producer.hash(&mut state);
    // This project key already binds the full ordered source graph, rules,
    // schema, all eleven Limits, explicit build inputs and checked target.
    identity.fingerprint().hash(&mut state);
    imports.len().hash(&mut state);
    for edges in imports {
        edges.len().hash(&mut state);
        for edge in edges {
            edge.alias.hash(&mut state);
            edge.module.hash(&mut state);
            edge.span.hash(&mut state);
        }
    }
    // Conservative option identity; output/cache locations may reduce reuse,
    // but never widen it. Wall deadlines and diagnostic timing are not language
    // identities; cancellation is checked again on the current request.
    // closed_executable is deliberately excluded: it only changes machine
    // linkage after a fresh Verify; actual linkage is bound by the object key.
    options.backend.name().hash(&mut state);
    format!("{:?}", options.optimization).hash(&mut state);
    format!("{:?}", options.profile).hash(&mut state);
    format!("{:?}", options.windows).hash(&mut state);
    options.codegen.units.hash(&mut state);
    options.codegen.jobs.hash(&mut state);
    options.entry.hash(&mut state);
    options.output.hash(&mut state);
    options.clang.hash(&mut state);
    options.cache_dir.hash(&mut state);
    options.object_cache.hash(&mut state);
    std::env::var_os("ZIL_CLANG").hash(&mut state);
    std::env::var_os("ZIL_LINUX_LINKER").hash(&mut state);
    state.digest()
}

impl Prepared {
    pub(super) fn new(
        options: &CompileOptions,
        identity: &QueryKey,
        imports: &[Vec<CoreImport>],
        filesystem: bool,
        report: &mut QueryCacheReport,
    ) -> Option<Self> {
        let root = options.query_cache.as_ref()?;
        let bypass = if !filesystem {
            Some("only filesystem project entry is supported")
        } else if options.stop_after != Stage::Build {
            Some("only complete build is supported")
        } else if options.emit_dir.is_some() {
            Some("stage artifacts require the original pipeline")
        } else if !cfg!(target_os = "linux") || options.windows.is_some() {
            Some("first checkpoint profile requires a Linux host and native build")
        } else if ["LD_PRELOAD", "LD_AUDIT"]
            .into_iter()
            .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty()))
        {
            Some("dynamic-loader interposition prevents trusted producer identity")
        } else {
            None
        };
        if let Some(reason) = bypass {
            report.reason = Some(reason.into());
            return None;
        }
        let prepare = || -> io::Result<Self> {
            let producer =
                zil_backend_llvm::trusted_installed_tool_identity(&std::env::current_exe()?)?;
            let key = action_key(producer, identity, imports, options);
            private_directory(root)?;
            let directory = fs::canonicalize(root)?.join("frontend-cp12-v1");
            private_directory(&directory)?;
            let path = directory.join(format!("{key}.qcp"));
            Ok(Self {
                directory,
                path,
                key,
                maximum: options.limits.max_source_bytes.min(MAX_PAYLOAD),
            })
        };
        match prepare() {
            Ok(cache) => {
                report.key = Some(cache.key);
                report.state = "miss";
                report.reason = None;
                Some(cache)
            }
            Err(error) => {
                report.reason = Some(error.to_string());
                None
            }
        }
    }

    pub(super) fn load(
        &self,
        project: &ProjectSources,
        fingerprint: Fingerprint,
        options: &CompileOptions,
        report: &mut QueryCacheReport,
    ) -> Option<Checkpoint> {
        let load = || -> io::Result<(Checkpoint, Vec<CtUsage>)> {
            let metadata = fs::symlink_metadata(&self.path)?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() < HEADER as u64
                || metadata.len() > (self.maximum + HEADER) as u64
            {
                return Err(invalid("invalid checkpoint file/size"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o022 != 0 {
                    return Err(invalid("checkpoint file is not private"));
                }
            }
            let mut bytes = Vec::new();
            File::open(&self.path)?
                .take((self.maximum + HEADER + 1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() < HEADER
                || bytes.len() > self.maximum + HEADER
                || &bytes[..8] != MAGIC
                || bytes[8..40] != self.key.0
            {
                return Err(invalid("checkpoint protocol or identity mismatch"));
            }
            let length = u64::from_le_bytes(bytes[72..80].try_into().expect("header"));
            let payload = &bytes[HEADER..];
            if length != payload.len() as u64 || Fingerprint::of(payload).0 != bytes[40..72] {
                return Err(invalid("checkpoint length or checksum mismatch"));
            }
            let mut reader = Cursor {
                bytes: payload,
                position: 0,
            };
            let wire_length = reader.size()?;
            let metadata_length = reader.size()?;
            let count = reader.size()?;
            if count != project.modules.len() {
                return Err(invalid("CT usage module coverage mismatch"));
            }
            if count > reader.remaining() / 28 {
                return Err(invalid("truncated CT usage table"));
            }
            let mut usage = Vec::with_capacity(count);
            for _ in 0..count {
                usage.push(CtUsage {
                    module: ModuleId(reader.u32()?),
                    steps: reader.u64()?,
                    nodes: reader.size()?,
                    functions: reader.size()?,
                });
            }
            replay_usage(&usage, project, &options.limits)?;
            let (module, candidate) =
                wire::decode_module_candidate(reader.take(wire_length)?, &options.limits)
                    .map_err(|_| invalid("checkpoint module/candidate codec rejected"))?;
            let program = wire::decode_query_cache_metadata(
                reader.take(metadata_length)?,
                module,
                &options.limits,
            )
            .map_err(|_| invalid("checkpoint project metadata codec rejected"))?;
            if reader.remaining() != 0
                || program.module.schema_version != zil_base::SCHEMA_VERSION
                || program.module.module_id != project.root
                || program.module.source_fingerprint != fingerprint
                || program.module.target != options.target
                || !program
                    .interfaces
                    .iter()
                    .map(|i| i.module)
                    .eq(project.modules.iter().map(|s| s.module_id))
            {
                return Err(invalid("checkpoint current project binding mismatch"));
            }
            Ok((Checkpoint { program, candidate }, usage))
        };
        match load() {
            Ok((checkpoint, usage)) => {
                report.state = "hit";
                record_usage(&usage, report);
                Some(checkpoint)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                report.state = "corrupt";
                report.reason = Some(error.to_string());
                None
            }
        }
    }

    pub(super) fn store(
        &self,
        program: &TypedProgram,
        candidate: &CandidatePlan,
        usage: Option<&[CtUsage]>,
        project: &ProjectSources,
        options: &CompileOptions,
        report: &mut CompileReport,
    ) {
        let Some(usage) = usage else {
            report.query_cache.write_reason =
                Some("preexisting in-memory CT snapshot has no checkpoint usage receipt".into());
            return;
        };
        record_usage(usage, &mut report.query_cache);
        let store = || -> io::Result<()> {
            if options.cancellation.is_cancelled() {
                return Err(invalid("checkpoint publication cancelled"));
            }
            if project
                .modules
                .iter()
                .any(|s| Path::new(&s.path) == self.path)
                || report.artifacts.values().any(|p| p == &self.path)
            {
                return Err(invalid(
                    "checkpoint destination overlaps a request input/output",
                ));
            }
            replay_usage(usage, project, &options.limits)?;
            let module = wire::encode_module_candidate(&program.module, candidate, &options.limits)
                .map_err(|_| invalid("checkpoint module/candidate exceeds codec bounds"))?;
            let metadata =
                wire::encode_query_cache_metadata(program, &options.limits).map_err(|_| {
                    invalid(
                        "checkpoint metadata exceeds codec bounds or carries verified relations",
                    )
                })?;
            let size = usage
                .len()
                .checked_mul(28)
                .and_then(|n| n.checked_add(24))
                .and_then(|n| n.checked_add(module.len()))
                .and_then(|n| n.checked_add(metadata.len()))
                .filter(|n| *n <= self.maximum)
                .ok_or_else(|| invalid("checkpoint payload exceeds transport bound"))?;
            let mut payload = Vec::with_capacity(size);
            payload.extend((module.len() as u64).to_le_bytes());
            payload.extend((metadata.len() as u64).to_le_bytes());
            payload.extend((usage.len() as u64).to_le_bytes());
            for row in usage {
                payload.extend(row.module.0.to_le_bytes());
                payload.extend(row.steps.to_le_bytes());
                payload.extend((row.nodes as u64).to_le_bytes());
                payload.extend((row.functions as u64).to_le_bytes());
            }
            payload.extend(module);
            payload.extend(metadata);
            self.publish(&payload, &options.cancellation)
        };
        match store() {
            Ok(()) => report.query_cache.stored = true,
            Err(error) => report.query_cache.write_reason = Some(error.to_string()),
        }
    }

    fn publish(&self, payload: &[u8], cancellation: &CancellationToken) -> io::Result<()> {
        private_directory(&self.directory)?;
        let temp = self.directory.join(format!(
            ".pending-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        // A create_new collision leaves a file owned by someone else intact.
        let mut temporary = Temporary(Some(temp.clone()));
        file.write_all(MAGIC)?;
        file.write_all(&self.key.0)?;
        file.write_all(&Fingerprint::of(payload).0)?;
        file.write_all(&(payload.len() as u64).to_le_bytes())?;
        file.write_all(payload)?;
        file.flush()?;
        drop(file);
        if cancellation.is_cancelled() {
            return Err(invalid("checkpoint publication cancelled"));
        }
        fs::rename(&temp, &self.path)?;
        temporary.0 = None;
        Ok(())
    }
}

fn replay_usage(usage: &[CtUsage], project: &ProjectSources, limits: &Limits) -> io::Result<()> {
    if usage.len() != project.modules.len() {
        return Err(invalid("CT usage does not cover current modules"));
    }
    let mut remaining = limits.clone();
    for (row, source) in usage.iter().zip(&project.modules) {
        if row.module != source.module_id {
            return Err(invalid("CT usage module order changed"));
        }
        remaining.max_ct_steps = remaining
            .max_ct_steps
            .checked_sub(row.steps)
            .ok_or_else(|| invalid("project CT step budget exceeded"))?;
        remaining.max_nodes = remaining
            .max_nodes
            .checked_sub(row.nodes)
            .ok_or_else(|| invalid("project CT node budget exceeded"))?;
        remaining.max_functions = remaining
            .max_functions
            .checked_sub(row.functions)
            .ok_or_else(|| invalid("project CT function budget exceeded"))?;
    }
    Ok(())
}
fn record_usage(usage: &[CtUsage], report: &mut QueryCacheReport) {
    report.ct_steps = usage.iter().map(|r| r.steps).sum();
    report.ct_nodes = usage.iter().map(|r| r.nodes).sum();
    report.ct_functions = usage.iter().map(|r| r.functions).sum();
}
struct Temporary(Option<PathBuf>);
impl Drop for Temporary {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}
struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(count)
            .filter(|n| *n <= self.bytes.len())
            .ok_or_else(|| invalid("truncated checkpoint section"))?;
        let bytes = &self.bytes[self.position..end];
        self.position = end;
        Ok(bytes)
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("width")))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("width")))
    }
    fn size(&mut self) -> io::Result<usize> {
        usize::try_from(self.u64()?).map_err(|_| invalid("checkpoint size does not fit host"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_ir::{FunctionId, PassingMode, ScalarFunction, ScalarInterface, Type};

    fn source() -> SourceUnit {
        SourceUnit::new(FileId(7), ModuleId(3), "key.zil", "fn main()->i32=>42")
    }
    fn key(source: &SourceUnit, options: &CompileOptions) -> Fingerprint {
        action_key(
            Fingerprint::of(b"installed compiler"),
            &super::super::session::query_key(source, options, Phase::Types, true),
            &[],
            options,
        )
    }
    fn program() -> TypedProgram {
        let source = source();
        let limits = Limits::default();
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &parsed).unwrap();
        let module = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
        let root = ScalarFunction {
            name: "main".into(),
            function: FunctionId(0),
            parameters: vec![],
            result: Type::I32,
            result_mode: PassingMode::Copy,
        };
        TypedProgram {
            module,
            interfaces: vec![ScalarInterface {
                module: source.module_id,
                exports: vec![root.clone()],
                signature_fingerprint: Fingerprint::of(b"signature"),
                implementation_fingerprint: Fingerprint::of(b"implementation"),
                result_origins: None,
                callable_contracts: None,
                callable_returns: None,
                contract_fingerprint: None,
            }],
            root_functions: vec![root],
        }
    }

    #[test]
    fn all_eleven_limits_change_the_actual_checkpoint_key() {
        let source = source();
        let baseline = CompileOptions::default();
        let original = key(&source, &baseline);
        for field in 0..11 {
            let mut options = baseline.clone();
            let l = &mut options.limits;
            match field {
                0 => l.max_source_bytes -= 1,
                1 => l.max_tokens -= 1,
                2 => l.max_nodes -= 1,
                3 => l.max_functions -= 1,
                4 => l.max_blocks -= 1,
                5 => l.max_instructions -= 1,
                6 => l.max_locals -= 1,
                7 => l.max_values -= 1,
                8 => l.max_analysis_steps -= 1,
                9 => l.max_ct_steps -= 1,
                10 => l.max_call_depth -= 1,
                _ => unreachable!(),
            }
            assert_ne!(key(&source, &options), original, "Limits field {field}");
        }
    }

    #[test]
    fn key_binds_producer_source_identity_rules_target_imports_and_build_options() {
        let source = source();
        let baseline = CompileOptions::default();
        let original = key(&source, &baseline);
        let producer = Fingerprint::of(b"installed compiler");
        let identity = super::super::session::query_key(&source, &baseline, Phase::Types, true);
        assert_ne!(
            action_key(
                Fingerprint::of(b"other compiler"),
                &identity,
                &[],
                &baseline
            ),
            original
        );
        let mut rules = identity.clone();
        rules.rules = Fingerprint::of(b"new rules/schema");
        assert_ne!(action_key(producer, &rules, &[], &baseline), original);
        for changed in [
            SourceUnit::new(FileId(8), source.module_id, &source.path, &source.text),
            SourceUnit::new(source.file_id, ModuleId(4), &source.path, &source.text),
            SourceUnit::new(source.file_id, source.module_id, "moved.zil", &source.text),
            SourceUnit::new(
                source.file_id,
                source.module_id,
                &source.path,
                "fn main()->i32=>43",
            ),
        ] {
            assert_ne!(key(&changed, &baseline), original);
        }
        let mut changes = Vec::new();
        let mut options = baseline.clone();
        options.build_inputs = Fingerprint::of(b"manifest");
        changes.push(options);
        let mut options = baseline.clone();
        options.target.rule_version.push_str("-changed");
        changes.push(options);
        let mut options = baseline.clone();
        options.entry = Some("value".into());
        changes.push(options);
        let mut options = baseline.clone();
        options.optimization = Optimization::O2;
        changes.push(options);
        let mut options = baseline.clone();
        options.codegen.units = 2;
        changes.push(options);
        let mut options = baseline.clone();
        options.clang = Some("other-clang".into());
        changes.push(options);
        for options in changes {
            assert_ne!(key(&source, &options), original);
        }
        let edge = CoreImport {
            alias: "math".into(),
            module: ModuleId(9),
            span: source.span(),
        };
        let imports = vec![vec![edge.clone()], vec![]];
        let imported = action_key(producer, &identity, &imports, &baseline);
        assert_ne!(imported, original);
        assert_ne!(
            action_key(
                producer,
                &identity,
                &[vec![], vec![edge.clone()]],
                &baseline
            ),
            imported
        );
        let mut edge_alias = edge.clone();
        edge_alias.alias = "other".into();
        assert_ne!(
            action_key(producer, &identity, &[vec![edge_alias], vec![]], &baseline),
            imported
        );
        let mut edge_module = edge;
        edge_module.module = ModuleId(10);
        assert_ne!(
            action_key(producer, &identity, &[vec![edge_module], vec![]], &baseline),
            imported
        );
        let mut observational = baseline.clone();
        observational.timings = true;
        observational.closed_executable = true;
        observational.query_cache = Some("another-private-location".into());
        observational.cancellation.cancel();
        assert_eq!(key(&source, &observational), original);
    }

    #[test]
    fn ct_receipt_replays_cumulative_steps_nodes_functions_and_module_order() {
        let project = ProjectSources {
            root: ModuleId(3),
            modules: vec![
                source(),
                SourceUnit::new(FileId(8), ModuleId(4), "dep.zil", ""),
            ],
            paths: BTreeMap::new(),
        };
        let rows = vec![
            CtUsage {
                module: ModuleId(3),
                steps: 2,
                nodes: 3,
                functions: 1,
            },
            CtUsage {
                module: ModuleId(4),
                steps: 5,
                nodes: 7,
                functions: 2,
            },
        ];
        let exact = Limits {
            max_ct_steps: 7,
            max_nodes: 10,
            max_functions: 3,
            ..Limits::default()
        };
        assert!(replay_usage(&rows, &project, &exact).is_ok());
        for lower in [
            Limits {
                max_ct_steps: 6,
                ..exact.clone()
            },
            Limits {
                max_nodes: 9,
                ..exact.clone()
            },
            Limits {
                max_functions: 2,
                ..exact.clone()
            },
        ] {
            assert!(replay_usage(&rows, &project, &lower).is_err());
        }
        assert!(replay_usage(&rows[..1], &project, &exact).is_err());
        let reversed = vec![rows[1].clone(), rows[0].clone()];
        assert!(replay_usage(&reversed, &project, &exact).is_err());
    }

    #[test]
    fn private_metadata_roundtrip_has_no_verified_relations_and_rejects_each_kind() {
        let limits = Limits::default();
        let program = program();
        let bytes = wire::encode_query_cache_metadata(&program, &limits).unwrap();
        assert_eq!(
            wire::decode_query_cache_metadata(&bytes, program.module.clone(), &limits).unwrap(),
            program
        );
        for field in 0..4 {
            let mut qualified = program.clone();
            let interface = &mut qualified.interfaces[0];
            match field {
                0 => interface.result_origins = Some(vec![]),
                1 => interface.callable_contracts = Some(vec![]),
                2 => interface.callable_returns = Some(vec![]),
                3 => interface.contract_fingerprint = Some(Fingerprint::ZERO),
                _ => unreachable!(),
            }
            assert!(wire::encode_query_cache_metadata(&qualified, &limits).is_err());
        }
        for kind in 0..4 {
            let mut malformed = bytes.clone();
            match kind {
                0 => malformed.push(0),
                1 => malformed[0] ^= 1,
                2 => malformed[8] ^= 1,
                3 => {
                    malformed.pop();
                }
                _ => unreachable!(),
            }
            assert!(
                wire::decode_query_cache_metadata(&malformed, program.module.clone(), &limits)
                    .is_err()
            );
        }
        let too_small = Limits {
            max_source_bytes: bytes.len() - 1,
            ..limits
        };
        assert!(wire::encode_query_cache_metadata(&program, &too_small).is_err());
        assert!(wire::decode_query_cache_metadata(&bytes, program.module, &too_small).is_err());
    }

    #[test]
    fn cancelled_atomic_publication_keeps_old_entry_and_removes_temporary() {
        let directory = std::env::temp_dir().join(format!(
            "cp12-publish-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        private_directory(&directory).unwrap();
        let path = directory.join("entry.qcp");
        let cache = Prepared {
            directory: directory.clone(),
            path: path.clone(),
            key: Fingerprint::of(b"key"),
            maximum: 1000,
        };
        cache.publish(b"first", &CancellationToken::new()).unwrap();
        let original = fs::read(&path).unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(cache.publish(b"replacement", &cancellation).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(target_os = "linux")]
    mod publication_cp27 {
        use super::*;
        use std::os::unix::fs::MetadataExt;
        use std::process::Command;

        const CHILD: &str = "ZIL_CP27_QUERY_PUBLICATION_DIRECTORY";

        struct Case(Prepared);
        impl Case {
            fn new() -> Self {
                let directory = std::env::temp_dir().join(format!(
                    "zil-cp27-query-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                private_directory(&directory).unwrap();
                Self(prepared(directory))
            }
        }
        impl Drop for Case {
            fn drop(&mut self) {
                if std::thread::panicking() {
                    eprintln!(
                        "CP27 query failure retained: {}",
                        self.0.directory.display()
                    );
                } else {
                    let _ = fs::remove_dir_all(&self.0.directory);
                }
            }
        }
        fn prepared(directory: PathBuf) -> Prepared {
            Prepared {
                path: directory.join("entry.qcp"),
                directory,
                key: Fingerprint::of(b"cp27-query"),
                maximum: 1000,
            }
        }
        fn child(case: &Case, test: &str, zero_file_limit: bool) {
            let exe = std::env::current_exe().unwrap();
            let mut command = if zero_file_limit {
                let mut command = Command::new("sh");
                command
                    .arg("-c")
                    .arg("ulimit -f 0 || exit 91; trap '' XFSZ; exec \"$@\"")
                    .arg("cp27-query-file-limit")
                    .arg(&exe);
                command
            } else {
                Command::new(&exe)
            };
            let output = command
                .args(["--exact", test, "--nocapture", "--test-threads=1"])
                .env(CHILD, &case.0.directory)
                .output()
                .unwrap();
            assert!(
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
                "child {test}: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        fn only_final(cache: &Prepared) {
            let paths: Vec<_> = fs::read_dir(&cache.directory)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            assert_eq!(paths, vec![cache.path.clone()]);
        }

        #[test]
        fn create_new_collision_preserves_unowned_file_and_old_entry() {
            if let Some(directory) = std::env::var_os(CHILD) {
                let cache = prepared(directory.into());
                let old = fs::read(&cache.path).unwrap();
                // An isolated exact-test process has not allocated a name yet.
                let pending = cache
                    .directory
                    .join(format!(".pending-{}-0", std::process::id()));
                fs::write(&pending, b"another owner's file").unwrap();
                let before = fs::metadata(&pending).unwrap();
                let error = cache
                    .publish(b"replacement", &CancellationToken::new())
                    .unwrap_err();
                assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
                assert_eq!(fs::read(&pending).unwrap(), b"another owner's file");
                let after = fs::metadata(&pending).unwrap();
                assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
                assert_eq!(before.modified().unwrap(), after.modified().unwrap());
                assert_eq!(fs::read(&cache.path).unwrap(), old);
                fs::remove_file(pending).unwrap();
                only_final(&cache);
                return;
            }
            let case = Case::new();
            case.0
                .publish(b"previous", &CancellationToken::new())
                .unwrap();
            child(
                &case,
                "query_cache::tests::publication_cp27::create_new_collision_preserves_unowned_file_and_old_entry",
                false,
            );
            case.0.publish(b"retry", &CancellationToken::new()).unwrap();
            assert_eq!(&fs::read(&case.0.path).unwrap()[HEADER..], b"retry");
            only_final(&case.0);
        }

        #[test]
        fn rename_failure_preserves_destination_and_cleans_owned_file() {
            let case = Case::new();
            fs::create_dir(&case.0.path).unwrap();
            let sentinel = case.0.path.join("keep");
            fs::write(&sentinel, b"existing directory contents").unwrap();
            assert!(
                case.0
                    .publish(b"replacement", &CancellationToken::new())
                    .is_err()
            );
            assert_eq!(fs::read(&sentinel).unwrap(), b"existing directory contents");
            only_final(&case.0);
            fs::remove_file(sentinel).unwrap();
            fs::remove_dir(&case.0.path).unwrap();
            case.0.publish(b"retry", &CancellationToken::new()).unwrap();
            assert_eq!(&fs::read(&case.0.path).unwrap()[HEADER..], b"retry");
            only_final(&case.0);
        }

        #[test]
        fn real_write_failure_preserves_old_entry_and_atomic_retry() {
            if let Some(directory) = std::env::var_os(CHILD) {
                let cache = prepared(directory.into());
                let old = fs::read(&cache.path).unwrap();
                let error = cache
                    .publish(b"replacement", &CancellationToken::new())
                    .unwrap_err();
                assert_eq!(error.raw_os_error(), Some(27), "{error}");
                assert_eq!(fs::read(&cache.path).unwrap(), old);
                only_final(&cache);
                return;
            }
            let case = Case::new();
            case.0
                .publish(b"previous", &CancellationToken::new())
                .unwrap();
            let old = fs::read(&case.0.path).unwrap();
            let mut old_reader = fs::File::open(&case.0.path).unwrap();
            child(
                &case,
                "query_cache::tests::publication_cp27::real_write_failure_preserves_old_entry_and_atomic_retry",
                true,
            );
            assert_eq!(fs::read(&case.0.path).unwrap(), old);
            case.0.publish(b"retry", &CancellationToken::new()).unwrap();
            assert_eq!(&fs::read(&case.0.path).unwrap()[HEADER..], b"retry");
            let mut still_old = Vec::new();
            old_reader.read_to_end(&mut still_old).unwrap();
            assert_eq!(still_old, old);
            only_final(&case.0);
        }
    }
}
