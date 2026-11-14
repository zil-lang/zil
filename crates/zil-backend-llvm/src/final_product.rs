//! Session-local final values. No verifier capability is stored here: callers
//! must first bind current verified LIR and authorize each current machine unit.
use crate::incremental::DiagnosticText;
use crate::{BuildArtifact, BuildOptions};
use std::borrow::Cow;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zil_base::{Fingerprint, FingerprintBuilder};
use zil_llvm_api::model::Module;

const MAX_PROGRAM: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    unix: [u64; 9],
}
impl Stamp {
    fn read(path: &Path) -> io::Result<Self> {
        let m = fs::symlink_metadata(path)?;
        if !m.is_file() {
            return Err(io::Error::other("artifact is not a regular file"));
        }
        Ok(Self {
            len: m.len(),
            modified: m.modified()?,
            #[cfg(unix)]
            unix: {
                use std::os::unix::fs::MetadataExt;
                [
                    m.dev(),
                    m.ino(),
                    m.mode() as u64,
                    m.uid() as u64,
                    m.gid() as u64,
                    m.nlink(),
                    m.ctime() as u64,
                    m.ctime_nsec() as u64,
                    m.mtime_nsec() as u64,
                ]
            },
        })
    }
}

/// All codegen configuration is retained, including scheduling and cache policy.
/// Only publication destinations are excluded: the isolated linker sees fixed
/// /inputs/unit-N.o and /out/program paths, independently of those destinations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Configuration {
    options: BuildOptions,
    namespace: Fingerprint,
    umask: u32,
    scoped_threads: bool,
    late_candidates: Option<Vec<Vec<String>>>,
}
impl Configuration {
    pub(crate) fn new(
        options: &BuildOptions,
        namespace: Fingerprint,
        scoped_threads: bool,
        late_candidates: Option<Vec<Vec<String>>>,
    ) -> Option<Self> {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        let mask = status
            .lines()
            .find_map(|line| line.strip_prefix("Umask:"))?;
        let umask = u32::from_str_radix(mask.trim(), 8).ok()?;
        let mut options = options.clone();
        options.output = PathBuf::new();
        options.published_paths = None;
        Some(Self {
            options,
            namespace,
            umask,
            scoped_threads,
            late_candidates,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Published {
    executable: PathBuf,
    llvm_ir: PathBuf,
    program_stamp: Stamp,
    diagnostic_stamp: Stamp,
}
impl Published {
    fn current(&self, executable: &Path, llvm_ir: &Path) -> bool {
        self.executable == executable
            && self.llvm_ir == llvm_ir
            && Stamp::read(executable).ok().as_ref() == Some(&self.program_stamp)
            && Stamp::read(llvm_ir).ok().as_ref() == Some(&self.diagnostic_stamp)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Product {
    configuration: Arc<Configuration>,
    profile: Arc<crate::native_link::Profile>,
    units: Arc<Vec<Module>>,
    object_value: Fingerprint,
    program: Arc<Vec<u8>>,
    #[cfg(unix)]
    mode: u32,
    diagnostic: DiagnosticText,
    published: Option<Published>,
}
impl Product {
    pub(crate) fn matches(
        &self,
        config: &Configuration,
        profile: &crate::native_link::Profile,
        units: &[Module],
    ) -> bool {
        self.configuration.as_ref() == config
            && self.profile.id == profile.id
            && self.units.len() == units.len()
            && self
                .units
                .iter()
                .zip(units)
                .all(|(old, now)| old.triple == now.triple && old.items == now.items)
            && self.profile.current_manifest()
    }
    pub(crate) fn same_objects(
        &self,
        config: &Configuration,
        profile: &crate::native_link::Profile,
        object_value: Fingerprint,
    ) -> bool {
        self.configuration.as_ref() == config
            && self.profile.id == profile.id
            && self.object_value == object_value
            && self.profile.current_manifest()
    }
    pub(crate) fn capture(
        configuration: Configuration,
        profile: Arc<crate::native_link::Profile>,
        units: Vec<Module>,
        object_value: Fingerprint,
        executable: &Path,
        diagnostic: DiagnosticText,
    ) -> io::Result<Self> {
        let before = Stamp::read(executable)?;
        if before.len > MAX_PROGRAM {
            return Err(io::Error::other("final program retention limit"));
        }
        let program = fs::read(executable)?;
        if Stamp::read(executable)? != before {
            return Err(io::Error::other("final program changed during capture"));
        }
        Ok(Self {
            configuration: Arc::new(configuration),
            profile,
            units: Arc::new(units),
            object_value,
            program: Arc::new(program),
            #[cfg(unix)]
            mode: before.unix[2] as u32 & 0o7777,
            diagnostic,
            published: None,
        })
    }
    pub(crate) fn refreshed(
        self: &Arc<Self>,
        units: Cow<'_, [Module]>,
        diagnostic: DiagnosticText,
    ) -> Arc<Self> {
        // This is exact value retention AFTER the caller's current machine
        // authorization. Include diagnostic fingerprints here, even though
        // they are deliberately excluded by matches()/the machine checker.
        let same_units = self.units.as_slice() == units.as_ref();
        let same_diagnostic = self.diagnostic == diagnostic;
        if same_units && same_diagnostic {
            return self.clone();
        }
        Arc::new(Self {
            configuration: self.configuration.clone(),
            profile: self.profile.clone(),
            units: if same_units {
                self.units.clone()
            } else {
                Arc::new(units.into_owned())
            },
            object_value: self.object_value,
            program: self.program.clone(),
            #[cfg(unix)]
            mode: self.mode,
            diagnostic,
            published: if same_diagnostic {
                self.published.clone()
            } else {
                None
            },
        })
    }
    pub(crate) fn restore_program(&self, path: &Path) -> io::Result<()> {
        if !self.profile.current_manifest() {
            return Err(io::Error::other(
                "native profile changed before artifact reuse",
            ));
        }
        let mut file = File::create(path)?;
        file.write_all(&self.program)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(self.mode))?;
        }
        Ok(())
    }
    pub(crate) fn token(
        self: &Arc<Self>,
        destinations: Option<&(PathBuf, PathBuf)>,
    ) -> ArtifactReuse {
        let retained = destinations
            .and_then(|(exe, ir)| self.published.as_ref().filter(|p| p.current(exe, ir)))
            .cloned();
        ArtifactReuse {
            product: self.clone(),
            retained,
        }
    }
    fn record_published(self: &Arc<Self>, executable: &Path, llvm_ir: &Path) -> Option<Arc<Self>> {
        if !self.profile.current_manifest() {
            return None;
        }
        let program_stamp = Stamp::read(executable).ok()?;
        let diagnostic_stamp = Stamp::read(llvm_ir).ok()?;
        #[cfg(unix)]
        if diagnostic_stamp.unix[2] & 0o7777 != u64::from(0o666 & !self.configuration.umask) {
            return None;
        }
        #[cfg(unix)]
        if program_stamp.unix[2] & 0o7777 != u64::from(self.mode) {
            return None;
        }
        if let Some(published) = &self.published
            && published.current(executable, llvm_ir)
        {
            return Some(self.clone());
        }
        if program_stamp.len != self.program.len() as u64 {
            return None;
        }
        let mut program = File::open(executable).ok()?;
        let mut buffer = [0u8; 65536];
        let mut position = 0usize;
        loop {
            let count = program.read(&mut buffer).ok()?;
            if count == 0 {
                break;
            }
            if self.program.get(position..position.checked_add(count)?)? != &buffer[..count] {
                return None;
            }
            position += count;
        }
        if position != self.program.len() {
            return None;
        }
        // Stream the retained diagnostic through a comparator. No aggregate
        // copy, digest shortcut or file alias can grant a publication lease.
        let mut comparator = CompareWriter {
            input: File::open(llvm_ir).ok()?,
        };
        self.diagnostic.write_to(&mut comparator).ok()?;
        if comparator.input.read(&mut buffer[..1]).ok()? != 0
            || Stamp::read(executable).ok()? != program_stamp
            || Stamp::read(llvm_ir).ok()? != diagnostic_stamp
        {
            return None;
        }
        let mut next = self.as_ref().clone();
        next.published = Some(Published {
            executable: executable.into(),
            llvm_ir: llvm_ir.into(),
            program_stamp,
            diagnostic_stamp,
        });
        Some(Arc::new(next))
    }
}
struct CompareWriter {
    input: File,
}
impl Write for CompareWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut buffer = [0u8; 65536];
        for chunk in bytes.chunks(buffer.len()) {
            self.input.read_exact(&mut buffer[..chunk.len()])?;
            if &buffer[..chunk.len()] != chunk {
                return Err(io::Error::other(
                    "published diagnostic differs from current producer",
                ));
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Opaque current-build publication evidence. It contains no language verifier
/// capability and is never imported from a file or a caller-supplied digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactReuse {
    product: Arc<Product>,
    retained: Option<Published>,
}
impl ArtifactReuse {
    pub fn retained_paths(&self) -> Option<(&Path, &Path)> {
        self.retained
            .as_ref()
            .map(|p| (p.executable.as_path(), p.llvm_ir.as_path()))
    }
    pub fn current_inputs(&self) -> bool {
        self.product.profile.current_manifest()
    }
    pub fn current_for(&self, executable: &Path, llvm_ir: &Path) -> bool {
        self.current_inputs()
            && self
                .retained
                .as_ref()
                .is_some_and(|p| p.current(executable, llvm_ir))
    }
    pub fn restore(&self, executable: &Path, llvm_ir: &Path) -> io::Result<()> {
        self.product.restore_program(executable)?;
        self.product.diagnostic.write(llvm_ir)?;
        if !self.current_inputs() {
            return Err(io::Error::other(
                "native profile changed during artifact restoration",
            ));
        }
        Ok(())
    }
    pub fn diagnostic_text(&self) -> String {
        self.product.diagnostic.to_text()
    }
}
impl crate::BackendSession {
    /// Called by the driver only after its two-file publication transaction.
    /// A raced/mutated output merely declines its optional file lease.
    pub fn record_published(&mut self, artifact: &BuildArtifact) {
        if let Some(reuse) = &artifact.artifact_reuse {
            self.final_product = reuse
                .product
                .record_published(&artifact.executable, &artifact.llvm_ir)
                .or_else(|| Some(reuse.product.clone()));
        }
    }
}

pub(crate) fn object_value(objects: &[PathBuf]) -> io::Result<Fingerprint> {
    let mut state = FingerprintBuilder::new();
    state.update(b"ZIL final ordered native object values v1");
    state.update(&(objects.len() as u64).to_le_bytes());
    let mut buffer = [0u8; 65536];
    for path in objects {
        let before = Stamp::read(path)?;
        state.update(&before.len.to_le_bytes());
        let mut file = File::open(path)?;
        let mut count = 0u64;
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            state.update(&buffer[..size]);
            count += size as u64;
        }
        if count != before.len || Stamp::read(path)? != before {
            return Err(io::Error::other(
                "native object changed during final value observation",
            ));
        }
    }
    Ok(state.digest())
}

/// Actual work performed by final-value reuse. No object-cache hit, native LLVM
/// verifier execution or subprocess is fabricated for this path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub reused_program: bool,
    pub retained_files: bool,
    pub current_preflight_checked_units: Option<usize>,
    pub current_preflight_reused_units: Option<usize>,
    pub before_codegen: bool,
    pub profile_id: String,
}
impl Summary {
    pub fn to_json(&self) -> String {
        let checked = self
            .current_preflight_checked_units
            .map_or_else(|| "null".into(), |v| v.to_string());
        let reused = self
            .current_preflight_reused_units
            .map_or_else(|| "null".into(), |v| v.to_string());
        format!(
            "{{\"reused_program\":{},\"retained_files\":{},\"before_codegen\":{},\"current_preflight_checked_units\":{},\"current_preflight_reused_units\":{},\"profile_id\":{},\"llvm_verification_scope\":\"retained-producer\",\"current_llvm_verifier_executed\":{}}}",
            self.reused_program,
            self.retained_files,
            self.before_codegen,
            checked,
            reused,
            zil_base::json_string(&self.profile_id),
            if self.before_codegen { "false" } else { "null" }
        )
    }
}

/// Current authorization is separate from the retained value match. The sealed
/// checker receipt binds every triple and item; diagnostic fingerprints are
/// excluded by the checker's own input projection, not by this cache.
pub(crate) fn authorize(
    units: &[Module],
    previous: &[Arc<crate::incremental::PreflightProduct>],
) -> Result<(Vec<Arc<crate::incremental::PreflightProduct>>, usize), zil_llvm_api::Error> {
    let mut products = Vec::with_capacity(units.len());
    let mut reused = 0;
    for (index, module) in units.iter().enumerate() {
        // Position is just a cheap candidate selector. Even its full value
        // match confers no authority: the current checker below must accept.
        // Partition reordering retains the original exact-value fallback.
        let matches = |old: &&Arc<crate::incremental::PreflightProduct>| {
            old.module.triple == module.triple && old.module.items == module.items
        };
        let old = previous
            .get(index)
            .filter(matches)
            .or_else(|| previous.iter().find(matches));
        let (receipt, hit) = zil_llvm_api::validate_module_with_receipt(
            module,
            old.map(|old| old.receipt.as_ref()),
        )?;
        reused += usize::from(hit);
        products.push(
            if let Some(old) = old.filter(|old| hit && old.module == *module) {
                // The call above freshly matched the sealed proof to CURRENT
                // inputs. Only a completely unchanged container (including its
                // diagnostic fingerprint) can retain its old allocation as well.
                old.clone()
            } else {
                Arc::new(crate::incremental::PreflightProduct {
                    module: module.clone(),
                    receipt: Arc::new(receipt),
                })
            },
        );
    }
    Ok((products, reused))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Block, Function, Inst, Item, Linkage, Op, Signature, Ty, Val};

    fn module(value: i128) -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "diagnostic-original".into(),
            items: vec![Item::Function(Function {
                name: "value".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), Vec::new()),
                attrs: Vec::new(),
                blocks: Some(
                    vec![Block {
                        name: "entry".into(),
                        instructions: vec![Inst::new(
                            None,
                            Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(value)))),
                        )],
                    }]
                    .into(),
                ),
            })]
            .into(),
        }
    }

    #[test]
    fn current_preflight_keeps_exact_owners_and_rebinds_changed_or_reordered_values() {
        let units = vec![module(1), module(2)];
        let (old, reused) = authorize(&units, &[]).unwrap();
        assert_eq!(reused, 0);
        // Independently allocated equal items are values, not stale identity
        // grants: the current checker still has to match its sealed proof.
        let rebuilt = vec![module(1), module(2)];
        assert!(!rebuilt[0].items.ptr_eq(&units[0].items));
        let (same, reused) = authorize(&rebuilt, &old).unwrap();
        assert_eq!(reused, 2);
        assert!(same.iter().zip(&old).all(|(a, b)| Arc::ptr_eq(a, b)));
        let (duplicates, _) = authorize(&[module(1), module(1)], &[]).unwrap();
        let (same, reused) = authorize(&[module(1), module(1)], &duplicates).unwrap();
        assert_eq!(reused, 2);
        assert!(
            Arc::ptr_eq(&same[1], &duplicates[1]),
            "prefer the exact same slot"
        );
        let (reordered, reused) = authorize(&[module(2), module(1)], &old).unwrap();
        assert_eq!(reused, 2);
        assert!(Arc::ptr_eq(&reordered[0], &old[1]));
        assert!(Arc::ptr_eq(&reordered[1], &old[0]));
        let mut diagnostic = module(1);
        diagnostic.fingerprint = "current-diagnostic".into();
        let (changed, reused) = authorize(std::slice::from_ref(&diagnostic), &old).unwrap();
        assert_eq!(reused, 1);
        assert!(!Arc::ptr_eq(&changed[0], &old[0]));
        assert_eq!(changed[0].module, diagnostic);
        let mut triple = module(1);
        triple.triple = "x86_64-pc-windows-msvc".into();
        let (changed, reused) = authorize(std::slice::from_ref(&triple), &old).unwrap();
        assert_eq!(reused, 0);
        assert_eq!(changed[0].module, triple);
        assert_eq!(authorize(&[module(3)], &old).unwrap().1, 0);

        let mut invalid = module(1);
        let Item::Function(function) = &mut invalid.items[0] else {
            panic!("function fixture")
        };
        function.blocks.as_mut().unwrap()[0].instructions[0].op = Op::Return(None);
        let expected = zil_llvm_api::validate_module(&invalid).unwrap_err();
        assert_eq!(
            authorize(std::slice::from_ref(&invalid), &old).unwrap_err(),
            expected
        );
        // Even an exact caller-side module snapshot paired with an unrelated
        // genuine receipt must fail in the checker, not skip authorization.
        let counterfeit = Arc::new(crate::incremental::PreflightProduct {
            module: invalid.clone(),
            receipt: old[0].receipt.clone(),
        });
        assert_eq!(authorize(&[invalid], &[counterfeit]).unwrap_err(), expected);
        assert_eq!(authorize(&units, &old).unwrap().1, 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires ZIL_TEST_NATIVE_LINK_PROFILE; value/publication checks, no native execution"]
    fn shared_final_value_refreshes_diagnostic_and_declines_stale_publication_lease() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        struct Dir(PathBuf);
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let dir = Dir(std::env::temp_dir().join(format!(
            "zil-final-value-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&dir.0).unwrap();
        let exe = dir.0.join("program");
        let ir = dir.0.join("program.ll");
        let profile = Arc::new(
            crate::native_link::Profile::load(Path::new(
                &std::env::var_os("ZIL_TEST_NATIVE_LINK_PROFILE")
                    .expect("explicit installed profile required"),
            ))
            .unwrap(),
        );
        let configuration = Configuration::new(
            &BuildOptions::new(exe.clone()),
            Fingerprint::of(b"namespace"),
            false,
            None,
        )
        .unwrap();
        let diagnostic = DiagnosticText::from(
            "; ZIL verified LLVM module\n; verified-fingerprint: original\nretained body\n"
                .to_owned(),
        );
        let units = vec![module(1), module(2)];
        fs::write(&exe, b"retained program value").unwrap();
        diagnostic.write(&ir).unwrap();
        let original = Arc::new(
            Product::capture(
                configuration,
                profile,
                units.clone(),
                Fingerprint::of(b"objects"),
                &exe,
                diagnostic.clone(),
            )
            .unwrap(),
        );
        let published = original.record_published(&exe, &ir).unwrap();
        assert!(Arc::ptr_eq(&original.units, &published.units));
        let current = published.record_published(&exe, &ir).unwrap();
        assert!(Arc::ptr_eq(&current, &published));
        let unchanged = current.refreshed(Cow::Borrowed(&units), diagnostic.clone());
        assert!(Arc::ptr_eq(&unchanged, &current));
        let paths = (exe.clone(), ir.clone());
        assert!(unchanged.token(Some(&paths)).current_for(&exe, &ir));
        let moved = current.refreshed(Cow::Borrowed(&units), diagnostic.current("changed"));
        assert!(Arc::ptr_eq(&moved.program, &current.program));
        assert!(Arc::ptr_eq(&moved.configuration, &current.configuration));
        assert!(Arc::ptr_eq(&moved.units, &current.units));
        assert!(moved.published.is_none());
        assert!(
            moved.record_published(&exe, &ir).is_none(),
            "old diagnostic cannot receive a current lease"
        );
        moved.diagnostic.write(&ir).unwrap();
        let moved = moved.record_published(&exe, &ir).unwrap();
        assert!(moved.token(Some(&paths)).current_for(&exe, &ir));
        fs::write(&exe, b"changed program value").unwrap();
        assert!(moved.record_published(&exe, &ir).is_none());
        assert!(!moved.token(Some(&paths)).current_for(&exe, &ir));
    }
}
