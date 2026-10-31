//! File destinations are planned before any artifact write. Native compilation
//! stays private until the last cancellation check and fallible dump complete.
use super::*;
use std::io;
use std::path::Component;
use std::sync::atomic::{AtomicU64, Ordering};
use zil_backend_llvm::BuildArtifact;

const DUMPS: &[(&str, &str, Stage)] = &[
    ("syntax", "txt", Stage::Parse),
    ("core", "txt", Stage::Core),
    ("source-map", "txt", Stage::Core),
    ("selection", "json", Stage::Core),
    ("ct-core", "txt", Stage::Types),
    ("typed", "txt", Stage::Types),
    ("interfaces", "txt", Stage::Types),
    ("candidate", "txt", Stage::Analyze),
    ("verified", "txt", Stage::Verify),
    ("verified-interfaces", "txt", Stage::Verify),
    ("lir", "txt", Stage::Lower),
    ("llvm", "ll", Stage::Build),
];

struct PlannedFile {
    name: &'static str,
    path: PathBuf,
}

pub(super) struct ArtifactPlan {
    files: Vec<PlannedFile>,
}
impl ArtifactPlan {
    pub(super) fn project(
        source: &SourceUnit,
        sources: &[SourceUnit],
        options: &CompileOptions,
    ) -> CompileResult<Self> {
        let mut plan = Self::new(source, options)?;
        let mut dependencies = Vec::new();
        let mut directories = DestinationCache::default();
        for dependency in sources.iter().filter(|unit| unit.file_id != source.file_id) {
            dependencies.push(PlannedFile {
                name: "source",
                path: directories.resolve(Path::new(&dependency.path))?,
            });
        }
        plan.files.splice(1..1, dependencies);
        plan.validate()?;
        Ok(plan)
    }
    pub(super) fn new(source: &SourceUnit, options: &CompileOptions) -> CompileResult<Self> {
        let mut files = vec![PlannedFile {
            name: "source",
            path: destination(Path::new(&source.path))?,
        }];
        if let Some(root) = &options.emit_dir {
            for &(name, extension, stage) in DUMPS {
                if stage <= options.stop_after {
                    files.push(PlannedFile {
                        name,
                        path: destination(&root.join(format!("{name}.{extension}")))?,
                    });
                }
            }
        }
        if options.stop_after == Stage::Build {
            let output = options.output.clone().unwrap_or_else(|| {
                Path::new(&source.path).with_extension(if cfg!(windows) { "exe" } else { "" })
            });
            let executable = destination(&output)?;
            let mut name = executable
                .file_name()
                .expect("destination has a name")
                .to_os_string();
            name.push(".ll");
            files.push(PlannedFile {
                name: "sidecar",
                path: executable.with_file_name(name),
            });
            files.push(PlannedFile {
                name: "executable",
                path: executable,
            });
        }
        let plan = Self { files };
        plan.validate()?;
        Ok(plan)
    }

    fn path(&self, name: &str) -> Option<&Path> {
        self.files
            .iter()
            .find(|file| file.name == name)
            .map(|file| file.path.as_path())
    }

    /// Repeat the identity checks immediately before writes/publication. Stored
    /// destinations already resolve parent symlinks, so changing such an alias
    /// cannot redirect the operation after the initial preflight.
    pub(super) fn validate(&self) -> CompileResult<()> {
        // No resolution is retained across this check boundary. In particular,
        // observer callbacks may change aliases before a later publication.
        let mut directories = DestinationCache::default();
        let identities = self
            .files
            .iter()
            .map(|file| identity(&file.path, &mut directories))
            .collect::<CompileResult<Vec<_>>>()?;
        // Project sources precede the fixed-size output set. Check source
        // identities once, then compare each source only against outputs.
        // A quadratic all-source comparison would defeat graph work limits.
        let first_output = self
            .files
            .iter()
            .position(|file| file.name != "source")
            .unwrap_or(self.files.len());
        let mut sources: Vec<_> = identities[..first_output].iter().collect();
        sources.sort_by(|left, right| left.resolved.cmp(&right.resolved));
        if sources
            .windows(2)
            .any(|pair| pair[1].resolved.starts_with(&pair[0].resolved))
        {
            return Err(infra("E-OUTPUT-SOURCE", "project source paths overlap"));
        }
        #[cfg(unix)]
        {
            let mut inodes = std::collections::BTreeSet::new();
            for source in &sources {
                if let Some(inode) = source.inode
                    && !inodes.insert(inode)
                {
                    return Err(infra(
                        "E-OUTPUT-SOURCE",
                        "project source files share a physical identity",
                    ));
                }
            }
        }
        for (index, left) in self.files.iter().enumerate() {
            for (right_index, right) in self
                .files
                .iter()
                .enumerate()
                .skip((index + 1).max(first_output))
            {
                let a = &identities[index];
                let b = &identities[right_index];
                let overlap = a.resolved == b.resolved
                    || a.resolved.starts_with(&b.resolved)
                    || b.resolved.starts_with(&a.resolved)
                    || same_file(a, b);
                if overlap {
                    let code = if left.name == "source" {
                        "E-OUTPUT-SOURCE"
                    } else {
                        "E-OUTPUT-COLLISION"
                    };
                    return Err(infra(
                        code,
                        format!(
                            "{} ({}) conflicts with {} ({})",
                            left.name,
                            left.path.display(),
                            right.name,
                            right.path.display()
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Do not serialize an unrequested stage dump. The underlying publication,
    /// path validation, cancellation, and output bytes stay on the same path.
    pub(super) fn dump_lazy(
        &self,
        report: &mut CompileReport,
        options: &CompileOptions,
        name: &str,
        contents: impl FnOnce(&CompileReport) -> String,
    ) -> CompileResult<()> {
        if self.path(name).is_none() {
            return self.dump(report, options, name, "");
        }
        let contents = contents(report);
        self.dump(report, options, name, &contents)
    }

    pub(super) fn dump(
        &self,
        report: &mut CompileReport,
        options: &CompileOptions,
        name: &str,
        contents: &str,
    ) -> CompileResult<()> {
        options
            .cancellation
            .check(report.phase)
            .map_err(|error| vec![error])?;
        let Some(path) = self.path(name) else {
            return Ok(());
        };
        self.validate()?;
        let parent = path.parent().expect("planned destination is absolute");
        fs::create_dir_all(parent).map_err(|error| infra("E-ARTIFACT-IO", error.to_string()))?;
        let temporary = Staging::create(parent)?;
        let staged = temporary.path.join("dump");
        fs::write(&staged, contents).map_err(|error| infra("E-ARTIFACT-IO", error.to_string()))?;
        options
            .cancellation
            .check(report.phase)
            .map_err(|error| vec![error])?;
        self.validate()?;
        // Replacing a directory entry never truncates another hard link's data.
        fs::rename(&staged, path).map_err(|error| infra("E-ARTIFACT-IO", error.to_string()))?;
        report.artifacts.insert(name.into(), path.to_path_buf());
        Ok(())
    }

    pub(super) fn native(&self) -> CompileResult<NativePublication> {
        self.validate()?;
        NativePublication::new(
            self.path("executable")
                .expect("build has an executable")
                .to_path_buf(),
            self.path("sidecar")
                .expect("build has a sidecar")
                .to_path_buf(),
        )
    }
}

/// Resolve directory symlinks, including the existing prefix of a not-yet-created
/// directory. The final component remains a directory entry: publication replaces
/// that entry rather than silently following an output symlink to another file.
fn destination(path: &Path) -> CompileResult<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| infra("E-OUTPUT-PATH", "a file destination must have a name"))?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| infra("E-OUTPUT-PATH", error.to_string()))?
            .join(path)
    };
    let mut parent = PathBuf::new();
    for component in absolute
        .parent()
        .expect("absolute file has a parent")
        .components()
    {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parent.pop();
            }
            other => {
                parent.push(other.as_os_str());
            }
        }
        match fs::metadata(&parent) {
            Ok(metadata) if metadata.is_dir() => {
                parent = fs::canonicalize(&parent)
                    .map_err(|error| infra("E-OUTPUT-PATH", error.to_string()))?;
            }
            Ok(_) => {
                return Err(infra(
                    "E-OUTPUT-PATH",
                    format!("{} is not a directory", parent.display()),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if fs::symlink_metadata(&parent).is_ok() {
                    return Err(infra(
                        "E-OUTPUT-PATH",
                        format!("{} is a dangling directory symlink", parent.display()),
                    ));
                }
            }
            Err(error) => return Err(infra("E-OUTPUT-PATH", error.to_string())),
        }
    }
    Ok(parent.join(name))
}

/// One successful parent per synchronous construction/validation batch. File
/// identities, failures and later checks are never cached. Keep the original
/// resolver as the miss path, including symlink-before-ParentDir semantics.
#[derive(Default)]
struct DestinationCache {
    last: Option<(PathBuf, PathBuf)>,
}
impl DestinationCache {
    fn resolve(&mut self, path: &Path) -> CompileResult<PathBuf> {
        let name = path
            .file_name()
            .ok_or_else(|| infra("E-OUTPUT-PATH", "a file destination must have a name"))?;
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|error| infra("E-OUTPUT-PATH", error.to_string()))?
                .join(path)
        };
        // Drive-relative Windows paths can remain relative after joining the
        // process directory; their implicit per-drive state is not a cache key.
        if !absolute.is_absolute() {
            return destination(path);
        }
        let parent = absolute.parent().expect("absolute file has a parent");
        if let Some((previous, resolved)) = &self.last
            && previous == parent
        {
            return Ok(resolved.join(name));
        }
        let resolved = destination(&absolute)?;
        self.last = Some((
            parent.to_path_buf(),
            resolved
                .parent()
                .expect("resolved file has a parent")
                .to_path_buf(),
        ));
        Ok(resolved)
    }
}

struct FileIdentity {
    resolved: PathBuf,
    #[cfg(unix)]
    inode: Option<(u64, u64)>,
}
fn identity(path: &Path, directories: &mut DestinationCache) -> CompileResult<FileIdentity> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(infra(
                    "E-OUTPUT-PATH",
                    format!("{} must be a regular file", path.display()),
                ));
            }
            #[cfg(unix)]
            let inode = {
                use std::os::unix::fs::MetadataExt;
                Some((metadata.dev(), metadata.ino()))
            };
            Ok(FileIdentity {
                resolved: fs::canonicalize(path)
                    .map_err(|error| infra("E-OUTPUT-PATH", error.to_string()))?,
                #[cfg(unix)]
                inode,
            })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A dangling output symlink has an unknown referent; fail closed.
            if fs::symlink_metadata(path).is_ok() {
                return Err(infra(
                    "E-OUTPUT-PATH",
                    format!("{} is a dangling symlink", path.display()),
                ));
            }
            Ok(FileIdentity {
                resolved: directories.resolve(path)?,
                #[cfg(unix)]
                inode: None,
            })
        }
        Err(error) => Err(infra("E-OUTPUT-PATH", error.to_string())),
    }
}
fn same_file(left: &FileIdentity, right: &FileIdentity) -> bool {
    #[cfg(unix)]
    {
        left.inode.is_some() && left.inode == right.inode
    }
    #[cfg(not(unix))]
    {
        left.resolved == right.resolved
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Staging {
    path: PathBuf,
    preserve: bool,
}
impl Staging {
    fn create(parent: &Path) -> CompileResult<Self> {
        for _ in 0..100 {
            let path = parent.join(format!(
                ".zil-driver-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        preserve: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(infra("E-ARTIFACT-IO", error.to_string())),
            }
        }
        Err(infra(
            "E-ARTIFACT-IO",
            "cannot reserve a private artifact directory",
        ))
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

struct PublicationMember {
    staged: PathBuf,
    target: PathBuf,
    backup: PathBuf,
    backed_up: bool,
    published: bool,
}
pub(super) struct NativePublication {
    staging: Staging,
    // Publish the IR first, then the executable. Neither is reported before both
    // renames succeed; any failure restores both old directory entries.
    members: [PublicationMember; 2],
    committed: bool,
}
impl NativePublication {
    fn new(executable: PathBuf, llvm_ir: PathBuf) -> CompileResult<Self> {
        let parent = executable.parent().expect("planned executable is absolute");
        fs::create_dir_all(parent).map_err(|error| infra("E-ARTIFACT-IO", error.to_string()))?;
        let staging = Staging::create(parent)?;
        let member = |name: &str, target| PublicationMember {
            staged: staging.path.join(name),
            target,
            backup: staging.path.join(format!("previous-{name}")),
            backed_up: false,
            published: false,
        };
        let members = [member("program.ll", llvm_ir), member("program", executable)];
        Ok(Self {
            staging,
            members,
            committed: false,
        })
    }
    pub(super) fn output(&self) -> &Path {
        &self.members[1].staged
    }

    pub(super) fn commit(
        mut self,
        mut artifact: BuildArtifact,
        cancellation: &CancellationToken,
    ) -> CompileResult<BuildArtifact> {
        if artifact.executable != self.members[1].staged
            || artifact.llvm_ir != self.members[0].staged
        {
            return Err(infra(
                "E-OUTPUT-PUBLISH",
                "backend produced a file outside this request's private destination",
            ));
        }
        if let Err(mut errors) = self.publish(cancellation) {
            let rollback_errors = self.rollback();
            if !rollback_errors.is_empty() {
                self.staging.preserve = true;
                errors[0].notes.push(format!(
                    "rollback incomplete; previous artifacts retained in {}: {}",
                    self.staging.path.display(),
                    rollback_errors.join("; ")
                ));
            }
            return Err(errors);
        }
        self.committed = true;
        artifact.llvm_ir = self.members[0].target.clone();
        artifact.executable = self.members[1].target.clone();
        Ok(artifact)
    }

    fn publish(&mut self, cancellation: &CancellationToken) -> CompileResult<()> {
        for member in &mut self.members {
            cancellation
                .check(Phase::Link)
                .map_err(|error| vec![error])?;
            match fs::symlink_metadata(&member.target) {
                Ok(metadata) => {
                    if !metadata.is_file() && !metadata.file_type().is_symlink() {
                        return Err(infra(
                            "E-OUTPUT-PUBLISH",
                            "native destination is no longer a file",
                        ));
                    }
                    fs::rename(&member.target, &member.backup)
                        .map_err(|error| infra("E-OUTPUT-PUBLISH", error.to_string()))?;
                    member.backed_up = true;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(infra("E-OUTPUT-PUBLISH", error.to_string())),
            }
        }
        for member in &mut self.members {
            cancellation
                .check(Phase::Link)
                .map_err(|error| vec![error])?;
            fs::rename(&member.staged, &member.target)
                .map_err(|error| infra("E-OUTPUT-PUBLISH", error.to_string()))?;
            member.published = true;
        }
        cancellation
            .check(Phase::Link)
            .map_err(|error| vec![error])?;
        Ok(())
    }

    fn rollback(&mut self) -> Vec<String> {
        let mut errors = Vec::new();
        for member in self.members.iter_mut().rev() {
            if member.backed_up {
                match fs::rename(&member.backup, &member.target) {
                    Ok(()) => {
                        member.backed_up = false;
                        member.published = false;
                    }
                    Err(error) => {
                        errors.push(format!("restore {}: {error}", member.target.display()))
                    }
                }
            } else if member.published {
                match fs::remove_file(&member.target) {
                    Ok(()) => member.published = false,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        member.published = false
                    }
                    Err(error) => {
                        errors.push(format!("remove {}: {error}", member.target.display()))
                    }
                }
            }
        }
        errors
    }
}
impl Drop for NativePublication {
    fn drop(&mut self) {
        if !self.committed && !self.rollback().is_empty() {
            // Never let temporary-directory cleanup erase the only old copy.
            self.staging.preserve = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_parent_resolution_matches_the_uncached_resolver() {
        let directory = Staging::create(&std::env::temp_dir()).unwrap();
        let root = &directory.path;
        fs::create_dir(root.join("a")).unwrap();
        fs::create_dir(root.join("b")).unwrap();
        fs::write(root.join("not-a-directory"), "file").unwrap();
        let paths = vec![
            root.join("a/first.zil"),
            root.join("a/second.zil"),
            root.join("a/missing/nested/first.zil"),
            root.join("a/missing/nested/second.zil"),
            root.join("a/../b/first.zil"),
            root.join("a/../b/second.zil"),
            root.join("not-a-directory/first.zil"),
            root.join("not-a-directory/second.zil"),
            PathBuf::from("./Cargo.toml"),
            PathBuf::from("./missing-source.zil"),
            PathBuf::from("/"),
            PathBuf::new(),
            root.join("a/.."),
        ];
        #[cfg(unix)]
        let paths = {
            use std::os::unix::fs::symlink;
            let mut paths = paths;
            fs::create_dir_all(root.join("deep/inner")).unwrap();
            symlink(root.join("deep/inner"), root.join("alias")).unwrap();
            symlink(root.join("absent"), root.join("dangling")).unwrap();
            paths.extend([
                root.join("alias/first.zil"),
                root.join("alias/second.zil"),
                root.join("alias/../first.zil"),
                root.join("alias/../second.zil"),
                root.join("dangling/first.zil"),
                root.join("dangling/second.zil"),
            ]);
            paths
        };
        let mut cached = DestinationCache::default();
        for path in paths.iter().cycle().take(paths.len() * 3) {
            assert_eq!(
                cached.resolve(path),
                destination(path),
                "{}",
                path.display()
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn later_validation_rechecks_files_parent_directories_and_aliases() {
        let directory = Staging::create(&std::env::temp_dir()).unwrap();
        let root = &directory.path;
        fs::write(root.join("a.zil"), "source a").unwrap();
        fs::write(root.join("b.zil"), "source b").unwrap();
        fs::create_dir(root.join("output")).unwrap();
        let sources = [
            SourceUnit::new(
                FileId(0),
                zil_base::ModuleId(0),
                root.join("a.zil").to_string_lossy(),
                "source a",
            ),
            SourceUnit::new(
                FileId(1),
                zil_base::ModuleId(1),
                root.join("b.zil").to_string_lossy(),
                "source b",
            ),
        ];
        let options = CompileOptions {
            stop_after: Stage::Build,
            output: Some(root.join("output/program")),
            ..CompileOptions::default()
        };
        let plan = ArtifactPlan::project(&sources[0], &sources, &options).unwrap();
        plan.validate().unwrap();
        fs::remove_file(root.join("b.zil")).unwrap();
        fs::hard_link(root.join("a.zil"), root.join("b.zil")).unwrap();
        assert_eq!(plan.validate().unwrap_err()[0].code, "E-OUTPUT-SOURCE");
        fs::remove_file(root.join("b.zil")).unwrap();
        fs::write(root.join("b.zil"), "source b").unwrap();
        plan.validate().unwrap();
        fs::remove_dir(root.join("output")).unwrap();
        fs::write(root.join("output"), "not a directory anymore").unwrap();
        assert_eq!(plan.validate().unwrap_err()[0].code, "E-OUTPUT-PATH");
        fs::remove_file(root.join("output")).unwrap();
        fs::create_dir(root.join("output")).unwrap();
        plan.validate().unwrap();
        fs::hard_link(root.join("a.zil"), root.join("output/program")).unwrap();
        assert_eq!(plan.validate().unwrap_err()[0].code, "E-OUTPUT-SOURCE");
        fs::remove_file(root.join("output/program")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(root.join("a.zil"), root.join("output/program")).unwrap();
            assert_eq!(plan.validate().unwrap_err()[0].code, "E-OUTPUT-SOURCE");
            fs::remove_file(root.join("output/program")).unwrap();
            symlink(root.join("missing.zil"), root.join("output/program")).unwrap();
            assert_eq!(plan.validate().unwrap_err()[0].code, "E-OUTPUT-PATH");
        }
        assert_eq!(fs::read_to_string(root.join("a.zil")).unwrap(), "source a");
    }

    #[test]
    fn second_native_rename_failure_restores_both_previous_outputs() {
        let directory = Staging::create(&std::env::temp_dir()).unwrap();
        let executable = directory.path.join("program");
        let ir = directory.path.join("program.ll");
        fs::write(&executable, "previous executable").unwrap();
        fs::write(&ir, "previous ir").unwrap();
        let publication = NativePublication::new(executable.clone(), ir.clone()).unwrap();
        let staging_path = publication.staging.path.clone();
        fs::write(&publication.members[0].staged, "replacement ir").unwrap();
        // A real filesystem error on the second rename, after the first new
        // member was published. This is transaction evidence, not ZIL acceptance.
        let artifact = BuildArtifact {
            executable: publication.members[1].staged.clone(),
            llvm_ir: publication.members[0].staged.clone(),
            command: Vec::new(),
            commands: Vec::new(),
            codegen: Default::default(),
            pending_cache: Default::default(),
        };
        let error = publication
            .commit(artifact, &CancellationToken::default())
            .unwrap_err();
        assert_eq!(error[0].code, "E-OUTPUT-PUBLISH");
        assert_eq!(
            fs::read_to_string(&executable).unwrap(),
            "previous executable"
        );
        assert_eq!(fs::read_to_string(&ir).unwrap(), "previous ir");
        assert!(!staging_path.exists());
    }
}
