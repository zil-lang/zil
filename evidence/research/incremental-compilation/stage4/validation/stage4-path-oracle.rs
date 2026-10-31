use std::{fs,io,path::{Path,PathBuf,Component}};
use zil_base::{CompileResult,Diagnostic,Phase,ResultCategory};
fn infra(code: &str, message: impl Into<String>) -> Vec<Diagnostic> { vec![Diagnostic::new(Phase::Driver,ResultCategory::InfraError,code,message,None)] }
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

#[derive(Debug,PartialEq,Eq)]
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

fn identity_uncached(path: &Path) -> CompileResult<FileIdentity> {
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
                resolved: destination(path)?,
                #[cfg(unix)]
                inode: None,
            })
        }
        Err(error) => Err(infra("E-OUTPUT-PATH", error.to_string())),
    }
}

fn main() {
    use std::os::unix::{fs::symlink,ffi::OsStringExt};
    let root=std::env::temp_dir().join(format!("zil-stage4-oracle-{}",std::process::id()));
    fs::create_dir_all(root.join("a/deep")).unwrap();fs::create_dir(root.join("b")).unwrap();
    fs::write(root.join("ordinary-file"),b"source").unwrap();
    fs::write(root.join("a/existing"),b"existing").unwrap();
    fs::hard_link(root.join("a/existing"),root.join("a/hardlink")).unwrap();
    symlink(root.join("a/deep"),root.join("alias")).unwrap();
    symlink(root.join("missing-target"),root.join("dangling")).unwrap();
    symlink(root.join("loop"),root.join("loop")).unwrap();
    symlink(root.join("a/existing"),root.join("a/final-link")).unwrap();
    symlink(root.join("absent-target"),root.join("a/final-dangling")).unwrap();
    let parents=["a","a/.","a//","a/deep/..","a/../b","alias","alias/..","alias/../..","missing","missing/deep","missing/../a","ordinary-file","ordinary-file/..","dangling","dangling/..","loop","loop/..",""];
    let names=["first.zil","second.zil","existing","hardlink","final-link","final-dangling",".","..",""];
    let mut paths=Vec::new();
    for parent in parents {for name in names {paths.push(root.join(format!("{parent}/{name}")));}}
    paths.extend([PathBuf::new(),PathBuf::from("/"),PathBuf::from("."),PathBuf::from(".."),PathBuf::from("./Cargo.toml"),PathBuf::from("./missing.zil"),PathBuf::from("Cargo.toml/child"),PathBuf::from("Cargo.toml/../child")]);
    paths.push(root.join("a").join(std::ffi::OsString::from_vec(vec![0xff,b'x'])));
    let mut cache=DestinationCache::default();let mut identity_cache=DestinationCache::default();
    let mut count=0;let mut successes=0;let mut rejects=0;
    for round in 0..4 {
        for offset in 0..paths.len() {
            let index=match round {0|1=>offset,2=>paths.len()-1-offset,_=>(offset*71)%paths.len()};
            let path=&paths[index];
            let expected=destination(path);assert_eq!(cache.resolve(path),expected,"destination {:?}",path);
            let expected_identity=identity_uncached(path);assert_eq!(identity(path,&mut identity_cache),expected_identity,"identity {:?}",path);
            if expected.is_ok(){successes+=1;}else{rejects+=1;}count+=1;
        }
    }
    let cwd=std::env::current_dir().unwrap();
    for dir in [&root,&root.join("a"),&root.join("b")] {
        std::env::set_current_dir(dir).unwrap();
        for name in ["first.zil","second.zil","./third.zil","../fourth.zil"] {
            assert_eq!(cache.resolve(Path::new(name)),destination(Path::new(name)));count+=1;
        }
    }
    std::env::set_current_dir(cwd).unwrap();
    println!("PASS {} resolver comparisons ({} normal-matrix successes, {} rejections; 12 changed-cwd controls), {} exact FileIdentity/Diagnostic comparisons",count,successes,rejects,paths.len()*4);
    fs::remove_dir_all(root).unwrap();
}
