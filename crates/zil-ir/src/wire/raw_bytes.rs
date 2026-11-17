//! A reversible raw dictionary row. Materialize its complete wire fragment
//! only when a byte consumer actually needs it, after final name rebinding.
use super::*;

#[derive(Clone)]
pub(super) enum RawBytes {
    Complete(Arc<[u8]>),
    Scalar(Arc<ScalarBytes>),
}
pub(super) struct ScalarBytes {
    template: Arc<[u8]>,
    id: FunctionId,
    name: String,
    complete: OnceLock<Arc<[u8]>>,
}
impl RawBytes {
    // Called only after the normalized scalar template passed the real Reader.
    pub(super) fn scalar(template: Arc<[u8]>, id: FunctionId, name: String) -> Self {
        Self::Scalar(Arc::new(ScalarBytes {
            template,
            id,
            name,
            complete: OnceLock::new(),
        }))
    }
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Complete(bytes) => bytes.len(),
            Self::Scalar(row) => row.template.len() + row.name.len(),
        }
    }
    fn complete(&self) -> &Arc<[u8]> {
        match self {
            Self::Complete(bytes) => bytes,
            Self::Scalar(row) => row.complete.get_or_init(|| {
                let mut bytes = Vec::with_capacity(self.len());
                bytes.extend_from_slice(&row.template[..6]);
                bytes.extend_from_slice(&row.id.0.to_le_bytes());
                bytes.extend_from_slice(&(row.name.len() as u32).to_le_bytes());
                bytes.extend_from_slice(row.name.as_bytes());
                bytes.extend_from_slice(&row.template[14..]);
                bytes.into()
            }),
        }
    }
    pub(super) fn shared(&self) -> Arc<[u8]> {
        self.complete().clone()
    }
    #[cfg(test)]
    pub(super) fn is_materialized(&self) -> bool {
        match self {
            Self::Complete(_) => true,
            Self::Scalar(row) => row.complete.get().is_some(),
        }
    }
}
impl std::ops::Deref for RawBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.complete()
    }
}
impl From<Arc<[u8]>> for RawBytes {
    fn from(bytes: Arc<[u8]>) -> Self {
        Self::Complete(bytes)
    }
}
impl From<Vec<u8>> for RawBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Complete(bytes.into())
    }
}
impl From<&[u8]> for RawBytes {
    fn from(bytes: &[u8]) -> Self {
        Self::Complete(bytes.into())
    }
}
