//! A raw decoder result and the exact fragment it consumed. This is a local
//! transport association, not a current Types or Verify result.
use super::*;

#[derive(Clone)]
pub(super) struct DecodedFunction {
    pub(super) function: Arc<TypedFunction>,
    // Includes the name's u32 length, relative to the function fragment.
    pub(super) name: std::ops::Range<usize>,
    pub(super) dictionary: Option<(Fingerprint, Arc<[u8]>)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawFunctionDecodingStats {
    pub decode_attempts: usize,
    pub decoded_functions: usize,
    pub reused_decoded_functions: usize,
    /// A subset of reused_decoded_functions: only the diagnostic label changed.
    pub relabeled_decoded_functions: usize,
    /// Exact decoder body reuse also retained the caller's raw result projection.
    pub reused_result_projections: usize,
}

/// A bounded lookup selector for the current scalar wire profile. This neither
/// validates the fragment nor associates it with a decoded owner. Unknown
/// versions/profiles simply use the ordinary complete decoder.
pub fn raw_scalar_function_id(bytes: &[u8]) -> Option<FunctionId> {
    if WIRE_VERSION != 47 || *bytes.first()? > 1 || bytes.get(1..6)? != [0; 5] {
        return None;
    }
    Some(FunctionId(u32::from_le_bytes(
        bytes.get(6..10)?.try_into().ok()?,
    )))
}

// Writer::function and Reader::function_with_name_range put the outer ID
// immediately after these six scalar-profile bytes. Read the name with the
// actual Reader; neither a source identity nor a guessed body offset is used.
fn scalar_header(bytes: &[u8], limits: &Limits) -> Option<(FunctionId, std::ops::Range<usize>)> {
    let id = raw_scalar_function_id(bytes)?;
    let mut reader = Reader::new(bytes, limits).ok()?;
    reader.u8().ok()?;
    for _ in 0..5 {
        if reader.boolean().ok()? {
            return None;
        }
    }
    if reader.u32().ok()? != id.0 {
        return None;
    }
    let start = reader.position;
    let length = usize::try_from(reader.u32().ok()?).ok()?;
    if length > limits.max_source_bytes {
        return None;
    }
    std::str::from_utf8(reader.take(length).ok()?).ok()?;
    Some((id, start..reader.position))
}

impl RawFunctionEncoding {
    /// Split only the scalar outer identity from a raw transport fragment.
    /// All ABI, callee, body and span bytes remain in the dictionary value.
    /// This is reversible transport factoring, not a successful decode.
    pub fn scalar_dictionary_fragment(
        bytes: &[u8],
        limits: &Limits,
    ) -> Option<(FunctionId, String, Arc<[u8]>)> {
        let (id, name) = scalar_header(bytes, limits)?;
        let label = std::str::from_utf8(bytes.get(name.start + 4..name.end)?).ok()?;
        let mut body = Vec::with_capacity(bytes.len().checked_sub(label.len())?);
        body.extend_from_slice(bytes.get(..6)?);
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(bytes.get(name.end..)?);
        Some((id, label.to_owned(), body.into()))
    }

    /// Reuse the actual decoder's normalized transport bytes when republishing
    /// a restored row. No full body copy or hashing is repeated per identity.
    pub fn dictionary_fragment(&self) -> Option<(FunctionId, String, Fingerprint, Arc<[u8]>)> {
        if let Some(decoded) = &self.decoded
            && let Some((key, bytes)) = &decoded.dictionary
        {
            return Some((
                decoded.function.id,
                decoded.function.name.clone(),
                *key,
                bytes.clone(),
            ));
        }
        let (id, name, bytes) = Self::scalar_dictionary_fragment(&self.bytes, &self.limits)?;
        Some((id, name, Fingerprint::of(&bytes), bytes))
    }

    /// Instantiate a dictionary entry produced by the actual bounded Reader.
    /// Only the outer scalar ID and diagnostic name change; the complete raw
    /// body and its original logical node charge are shared by every row.
    pub fn rebind_decoded_scalar_identity(
        &self,
        id: FunctionId,
        name: &str,
        limits: &Limits,
        remaining_nodes: usize,
    ) -> Option<(Arc<TypedFunction>, Self)> {
        let decoded = self.decoded.as_ref()?;
        if !self.matches_limits(limits)
            || self.nodes > remaining_nodes.min(limits.max_nodes)
            || raw_scalar_function_id(&self.bytes)? != FunctionId(0)
            || decoded.function.id != FunctionId(0)
            || !decoded.function.name.is_empty()
            || decoded.name != (10..14)
        {
            return None;
        }
        let length = self.bytes.len().checked_add(name.len())?;
        if length > limits.max_source_bytes || u32::try_from(name.len()).is_err() {
            return None;
        }
        let mut function = decoded.function.as_ref().clone();
        function.id = id;
        function.name = name.to_owned();
        let function = Arc::new(function);
        Some((
            function.clone(),
            Self {
                bytes: raw_bytes::RawBytes::scalar(self.shared_bytes(), id, name.to_owned()),
                limits: limits.clone(),
                nodes: self.nodes,
                fingerprint: OnceLock::new(),
                decoded: Some(DecodedFunction {
                    function,
                    name: 10..14 + name.len(),
                    dictionary: Some((self.content_fingerprint(), self.shared_bytes())),
                }),
            },
        ))
    }

    /// Request-local lookup only: hash every scalar-fragment byte except its
    /// outer ID and diagnostic name. A match still requires the exact comparison
    /// and real decoded owner in reuse_decoded_scalar_header.
    pub fn scalar_header_selector(bytes: &[u8], limits: &Limits) -> Option<Fingerprint> {
        let (_, name) = scalar_header(bytes, limits)?;
        let mut state = zil_base::FingerprintBuilder::new();
        state.update(b"ZIL-RAW-SCALAR-HEADER-v47\0");
        state.update(bytes.get(..6)?);
        state.update(bytes.get(10..name.start)?);
        state.update(bytes.get(name.end..)?);
        Some(state.digest())
    }

    /// Share only this successful Reader's immutable raw body, rebinding the
    /// scalar outer ID/name to the actual current bytes. Callee IDs, contracts,
    /// ABI, spans and every other byte must remain exactly equal. This conveys
    /// no Source, Types or Verify authority and renews no aggregate allowance.
    pub fn reuse_decoded_scalar_header(
        &self,
        bytes: Arc<[u8]>,
        limits: &Limits,
        remaining_nodes: usize,
    ) -> Option<(Arc<TypedFunction>, Self)> {
        if !self.matches_limits(limits) || self.nodes > remaining_nodes.min(limits.max_nodes) {
            return None;
        }
        let decoded = self.decoded.as_ref()?;
        let (id, name) = scalar_header(&bytes, limits)?;
        let text_start = name.start.checked_add(4)?;
        if raw_scalar_function_id(&self.bytes)? != decoded.function.id
            || name.start != decoded.name.start
            || bytes.get(..6)? != self.bytes.get(..6)?
            || bytes.get(10..name.start)? != self.bytes.get(10..decoded.name.start)?
            || bytes.get(name.end..)? != self.bytes.get(decoded.name.end..)?
            || self.bytes.get(text_start..decoded.name.end)? != decoded.function.name.as_bytes()
        {
            return None;
        }
        if id == decoded.function.id {
            return self.reuse_decoded_function(bytes, limits, remaining_nodes);
        }
        let mut function = decoded.function.as_ref().clone();
        function.id = id;
        function.name = std::str::from_utf8(bytes.get(text_start..name.end)?)
            .ok()?
            .to_owned();
        let function = Arc::new(function);
        let encoding = Self {
            bytes: bytes.into(),
            limits: limits.clone(),
            nodes: self.nodes,
            fingerprint: OnceLock::new(),
            decoded: Some(DecodedFunction {
                function: function.clone(),
                name,
                dictionary: decoded.dictionary.clone(),
            }),
        };
        Some((function, encoding))
    }

    /// Bind this real decoder's bytes to a complete equal current raw value.
    /// Only its recorded diagnostic-name field may differ. The caller's
    /// original Writer still enforces ordered node/byte quotas and first errors.
    pub fn bind_decoded_owner(
        &self,
        function: Arc<TypedFunction>,
        limits: &Limits,
        maximum_bytes: usize,
    ) -> Option<Self> {
        if !self.matches_limits(limits) {
            return None;
        }
        let decoded = self.decoded.as_ref()?;
        if Arc::ptr_eq(&function, &decoded.function) {
            return (self.bytes.len() <= maximum_bytes.min(limits.max_source_bytes))
                .then(|| self.clone());
        }
        let mut comparable = function.as_ref().clone();
        comparable.name = decoded.function.name.clone();
        if comparable != *decoded.function {
            return None;
        }
        if let Some((key, template)) = &decoded.dictionary {
            let length = template.len().checked_add(function.name.len())?;
            if length > maximum_bytes.min(limits.max_source_bytes)
                || u32::try_from(function.name.len()).is_err()
            {
                return None;
            }
            let name_end = 14 + function.name.len();
            return Some(Self {
                bytes: raw_bytes::RawBytes::scalar(
                    template.clone(),
                    function.id,
                    function.name.clone(),
                ),
                limits: limits.clone(),
                nodes: self.nodes,
                fingerprint: OnceLock::new(),
                decoded: Some(DecodedFunction {
                    function,
                    name: 10..name_end,
                    dictionary: Some((*key, template.clone())),
                }),
            });
        }
        let text_start = decoded.name.start.checked_add(4)?;
        if self.bytes.get(text_start..decoded.name.end)? != decoded.function.name.as_bytes() {
            return None;
        }
        let length = self
            .bytes
            .len()
            .checked_sub(decoded.name.len())?
            .checked_add(4)?
            .checked_add(function.name.len())?;
        if length > maximum_bytes.min(limits.max_source_bytes) {
            return None;
        }
        if function.name == decoded.function.name {
            let mut encoding = self.clone();
            encoding.decoded = Some(DecodedFunction {
                function,
                name: decoded.name.clone(),
                dictionary: decoded.dictionary.clone(),
            });
            return Some(encoding);
        }
        let name_length = u32::try_from(function.name.len()).ok()?;
        let name_end = text_start.checked_add(function.name.len())?;
        let mut bytes = Vec::with_capacity(length);
        bytes.extend_from_slice(self.bytes.get(..decoded.name.start)?);
        bytes.extend_from_slice(&name_length.to_le_bytes());
        bytes.extend_from_slice(function.name.as_bytes());
        bytes.extend_from_slice(self.bytes.get(decoded.name.end..)?);
        Some(Self {
            bytes: bytes.into(),
            limits: limits.clone(),
            nodes: self.nodes,
            fingerprint: OnceLock::new(),
            decoded: Some(DecodedFunction {
                function,
                name: decoded.name.start..name_end,
                dictionary: decoded.dictionary.clone(),
            }),
        })
    }

    /// The actual immutable function produced by the same successful Reader
    /// that established these bytes, boundaries and node cost. Encoded-only
    /// fragments and FunctionPlan encodings do not carry a decoded owner.
    pub fn decoded_function(&self) -> Option<&Arc<TypedFunction>> {
        self.decoded.as_ref().map(|decoded| &decoded.function)
    }

    /// Reuse a real decoded owner only for complete equal bytes, or a change
    /// confined to its Reader-recorded diagnostic-name field. Every other raw
    /// byte, including all contracts/IDs/spans, must match. The aggregate caller
    /// still checks the complete manifest and charges its original six fees.
    pub fn reuse_decoded_function(
        &self,
        bytes: Arc<[u8]>,
        limits: &Limits,
        remaining_nodes: usize,
    ) -> Option<(Arc<TypedFunction>, Self)> {
        if !self.matches_limits(limits)
            || self.nodes > remaining_nodes.min(limits.max_nodes)
            || bytes.len() > limits.max_source_bytes
        {
            return None;
        }
        let decoded = self.decoded.as_ref()?;
        if Arc::ptr_eq(&bytes, &self.shared_bytes()) || bytes.as_ref() == self.bytes.as_ref() {
            let mut encoding = self.clone();
            encoding.bytes = bytes.into();
            return Some((decoded.function.clone(), encoding));
        }
        let name_start = decoded.name.start;
        let text_start = name_start.checked_add(4)?;
        let length =
            u32::from_le_bytes(bytes.get(name_start..text_start)?.try_into().ok()?) as usize;
        if length > limits.max_source_bytes {
            return None;
        }
        let text_end = text_start.checked_add(length)?;
        if bytes.get(..name_start)? != self.bytes.get(..name_start)?
            || bytes.get(text_end..)? != self.bytes.get(decoded.name.end..)?
            || self.bytes.get(text_start..decoded.name.end)? != decoded.function.name.as_bytes()
        {
            return None;
        }
        let name = std::str::from_utf8(bytes.get(text_start..text_end)?).ok()?;
        let mut function = decoded.function.as_ref().clone();
        function.name = name.to_owned();
        let function = Arc::new(function);
        // Reader::text charges no nodes. Exact unchanged prefix/suffix plus
        // the bounded UTF-8 length establish the same complete function under
        // the same limits; only small headers change, body owners stay shared.
        let encoding = Self {
            bytes: bytes.into(),
            limits: limits.clone(),
            nodes: self.nodes,
            fingerprint: OnceLock::new(),
            decoded: Some(DecodedFunction {
                function: function.clone(),
                name: name_start..text_end,
                dictionary: decoded.dictionary.clone(),
            }),
        };
        Some((function, encoding))
    }
}
