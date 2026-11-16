//! A raw decoder result and the exact fragment it consumed. This is a local
//! transport association, not a current Types or Verify result.
use super::*;

#[derive(Clone)]
pub(super) struct DecodedFunction {
    pub(super) function: Arc<TypedFunction>,
    // Includes the name's u32 length, relative to the function fragment.
    pub(super) name: std::ops::Range<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawFunctionDecodingStats {
    pub decode_attempts: usize,
    pub decoded_functions: usize,
    pub reused_decoded_functions: usize,
    /// A subset of reused_decoded_functions: only the diagnostic label changed.
    pub relabeled_decoded_functions: usize,
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

impl RawFunctionEncoding {
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
        if Arc::ptr_eq(&bytes, &self.bytes) || bytes.as_ref() == self.bytes.as_ref() {
            let mut encoding = self.clone();
            encoding.bytes = bytes;
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
            bytes,
            limits: limits.clone(),
            nodes: self.nodes,
            fingerprint: OnceLock::new(),
            decoded: Some(DecodedFunction {
                function: function.clone(),
                name: name_start..text_end,
            }),
        };
        Some((function, encoding))
    }
}
