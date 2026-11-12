//! A cache batch keeps the complete artifact guard at its boundaries. Between
//! them, Linux writes through a held directory and checks the identities of all
//! planned paths and their ancestors, avoiding repeated canonicalization and
//! all-source/output collision comparisons for each immutable item.
use super::*;

pub(crate) struct CacheNamespace {
    plan: ArtifactPlan,
    root: PathBuf,
    #[cfg(target_os = "linux")]
    directory: fs::File,
    #[cfg(target_os = "linux")]
    pinned: PathBuf,
    #[cfg(target_os = "linux")]
    stamps: Vec<(PathBuf, Option<Stamp>)>,
}

impl CacheNamespace {
    pub(super) fn new(plan: &ArtifactPlan, root: &Path) -> CompileResult<Self> {
        if !plan
            .files
            .iter()
            .any(|file| file.directory && file.path == root)
        {
            return Err(infra("E-OUTPUT-PATH", "cache directory was not planned"));
        }
        plan.validate()?;
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let directory =
                fs::File::open(root).map_err(|error| infra("E-OUTPUT-PATH", error.to_string()))?;
            let pinned = PathBuf::from(format!("/proc/self/fd/{}/.", directory.as_raw_fd()));
            let mut paths = std::collections::BTreeSet::new();
            for file in &plan.files {
                paths.extend(file.path.ancestors().map(Path::to_path_buf));
                match fs::canonicalize(&file.path) {
                    Ok(resolved) => paths.extend(resolved.ancestors().map(Path::to_path_buf)),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(infra("E-OUTPUT-PATH", error.to_string())),
                }
            }
            let stamps = paths
                .into_iter()
                .map(|path| {
                    let stamp = stamp(&path)?;
                    Ok((path, stamp))
                })
                .collect::<CompileResult<Vec<_>>>()?;
            let namespace = Self {
                plan: plan.clone(),
                root: root.to_path_buf(),
                directory,
                pinned,
                stamps,
            };
            namespace.validate()?;
            Ok(namespace)
        }
        #[cfg(not(target_os = "linux"))]
        Ok(Self {
            plan: plan.clone(),
            root: root.to_path_buf(),
        })
    }

    pub(crate) fn root(&self) -> &Path {
        #[cfg(target_os = "linux")]
        {
            &self.pinned
        }
        #[cfg(not(target_os = "linux"))]
        {
            &self.root
        }
    }

    pub(crate) fn validate_write(&self) -> CompileResult<()> {
        #[cfg(target_os = "linux")]
        {
            for (path, expected) in &self.stamps {
                if &stamp(path)? != expected {
                    return Err(infra(
                        "E-OUTPUT-PATH",
                        "artifact identity changed during cache publication",
                    ));
                }
            }
            self.validate_directory()
        }
        #[cfg(not(target_os = "linux"))]
        self.plan.validate()
    }

    pub(crate) fn validate(&self) -> CompileResult<()> {
        self.validate_write()?;
        self.plan.validate()
    }

    #[cfg(target_os = "linux")]
    fn validate_directory(&self) -> CompileResult<()> {
        use std::os::unix::fs::MetadataExt;
        let held = self
            .directory
            .metadata()
            .map_err(|error| infra("E-OUTPUT-PATH", error.to_string()))?;
        let current = fs::symlink_metadata(&self.root)
            .map_err(|error| infra("E-OUTPUT-PATH", error.to_string()))?;
        let pinned = fs::metadata(&self.pinned)
            .map_err(|error| infra("E-OUTPUT-PATH", error.to_string()))?;
        if !held.is_dir()
            || !current.is_dir()
            || !pinned.is_dir()
            || (held.dev(), held.ino()) != (current.dev(), current.ino())
            || (held.dev(), held.ino()) != (pinned.dev(), pinned.ino())
        {
            return Err(infra("E-OUTPUT-PATH", "cache directory identity changed"));
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[derive(PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    mode: u32,
    // Directory contents change as this batch publishes children. Their inode
    // and type bind path traversal; each relevant child has its own stamp.
    contents: Option<(u64, i64, i64, i64, i64)>,
}

#[cfg(target_os = "linux")]
fn stamp(path: &Path) -> CompileResult<Option<Stamp>> {
    use std::os::unix::fs::MetadataExt;
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(Stamp {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            contents: (!metadata.is_dir()).then(|| {
                (
                    metadata.len(),
                    metadata.mtime(),
                    metadata.mtime_nsec(),
                    metadata.ctime(),
                    metadata.ctime_nsec(),
                )
            }),
        })),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(infra("E-OUTPUT-PATH", error.to_string())),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn fixture() -> (Staging, ArtifactPlan, PathBuf) {
        let directory = Staging::create(&std::env::temp_dir()).unwrap();
        let path = directory.path.join("source.zil");
        fs::write(&path, "fn main()->i32{return 1;}").unwrap();
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            path.to_string_lossy(),
            "fn main()->i32{return 1;}",
        );
        let mut plan = ArtifactPlan::new(&source, &CompileOptions::default()).unwrap();
        let root = directory.path.join("items");
        fs::create_dir(&root).unwrap();
        plan.cache_directory("items", &root).unwrap();
        (directory, plan, root)
    }

    #[test]
    fn normal_items_do_not_invalidate_the_bound_namespace() {
        let (_directory, plan, root) = fixture();
        let bound = plan.pin_cache_directory(&root).unwrap();
        for item in 0..20 {
            bound.validate_write().unwrap();
            fs::write(bound.root().join(item.to_string()), b"item").unwrap();
        }
        bound.validate().unwrap();
        assert_eq!(fs::read_dir(root).unwrap().count(), 20);
    }

    #[test]
    fn namespace_replacement_cannot_redirect_pinned_writes() {
        let (directory, plan, root) = fixture();
        let bound = plan.pin_cache_directory(&root).unwrap();
        let held = directory.path.join("held");
        fs::rename(&root, &held).unwrap();
        let victim = directory.path.join("victim");
        fs::create_dir(&victim).unwrap();
        symlink(&victim, &root).unwrap();
        assert!(bound.validate_write().is_err());
        assert!(bound.validate().is_err());
        // Even a write after a race targets the held directory, not the alias.
        fs::write(bound.root().join("item"), b"item").unwrap();
        assert!(held.join("item").is_file());
        assert!(!victim.join("item").exists());
    }

    #[test]
    fn source_replacement_and_new_dangling_output_are_rejected() {
        let (directory, mut plan, root) = fixture();
        let output = directory.path.join("new-output");
        plan.cache_destination("manifest", &output).unwrap();
        let bound = plan.pin_cache_directory(&root).unwrap();
        symlink(directory.path.join("missing"), &output).unwrap();
        assert!(bound.validate_write().is_err());
        fs::remove_file(&output).unwrap();
        let bound = plan.pin_cache_directory(&root).unwrap();
        let source = directory.path.join("source.zil");
        fs::rename(&source, directory.path.join("old-source")).unwrap();
        fs::hard_link(directory.path.join("old-source"), &output).unwrap();
        fs::hard_link(&output, &source).unwrap();
        assert!(bound.validate_write().is_err());
        assert!(bound.validate().is_err());
    }
}
