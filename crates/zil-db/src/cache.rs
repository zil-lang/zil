use crate::CancellationToken;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::Fingerprint;

const MAGIC: &[u8; 8] = b"ZILRAW02";
const HEADER: usize = 80;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheCorruption {
    TooLarge,
    Truncated,
    Version,
    Identity,
    Length,
    Digest,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CacheRead {
    Missing,
    Candidate(Vec<u8>),
    Corrupt(CacheCorruption),
}
#[derive(Debug)]
pub enum CacheWriteError {
    Io(io::Error),
    Cancelled,
}
impl From<io::Error> for CacheWriteError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl std::fmt::Display for CacheWriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Cancelled => formatter.write_str("cache write cancelled before publication"),
        }
    }
}
impl std::error::Error for CacheWriteError {}

/// This store has only a raw byte API. Integrity is not semantic verification;
/// callers must reconstruct and check any candidate before using it as code.
#[derive(Debug)]
pub struct ArtifactCache {
    root: PathBuf,
    max_payload: usize,
}
impl ArtifactCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_limit(root, 64 * 1024 * 1024)
    }
    pub fn with_limit(root: impl Into<PathBuf>, max_payload: usize) -> Self {
        Self {
            root: root.into(),
            max_payload,
        }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn path(&self, key: Fingerprint) -> PathBuf {
        self.root.join(format!("{key}.candidate"))
    }
    pub fn inspect_candidate(&self, key: Fingerprint) -> io::Result<CacheRead> {
        let file = match File::open(self.path(key)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(CacheRead::Missing),
            Err(error) => return Err(error),
        };
        let maximum = (self.max_payload as u64).saturating_add(HEADER as u64);
        if file.metadata()?.len() > maximum {
            return Ok(CacheRead::Corrupt(CacheCorruption::TooLarge));
        }
        // The read is bounded even when the file grows after metadata was read.
        let mut raw = Vec::new();
        file.take(maximum.saturating_add(1)).read_to_end(&mut raw)?;
        if raw.len() as u64 > maximum {
            return Ok(CacheRead::Corrupt(CacheCorruption::TooLarge));
        }
        if raw.len() < HEADER {
            return Ok(CacheRead::Corrupt(CacheCorruption::Truncated));
        }
        if &raw[..8] != MAGIC {
            return Ok(CacheRead::Corrupt(CacheCorruption::Version));
        }
        if raw[8..40] != key.0 {
            return Ok(CacheRead::Corrupt(CacheCorruption::Identity));
        }
        let mut size_bytes = [0; 8];
        size_bytes.copy_from_slice(&raw[72..80]);
        let size = u64::from_le_bytes(size_bytes);
        if size > self.max_payload as u64 || size != (raw.len() - HEADER) as u64 {
            return Ok(CacheRead::Corrupt(CacheCorruption::Length));
        }
        let payload = &raw[HEADER..];
        if Fingerprint::of(payload).0 != raw[40..72] {
            return Ok(CacheRead::Corrupt(CacheCorruption::Digest));
        }
        Ok(CacheRead::Candidate(payload.to_vec()))
    }
    pub fn read_candidate(&self, key: Fingerprint) -> io::Result<Option<Vec<u8>>> {
        Ok(match self.inspect_candidate(key)? {
            CacheRead::Candidate(bytes) => Some(bytes),
            _ => None,
        })
    }
    pub fn write_candidate(&self, key: Fingerprint, payload: &[u8]) -> io::Result<()> {
        self.write_candidate_cancellable(key, payload, &CancellationToken::default())
            .map_err(|error| match error {
                CacheWriteError::Io(error) => error,
                CacheWriteError::Cancelled => {
                    io::Error::new(io::ErrorKind::Interrupted, "cache write cancelled")
                }
            })
    }
    pub fn write_candidate_cancellable(
        &self,
        key: Fingerprint,
        payload: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<(), CacheWriteError> {
        self.write_with_hook(key, payload, cancellation, || {})
    }
    fn write_with_hook(
        &self,
        key: Fingerprint,
        payload: &[u8],
        cancellation: &CancellationToken,
        before_publish: impl FnOnce(),
    ) -> Result<(), CacheWriteError> {
        if cancellation.is_cancelled() {
            return Err(CacheWriteError::Cancelled);
        }
        if payload.len() > self.max_payload {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cache payload budget exceeded",
            )
            .into());
        }
        fs::create_dir_all(&self.root)?;
        let path = self.root.join(format!(
            ".{key}.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let temporary = Temporary(path);
        file.write_all(MAGIC)?;
        file.write_all(&key.0)?;
        file.write_all(&Fingerprint::of(payload).0)?;
        file.write_all(&(payload.len() as u64).to_le_bytes())?;
        file.write_all(payload)?;
        file.sync_all()?;
        drop(file);
        before_publish();
        if cancellation.is_cancelled() {
            return Err(CacheWriteError::Cancelled);
        }
        if let Err(error) = fs::rename(&temporary.0, self.path(key)) {
            if self.read_candidate(key)?.as_deref() == Some(payload) {
                return Ok(());
            }
            return Err(error.into());
        }
        Ok(())
    }
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "zil-cache-test-{}-{}",
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

    #[test]
    fn each_corruption_class_is_untrusted_and_can_be_rebuilt() {
        let directory = Directory::new();
        let cache = ArtifactCache::new(&directory.0);
        let key = Fingerprint::of(b"key");
        cache.write_candidate(key, b"candidate").unwrap();
        let original = fs::read(cache.path(key)).unwrap();
        let mut cases = vec![(original[..20].to_vec(), CacheCorruption::Truncated)];
        let mut bad = original.clone();
        bad[7] = b'1';
        cases.push((bad, CacheCorruption::Version));
        let mut bad = original.clone();
        bad[8] ^= 1;
        cases.push((bad, CacheCorruption::Identity));
        let mut bad = original.clone();
        bad[72] ^= 1;
        cases.push((bad, CacheCorruption::Length));
        let mut bad = original;
        bad[80] ^= 1;
        cases.push((bad, CacheCorruption::Digest));
        for (bytes, reason) in cases {
            fs::write(cache.path(key), bytes).unwrap();
            assert_eq!(
                cache.inspect_candidate(key).unwrap(),
                CacheRead::Corrupt(reason)
            );
            assert_eq!(cache.read_candidate(key).unwrap(), None);
            cache.write_candidate(key, b"rebuilt candidate").unwrap();
            assert_eq!(
                cache.read_candidate(key).unwrap(),
                Some(b"rebuilt candidate".to_vec())
            );
        }
    }
    #[test]
    fn cancellation_after_real_temporary_write_cannot_publish_or_leave_partial_file() {
        let directory = Directory::new();
        let cache = ArtifactCache::new(&directory.0);
        let key = Fingerprint::of(b"key");
        let token = CancellationToken::default();
        let result = cache.write_with_hook(key, b"new candidate", &token, || token.cancel());
        assert!(matches!(result, Err(CacheWriteError::Cancelled)));
        assert_eq!(cache.read_candidate(key).unwrap(), None);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
        cache.write_candidate(key, b"rebuilt").unwrap();
        let token = CancellationToken::default();
        assert!(matches!(
            cache.write_with_hook(key, b"replacement", &token, || token.cancel()),
            Err(CacheWriteError::Cancelled)
        ));
        assert_eq!(
            cache.read_candidate(key).unwrap(),
            Some(b"rebuilt".to_vec())
        );
    }
    #[test]
    fn incomplete_temp_files_are_not_committed_candidates() {
        let directory = Directory::new();
        let cache = ArtifactCache::new(&directory.0);
        let key = Fingerprint::of(b"key");
        fs::write(
            directory.0.join(format!(".{key}.interrupted.tmp")),
            b"ZILRAW02partial",
        )
        .unwrap();
        assert_eq!(cache.read_candidate(key).unwrap(), None);
        cache.write_candidate(key, b"rebuilt").unwrap();
        assert_eq!(
            cache.read_candidate(key).unwrap(),
            Some(b"rebuilt".to_vec())
        );
    }
    #[test]
    fn oversized_reads_and_writes_are_bounded() {
        let directory = Directory::new();
        let cache = ArtifactCache::with_limit(&directory.0, 1);
        let key = Fingerprint::ZERO;
        assert!(cache.write_candidate(key, b"too large").is_err());
        fs::write(cache.path(key), vec![0; HEADER + 2]).unwrap();
        assert_eq!(
            cache.inspect_candidate(key).unwrap(),
            CacheRead::Corrupt(CacheCorruption::TooLarge)
        );
    }
}
