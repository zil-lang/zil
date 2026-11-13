//! Explicit, content-pinned Linux link toolchains. The snapshot is the complete
//! filesystem visible to ld; arbitrary --clang drivers keep their original path.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use zil_base::{Fingerprint, FingerprintBuilder};

const POLICY: &str = include_str!("native_link.policy");
const MAX_FILES: usize = 4096;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
fn bad(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: std::time::SystemTime,
    directory: bool,
    #[cfg(unix)]
    unix: [u64; 9],
}
impl Stamp {
    fn read(path: &Path) -> io::Result<Self> {
        let m = fs::symlink_metadata(path)?;
        if !m.is_file() && !m.is_dir() {
            return Err(bad("link profile contains a non-regular owner"));
        }
        Ok(Self {
            len: m.len(),
            modified: m.modified()?,
            directory: m.is_dir(),
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
fn hex(text: &str) -> io::Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) || text.len() > 16384 {
        return Err(bad("invalid link-profile hex field"));
    }
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| {
            let digit = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                _ => None,
            };
            Ok(digit(p[0]).ok_or_else(|| bad("invalid hex"))? * 16
                + digit(p[1]).ok_or_else(|| bad("invalid hex"))?)
        })
        .collect()
}
fn text(encoded: &str) -> io::Result<String> {
    let bytes = hex(encoded)?;
    if bytes.contains(&0) {
        return Err(bad("NUL in link profile"));
    }
    String::from_utf8(bytes).map_err(|_| bad("non-UTF8 link profile field"))
}
fn absolute(encoded: &str) -> io::Result<PathBuf> {
    let p = PathBuf::from(text(encoded)?);
    if !p.is_absolute()
        || p.components()
            .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
    {
        return Err(bad("noncanonical link profile path"));
    }
    Ok(p)
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Argument {
    Literal(String),
    Objects,
    Output,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct FileSpec {
    size: u64,
    mode: u32,
    sha: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Spec {
    loader: PathBuf,
    bwrap: PathBuf,
    libdirs: Vec<PathBuf>,
    variants: BTreeMap<(bool, bool), Vec<Argument>>,
    files: BTreeMap<PathBuf, FileSpec>,
    dirs: BTreeSet<PathBuf>,
}
impl Spec {
    fn parse(value: &str) -> io::Result<Self> {
        let mut lines = value.lines();
        if lines.next() != Some("ZILLINK1") {
            return Err(bad("unknown native link profile"));
        }
        let mut loader = None;
        let mut bwrap = None;
        let mut libdirs = vec![];
        let mut variants = BTreeMap::new();
        let mut active: Option<((bool, bool), Vec<Argument>)> = None;
        let mut files = BTreeMap::new();
        let mut dirs = BTreeSet::new();
        let mut target = false;
        let mut end = false;
        for line in lines {
            if end {
                return Err(bad("trailing native link profile data"));
            }
            let f: Vec<_> = line.split_ascii_whitespace().collect();
            match f.as_slice() {
                ["TARGET", v] if !target && text(v)? == "x86_64-unknown-linux-gnu" => target = true,
                ["LOADER", v] if loader.is_none() => loader = Some(absolute(v)?),
                ["BWRAP", v] if bwrap.is_none() => bwrap = Some(absolute(v)?),
                ["LIBDIR", v] => {
                    let p = absolute(v)?;
                    if libdirs.contains(&p) {
                        return Err(bad("duplicate loader directory"));
                    }
                    libdirs.push(p);
                }
                ["VARIANT", opt, threads] if active.is_none() => {
                    let opt = match *opt {
                        "O0" => false,
                        "O2" => true,
                        _ => return Err(bad("unknown link optimization")),
                    };
                    let threads = match *threads {
                        "0" => false,
                        "1" => true,
                        _ => return Err(bad("unknown thread mode")),
                    };
                    active = Some(((opt, threads), vec![]));
                }
                ["ARG", v] => active
                    .as_mut()
                    .ok_or_else(|| bad("argument outside variant"))?
                    .1
                    .push(Argument::Literal(text(v)?)),
                ["OBJECTS"] => active
                    .as_mut()
                    .ok_or_else(|| bad("objects outside variant"))?
                    .1
                    .push(Argument::Objects),
                ["OUTPUT"] => active
                    .as_mut()
                    .ok_or_else(|| bad("output outside variant"))?
                    .1
                    .push(Argument::Output),
                ["ENDVARIANT"] => {
                    let (key, args) = active.take().ok_or_else(|| bad("unmatched variant end"))?;
                    if args.iter().filter(|a| **a == Argument::Objects).count() != 1
                        || args.iter().filter(|a| **a == Argument::Output).count() != 1
                        || !matches!(args.first(),Some(Argument::Literal(p)) if Path::new(p).is_absolute())
                        || variants.insert(key, args).is_some()
                    {
                        return Err(bad("incomplete/duplicate link variant"));
                    }
                }
                ["FILE", p, size, mode, sha] => {
                    let p = absolute(p)?;
                    let size = size.parse().map_err(|_| bad("bad file size"))?;
                    let mode = u32::from_str_radix(mode, 8).map_err(|_| bad("bad file mode"))?;
                    if sha.len() != 64
                        || hex(sha)?.len() != 32
                        || files
                            .insert(
                                p,
                                FileSpec {
                                    size,
                                    mode,
                                    sha: (*sha).into(),
                                },
                            )
                            .is_some()
                    {
                        return Err(bad("bad/duplicate file identity"));
                    }
                }
                ["DIR", p] => {
                    if !dirs.insert(absolute(p)?) {
                        return Err(bad("duplicate profile directory"));
                    }
                }
                ["END"] if active.is_none() => end = true,
                _ => return Err(bad("unsupported native link profile field")),
            }
        }
        if !target
            || !end
            || variants.len() != 4
            || files.len() > MAX_FILES
            || dirs.len() > MAX_FILES
            || libdirs.is_empty()
        {
            return Err(bad("incomplete native link profile"));
        }
        let loader = loader.ok_or_else(|| bad("missing loader"))?;
        let bwrap = bwrap.ok_or_else(|| bad("missing sandbox"))?;
        if !files.contains_key(&loader)||!files.contains_key(&bwrap)||variants.values().any(|v|!matches!(v.first(),Some(Argument::Literal(p)) if files.contains_key(Path::new(p)))) {return Err(bad("unbound native link executable"));}
        for p in ["/inputs", "/out", "/work", "/proc", "/dev"] {
            if !dirs.contains(Path::new(p)) {
                return Err(bad("missing isolated link directory"));
            }
        }
        Ok(Self {
            loader,
            bwrap,
            libdirs,
            variants,
            files,
            dirs,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Profile {
    manifest: PathBuf,
    root: PathBuf,
    spec: Arc<Spec>,
    stamps: Vec<(PathBuf, Stamp)>,
    pub(crate) id: Fingerprint,
}
impl Profile {
    fn in_root(&self, path: &Path) -> PathBuf {
        self.root.join(
            path.strip_prefix("/")
                .expect("validated absolute profile path"),
        )
    }
    pub(crate) fn load(manifest: &Path) -> io::Result<Self> {
        if !cfg!(target_os = "linux") {
            return Err(bad("native link profiles require Linux"));
        }
        let manifest = fs::canonicalize(manifest)?;
        let before = Stamp::read(&manifest)?;
        let value = fs::read_to_string(&manifest)?;
        // This compiled anchor is reviewed toolchain policy, not a descriptor's
        // self-reported digest. Unknown providers cannot grant cache authority.
        if value != POLICY || value.len() > 2 * 1024 * 1024 {
            return Err(bad(
                "native link profile is not the compiled toolchain policy",
            ));
        }
        let spec = Arc::new(Spec::parse(&value)?);
        let root = manifest
            .parent()
            .ok_or_else(|| bad("profile has no parent"))?
            .join("root");
        if fs::canonicalize(&root)? != root {
            return Err(bad("profile root aliases another path"));
        }
        let mut result = Self {
            manifest: manifest.clone(),
            root,
            spec,
            stamps: vec![(manifest.clone(), before)],
            id: Fingerprint::of(value.as_bytes()),
        };
        let mut actual_files = BTreeSet::new();
        let mut actual_dirs = BTreeSet::new();
        let mut pending = vec![result.root.clone()];
        while let Some(p) = pending.pop() {
            let stamp = Stamp::read(&p)?;
            let relative = Path::new("/").join(
                p.strip_prefix(&result.root)
                    .map_err(|_| bad("profile path escaped"))?,
            );
            if stamp.directory {
                actual_dirs.insert(relative);
                for child in fs::read_dir(&p)? {
                    pending.push(child?.path());
                }
            } else {
                actual_files.insert(relative);
            }
            if actual_files.len() + actual_dirs.len() > 2 * MAX_FILES {
                return Err(bad("native profile inventory limit"));
            }
        }
        let expected_files = result.spec.files.keys().cloned().collect::<BTreeSet<_>>();
        let mut expected_dirs = result.spec.dirs.clone();
        expected_dirs.insert(PathBuf::from("/"));
        if actual_files != expected_files || actual_dirs != expected_dirs {
            return Err(bad("native profile inventory differs from pinned policy"));
        }
        let mut total = 0u64;
        for (path, entry) in &result.spec.files {
            total = total
                .checked_add(entry.size)
                .filter(|n| *n <= MAX_BYTES)
                .ok_or_else(|| bad("native profile byte limit"))?;
            let p = result.in_root(path);
            let stamp = Stamp::read(&p)?;
            if stamp.directory || stamp.len != entry.size {
                return Err(bad("native profile size mismatch"));
            }
            #[cfg(unix)]
            if stamp.unix[2] & 0o7777 != entry.mode as u64 {
                return Err(bad("native profile mode mismatch"));
            }
            let mut file = File::open(&p)?;
            let mut hash = FingerprintBuilder::new();
            let mut bytes = [0; 65536];
            loop {
                let n = file.read(&mut bytes)?;
                if n == 0 {
                    break;
                }
                hash.update(&bytes[..n]);
            }
            if hash.digest().to_hex() != entry.sha || Stamp::read(&p)? != stamp {
                return Err(bad("native profile content changed"));
            }
            result.stamps.push((p, stamp));
        }
        for path in &expected_dirs {
            let p = result.in_root(path);
            let stamp = Stamp::read(&p)?;
            if !stamp.directory {
                return Err(bad("profile directory replaced"));
            }
            result.stamps.push((p, stamp));
        }
        if !result.current(&manifest) {
            return Err(bad("native link profile changed during load"));
        }
        Ok(result)
    }
    pub(crate) fn current_manifest(&self) -> bool {
        self.current(&self.manifest)
    }
    pub(crate) fn current(&self, manifest: &Path) -> bool {
        // The outer loader executes before the mount namespace. Interposition
        // via the host preload file is outside this closed provider domain.
        matches!(fs::symlink_metadata("/etc/ld.so.preload"),Err(e) if e.kind()==io::ErrorKind::NotFound)
            && fs::canonicalize(manifest).ok().as_ref() == Some(&self.manifest)
            && self
                .stamps
                .iter()
                .all(|(p, s)| Stamp::read(p).ok().as_ref() == Some(s))
    }
    pub(crate) fn link(
        &self,
        objects: &[PathBuf],
        output: &Path,
        optimization: crate::Optimization,
        threads: bool,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> io::Result<Vec<String>> {
        if cancelled() {
            return Err(bad("native profile link cancelled"));
        }
        if objects.is_empty() || objects.len() > 64 || !self.current_manifest() {
            return Err(bad("native link profile no longer current"));
        }
        let parent = output
            .parent()
            .ok_or_else(|| bad("link output has no parent"))?;
        let inputs = parent.join("profile-inputs");
        let outputs = parent.join("profile-outputs");
        fs::create_dir(&inputs)?;
        fs::create_dir(&outputs)?;
        for (n, path) in objects.iter().enumerate() {
            let dst = inputs.join(format!("unit-{n}.o"));
            fs::hard_link(path, &dst).or_else(|_| fs::copy(path, &dst).map(|_| ()))?;
        }
        let host_libpath = std::env::join_paths(self.spec.libdirs.iter().map(|p| self.in_root(p)))
            .map_err(|_| bad("invalid profile loader path"))?;
        let inner_libpath = std::env::join_paths(&self.spec.libdirs)
            .map_err(|_| bad("invalid isolated loader path"))?;
        let mut command = Command::new(self.in_root(&self.spec.loader));
        command
            .args([
                "--inhibit-cache",
                "--glibc-hwcaps-mask",
                "",
                "--library-path",
            ])
            .arg(host_libpath)
            .arg(self.in_root(&self.spec.bwrap))
            .args([
                "--unshare-user",
                "--uid",
                "0",
                "--gid",
                "0",
                "--unshare-pid",
                "--unshare-net",
                "--unshare-ipc",
                "--unshare-uts",
                "--die-with-parent",
                "--new-session",
                "--ro-bind",
            ])
            .arg(&self.root)
            .arg("/")
            .args(["--proc", "/proc", "--dev", "/dev", "--ro-bind"])
            .arg(&inputs)
            .arg("/inputs")
            .arg("--bind")
            .arg(&outputs)
            .arg("/out")
            .args([
                "--chdir",
                "/work",
                "--clearenv",
                "--setenv",
                "LC_ALL",
                "C",
                "--",
            ])
            .arg(&self.spec.loader)
            .args([
                "--inhibit-cache",
                "--glibc-hwcaps-mask",
                "",
                "--library-path",
            ])
            .arg(inner_libpath);
        let variant = &self.spec.variants[&(optimization == crate::Optimization::O2, threads)];
        for arg in variant {
            match arg {
                Argument::Literal(s) => {
                    command.arg(s);
                }
                Argument::Objects => {
                    for n in 0..objects.len() {
                        command.arg(format!("/inputs/unit-{n}.o"));
                    }
                }
                Argument::Output => {
                    command.arg("/out/program");
                }
            }
        }
        command.env_clear().env("LC_ALL", "C");
        let mut recorded = vec![command.get_program().to_string_lossy().into_owned()];
        recorded.extend(command.get_args().map(|s| s.to_string_lossy().into_owned()));
        let result = command.output()?;
        if !result.status.success() {
            return Err(bad(format!(
                "isolated native linker failed: {}",
                String::from_utf8_lossy(&result.stderr)
            )));
        }
        if cancelled() || !self.current_manifest() {
            return Err(bad("native link inputs changed before publication"));
        }
        let built = outputs.join("program");
        let meta = fs::symlink_metadata(&built)?;
        if !meta.is_file() {
            return Err(bad("native link did not produce a regular executable"));
        }
        fs::rename(built, output)?;
        Ok(recorded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_policy_has_four_bound_link_commands_and_current_output_slots() {
        let spec = Spec::parse(POLICY).expect("installed reviewed policy");
        for optimization in [false, true] {
            for threads in [false, true] {
                let args = &spec.variants[&(optimization, threads)];
                assert!(
                    matches!(&args[0], Argument::Literal(path) if spec.files.contains_key(Path::new(path)))
                );
                assert_eq!(
                    args.iter().filter(|arg| **arg == Argument::Objects).count(),
                    1
                );
                let output = args
                    .iter()
                    .position(|arg| *arg == Argument::Output)
                    .unwrap();
                assert!(output > 0);
                assert_eq!(args[output - 1], Argument::Literal("-o".into()));
                assert_eq!(
                    args.iter().filter(|arg| **arg == Argument::Output).count(),
                    1
                );
            }
        }
    }

    #[test]
    fn malformed_variants_and_trailing_inventory_never_create_a_profile() {
        assert!(Spec::parse(&POLICY.replacen("ZILLINK1", "ZILLINK2", 1)).is_err());
        assert!(Spec::parse(&POLICY.replacen("VARIANT O2 0", "VARIANT O0 0", 1)).is_err());
        assert!(Spec::parse(&(POLICY.to_owned() + "DIR 2f696e76656e746564\n")).is_err());
        let loader = POLICY
            .lines()
            .find(|line| line.starts_with("LOADER "))
            .unwrap();
        assert!(
            Spec::parse(&POLICY.replacen(loader, "LOADER 2f756e626f756e642d6c6f61646572", 1))
                .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_syntactically_valid_manifest_cannot_authorize_its_own_changed_bytes() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "zil-untrusted-native-profile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let manifest = directory.join("manifest");
        let altered = POLICY.replacen("VARIANT O0 0", "VARIANT O0 0 ", 1);
        assert_ne!(altered, POLICY);
        assert!(Spec::parse(&altered).is_ok());
        fs::write(&manifest, altered).unwrap();
        let result = Profile::load(&manifest);
        fs::remove_file(&manifest).unwrap();
        fs::remove_dir(&directory).unwrap();
        let error = result.expect_err("only the compiled policy can grant authority");
        assert!(error.to_string().contains("compiled toolchain policy"));
    }
}
