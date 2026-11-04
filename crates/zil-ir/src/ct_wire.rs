//! Bounded, canonical CT-only wire. Field codecs below use explicit
//! schema-order tags; no Debug/serde layout or runtime Wire31 dependency.
use crate::ct_artifact::*;
use crate::{BinaryOp, Constant, StorageLifeContract, UnaryOp};
#[path = "ct_wire_scopes.rs"]
mod scopes;
use zil_base::{FileId, Fingerprint, FingerprintBuilder, Limits, ModuleId, NodeId, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    Limit,
    Truncated,
    UnknownTag,
    BadVersion,
    BadFingerprint,
    Trailing,
    InvalidUtf8,
    NonCanonical,
}
pub type WireResult<T> = std::result::Result<T, WireError>;
type Result<T> = WireResult<T>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireBudget {
    pub bytes: usize,
    pub nodes: usize,
    pub work: usize,
    pub allocation: usize,
    pub max_depth: usize,
}
impl WireBudget {
    pub fn from_limits(limits: &Limits) -> Self {
        Self {
            bytes: limits.max_source_bytes,
            nodes: limits.max_nodes,
            work: limits.max_analysis_steps,
            allocation: limits.max_source_bytes,
            max_depth: limits.max_call_depth,
        }
    }
    fn intersect(self, other: Self) -> Self {
        Self {
            bytes: self.bytes.min(other.bytes),
            nodes: self.nodes.min(other.nodes),
            work: self.work.min(other.work),
            allocation: self.allocation.min(other.allocation),
            max_depth: self.max_depth.min(other.max_depth),
        }
    }
    fn consume(&mut self, usage: WireUsage) {
        self.bytes -= usage.bytes;
        self.nodes -= usage.nodes;
        self.work -= usage.work;
        self.allocation -= usage.allocation;
    }
    fn attempt(&mut self) -> Result<()> {
        self.work = self.work.checked_sub(1).ok_or(WireError::Limit)?;
        Ok(())
    }
}
fn begin_attempt(limits: &Limits, remaining: &mut WireBudget) -> Result<WireBudget> {
    let mut effective = WireBudget::from_limits(limits).intersect(*remaining);
    if effective.work == 0 {
        return Err(WireError::Limit);
    }
    remaining.attempt()?;
    effective.work -= 1;
    Ok(effective)
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireUsage {
    pub bytes: usize,
    pub nodes: usize,
    pub work: usize,
    pub allocation: usize,
    pub max_depth: usize,
    pub growths: usize,
}

trait Wire: Sized {
    fn encode(&self, e: &mut Encoder) -> Result<()>;
    fn decode(d: &mut Decoder<'_>) -> Result<Self>;
}
struct Encoder {
    out: Vec<u8>,
    bytes_left: usize,
    nodes_left: usize,
    work_left: usize,
    allocation_left: usize,
    charged_capacity: usize,
    depth: usize,
    max_depth: usize,
    depth_peak: usize,
    growths: usize,
}
impl Encoder {
    fn new(budget: WireBudget) -> Self {
        Self {
            out: Vec::new(),
            bytes_left: budget.bytes,
            nodes_left: budget.nodes,
            work_left: budget.work,
            allocation_left: budget.allocation,
            charged_capacity: 0,
            depth: 0,
            max_depth: budget.max_depth,
            depth_peak: 0,
            growths: 0,
        }
    }
    fn usage(&self, initial: WireBudget) -> WireUsage {
        WireUsage {
            bytes: initial.bytes - self.bytes_left,
            nodes: initial.nodes - self.nodes_left,
            work: initial.work - self.work_left,
            allocation: initial.allocation - self.allocation_left,
            max_depth: self.depth_peak,
            growths: self.growths,
        }
    }
    fn work(&mut self, n: usize) -> Result<()> {
        self.work_left = self.work_left.checked_sub(n).ok_or(WireError::Limit)?;
        Ok(())
    }
    fn nodes(&mut self, n: usize) -> Result<()> {
        if n > self.nodes_left || n > self.work_left {
            return Err(WireError::Limit);
        }
        self.nodes_left -= n;
        self.work_left -= n;
        Ok(())
    }
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let needed = self
            .out
            .len()
            .checked_add(bytes.len())
            .ok_or(WireError::Limit)?;
        if bytes.len() > self.bytes_left {
            return Err(WireError::Limit);
        }
        let old_capacity = self.charged_capacity;
        if needed > old_capacity {
            let ceiling = old_capacity
                .checked_add(self.allocation_left)
                .ok_or(WireError::Limit)?;
            if needed > ceiling {
                return Err(WireError::Limit);
            }
            let target = old_capacity.saturating_mul(2).max(needed).min(ceiling);
            let growth = target - old_capacity;
            let work = bytes
                .len()
                .checked_add(self.out.len())
                .ok_or(WireError::Limit)?;
            if work > self.work_left {
                return Err(WireError::Limit);
            }
            self.work(self.out.len())?;
            self.allocation_left -= growth;
            if self.out.try_reserve_exact(target - self.out.len()).is_err() {
                self.allocation_left += growth;
                self.work_left += self.out.len();
                return Err(WireError::Limit);
            }
            self.charged_capacity = target;
            self.growths += 1;
            // The allocator may supply spare capacity, but admission uses
            // charged_capacity so those bytes are never free to the caller.
        }
        if bytes.len() > self.work_left {
            return Err(WireError::Limit);
        }
        self.work(bytes.len())?;
        self.bytes_left -= bytes.len();
        self.out.extend_from_slice(bytes);
        Ok(())
    }
    fn descend<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.enter()?;
        let answer = f(self);
        self.depth -= 1;
        answer
    }
    fn enter(&mut self) -> Result<()> {
        if self.depth >= self.max_depth {
            return Err(WireError::Limit);
        }
        self.nodes(1)?;
        self.depth += 1;
        self.depth_peak = self.depth_peak.max(self.depth);
        Ok(())
    }
}
struct Decoder<'a> {
    bytes: &'a [u8],
    at: usize,
    bytes_left: usize,
    nodes_left: usize,
    allocation_left: usize,
    work_left: usize,
    depth: usize,
    max_depth: usize,
    depth_peak: usize,
    hashed_bytes: bool,
}
impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8], budget: WireBudget) -> Result<Self> {
        if bytes.len() > budget.bytes {
            return Err(WireError::Limit);
        }
        Ok(Self {
            bytes,
            at: 0,
            bytes_left: budget.bytes,
            nodes_left: budget.nodes,
            allocation_left: budget.allocation,
            work_left: budget.work,
            depth: 0,
            max_depth: budget.max_depth,
            depth_peak: 0,
            hashed_bytes: false,
        })
    }
    fn usage(&self, initial: WireBudget) -> WireUsage {
        WireUsage {
            bytes: initial.bytes - self.bytes_left,
            nodes: initial.nodes - self.nodes_left,
            work: initial.work - self.work_left,
            allocation: initial.allocation - self.allocation_left,
            max_depth: self.depth_peak,
            growths: 0,
        }
    }
    fn nodes(&mut self, n: usize) -> Result<()> {
        if n > self.nodes_left || n > self.work_left {
            return Err(WireError::Limit);
        }
        self.nodes_left -= n;
        self.work_left -= n;
        Ok(())
    }
    fn allocation(&mut self, n: usize) -> Result<()> {
        if n > self.allocation_left || n > self.work_left {
            return Err(WireError::Limit);
        }
        self.allocation_left -= n;
        self.work_left -= n;
        Ok(())
    }
    fn read(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(WireError::Limit)?;
        let bytes = self.bytes.get(self.at..end).ok_or(WireError::Truncated)?;
        if (!self.hashed_bytes && n > self.bytes_left) || n > self.work_left {
            return Err(WireError::Limit);
        }
        if !self.hashed_bytes {
            self.bytes_left -= n;
        }
        self.work_left -= n;
        self.at = end;
        Ok(bytes)
    }
    fn descend<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.enter()?;
        let answer = f(self);
        self.depth -= 1;
        answer
    }
    fn enter(&mut self) -> Result<()> {
        if self.depth >= self.max_depth {
            return Err(WireError::Limit);
        }
        self.nodes(1)?;
        self.depth += 1;
        self.depth_peak = self.depth_peak.max(self.depth);
        Ok(())
    }
    fn finish(&self) -> Result<()> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(WireError::Trailing)
        }
    }
}

/// Bounded continuation storage for iterative recursive codecs. Logical
/// capacity, rather than allocator spare capacity, controls every push.
struct Scratch<T> {
    items: Vec<T>,
    charged_capacity: usize,
}
impl<T> Scratch<T> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            charged_capacity: 0,
        }
    }
    fn growth(&self, available: usize) -> Result<(usize, usize, usize)> {
        let needed = self.items.len().checked_add(1).ok_or(WireError::Limit)?;
        if needed <= self.charged_capacity {
            return Ok((self.charged_capacity, 0, 0));
        }
        let size = std::mem::size_of::<T>().max(1);
        let ceiling = self
            .charged_capacity
            .checked_add(available / size)
            .ok_or(WireError::Limit)?;
        if needed > ceiling {
            return Err(WireError::Limit);
        }
        let target = self
            .charged_capacity
            .saturating_mul(2)
            .max(needed)
            .min(ceiling);
        let allocation = (target - self.charged_capacity)
            .checked_mul(size)
            .ok_or(WireError::Limit)?;
        let copying = self.items.len().checked_mul(size).ok_or(WireError::Limit)?;
        Ok((target, allocation, copying))
    }
    fn push_encode(&mut self, e: &mut Encoder, value: T) -> Result<()> {
        let (target, allocation, copying) = self.growth(e.allocation_left)?;
        if allocation != 0 {
            let work = allocation.checked_add(copying).ok_or(WireError::Limit)?;
            if work > e.work_left {
                return Err(WireError::Limit);
            }
            e.work_left -= work;
            e.allocation_left -= allocation;
            if self
                .items
                .try_reserve_exact(target - self.items.len())
                .is_err()
            {
                e.work_left += work;
                e.allocation_left += allocation;
                return Err(WireError::Limit);
            }
            self.charged_capacity = target;
        }
        self.items.push(value);
        Ok(())
    }
    fn push_decode(&mut self, d: &mut Decoder<'_>, value: T) -> Result<()> {
        let (target, allocation, copying) = self.growth(d.allocation_left)?;
        if allocation != 0 {
            let work = allocation.checked_add(copying).ok_or(WireError::Limit)?;
            if work > d.work_left {
                return Err(WireError::Limit);
            }
            d.work_left -= work;
            d.allocation_left -= allocation;
            if self
                .items
                .try_reserve_exact(target - self.items.len())
                .is_err()
            {
                d.work_left += work;
                d.allocation_left += allocation;
                return Err(WireError::Limit);
            }
            self.charged_capacity = target;
        }
        self.items.push(value);
        Ok(())
    }
    fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }
    fn last(&self) -> Option<&T> {
        self.items.last()
    }
    fn last_mut(&mut self) -> Option<&mut T> {
        self.items.last_mut()
    }
    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl Wire for u8 {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.write(&[*self])
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(d.read(1)?[0])
    }
}
macro_rules! integer_wire { ($($t:ty),+ $(,)?) => {$(
    impl Wire for $t { fn encode(&self,e:&mut Encoder)->Result<()> { e.write(&self.to_le_bytes()) }
        fn decode(d:&mut Decoder<'_>)->Result<Self> { let bytes=d.read(std::mem::size_of::<Self>())?;
            Ok(Self::from_le_bytes(bytes.try_into().map_err(|_| WireError::Truncated)?)) } }
)+}; }
integer_wire!(u16, u32, u64, i32, i64);
impl Wire for bool {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        (*self as u8).encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        match u8::decode(d)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError::NonCanonical),
        }
    }
}
impl<T: Wire> Wire for Vec<T> {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        let n = u32::try_from(self.len()).map_err(|_| WireError::Limit)?;
        if self.len() > e.nodes_left {
            return Err(WireError::Limit);
        }
        n.encode(e)?;
        for value in self {
            e.nodes(1)?;
            value.encode(e)?;
        }
        Ok(())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        let n = u32::decode(d)? as usize;
        if n > d.nodes_left {
            return Err(WireError::Limit);
        }
        let allocation = n
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(WireError::Limit)?;
        d.allocation(allocation)?;
        let mut out = Vec::new();
        if out.try_reserve_exact(n).is_err() {
            d.allocation_left += allocation;
            d.work_left += allocation;
            return Err(WireError::Limit);
        }
        for _ in 0..n {
            d.nodes(1)?;
            out.push(T::decode(d)?);
        }
        Ok(out)
    }
}
impl<T: Wire> Wire for Option<T> {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        match self {
            None => 0u8.encode(e),
            Some(v) => {
                1u8.encode(e)?;
                v.encode(e)
            }
        }
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        match u8::decode(d)? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(d)?)),
            _ => Err(WireError::NonCanonical),
        }
    }
}
impl<T: Wire> Wire for Box<T> {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.as_ref().encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        let value = T::decode(d)?;
        d.allocation(std::mem::size_of::<T>())?;
        Ok(Box::new(value))
    }
}
impl Wire for String {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        let n = u32::try_from(self.len()).map_err(|_| WireError::Limit)?;
        n.encode(e)?;
        e.write(self.as_bytes())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        let n = u32::decode(d)? as usize;
        let bytes = d.read(n)?;
        let text = std::str::from_utf8(bytes).map_err(|_| WireError::InvalidUtf8)?;
        d.allocation(n)?;
        let mut out = String::new();
        if out.try_reserve_exact(n).is_err() {
            d.allocation_left += n;
            d.work_left += n;
            return Err(WireError::Limit);
        }
        out.push_str(text);
        Ok(out)
    }
}
impl Wire for Fingerprint {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.write(&self.0)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(
            d.read(32)?.try_into().map_err(|_| WireError::Truncated)?,
        ))
    }
}
macro_rules! base_id { ($($t:ident),+ $(,)?) => {$( impl Wire for $t {
    fn encode(&self,e:&mut Encoder)->Result<()> { self.0.encode(e) }
    fn decode(d:&mut Decoder<'_>)->Result<Self> { Ok(Self(u32::decode(d)?)) }
} )+}; }
base_id!(FileId, ModuleId, NodeId, CtAmbientScopeId);
impl Wire for Span {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.file.encode(e)?;
        if self.start > self.end {
            return Err(WireError::NonCanonical);
        }
        u64::try_from(self.start)
            .map_err(|_| WireError::Limit)?
            .encode(e)?;
        u64::try_from(self.end)
            .map_err(|_| WireError::Limit)?
            .encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        let file = FileId::decode(d)?;
        let start = usize::try_from(u64::decode(d)?).map_err(|_| WireError::Limit)?;
        let end = usize::try_from(u64::decode(d)?).map_err(|_| WireError::Limit)?;
        if start > end {
            return Err(WireError::NonCanonical);
        }
        Ok(Self { file, start, end })
    }
}

const PROGRAM_MAGIC: &[u8; 8] = b"ZILCTP01";
const VALUE_MAGIC: &[u8; 8] = b"ZILCTV01";
const REGISTRY_MAGIC: &[u8; 8] = b"ZILCTR01";
const FUNCTION_MAGIC: &[u8; 8] = b"ZILCTF01";
const VALUE_DOMAIN: &[u8] = b"zil/ct-value/v5";
const FUNCTION_DOMAIN: &[u8] = b"zil/ct-function/v5";
fn fingerprint(domain: &[u8], body: &[u8]) -> Fingerprint {
    let mut hash = FingerprintBuilder::new();
    hash.update(domain);
    hash.update(body);
    hash.digest()
}
fn encode_enveloped<T: Wire>(
    value: &T,
    magic: &[u8; 8],
    domain: &[u8],
    limits: &Limits,
    remaining: &mut WireBudget,
) -> Result<(Vec<u8>, WireUsage)> {
    let effective = begin_attempt(limits, remaining)?;
    let mut e = Encoder::new(effective);
    let encoded = (|| {
        e.write(magic)?;
        CT_ARTIFACT_SCHEMA_VERSION.encode(&mut e)?;
        value.encode(&mut e)?;
        e.work(
            e.out
                .len()
                .checked_add(domain.len())
                .ok_or(WireError::Limit)?,
        )?;
        let digest = fingerprint(domain, &e.out);
        e.write(&digest.0)?;
        Ok(())
    })();
    let mut usage = e.usage(effective);
    remaining.consume(usage);
    encoded?;
    usage.work += 1;
    Ok((e.out, usage))
}
fn decode_enveloped<T: Wire>(
    bytes: &[u8],
    magic: &[u8; 8],
    domain: &[u8],
    limits: &Limits,
    remaining: &mut WireBudget,
) -> Result<(T, WireUsage)> {
    let effective = begin_attempt(limits, remaining)?;
    if bytes.len() < 44 {
        return Err(WireError::Truncated);
    }
    if bytes.len() > effective.bytes {
        return Err(WireError::Limit);
    }
    let (body, trailer) = bytes.split_at(bytes.len() - 32);
    let mut d = Decoder::new(body, effective)?;
    let decoded = (|| {
        let hash_work = body
            .len()
            .checked_add(32)
            .and_then(|n| n.checked_add(domain.len()))
            .ok_or(WireError::Limit)?;
        if bytes.len() > d.bytes_left || hash_work > d.work_left {
            return Err(WireError::Limit);
        }
        d.bytes_left -= bytes.len();
        d.hashed_bytes = true;
        d.work_left -= hash_work;
        if fingerprint(domain, body).0 != trailer {
            return Err(WireError::BadFingerprint);
        }
        if d.read(8)? != magic {
            return Err(WireError::BadVersion);
        }
        if u32::decode(&mut d)? != CT_ARTIFACT_SCHEMA_VERSION {
            return Err(WireError::BadVersion);
        }
        let value = T::decode(&mut d)?;
        d.finish()?;
        Ok(value)
    })();
    let mut usage = d.usage(effective);
    remaining.consume(usage);
    let value = decoded?;
    usage.work += 1;
    Ok((value, usage))
}
pub fn encode_program_with_usage(
    value: &CtArtifactProgram,
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(Vec<u8>, WireUsage)> {
    if value.schema_version != CT_ARTIFACT_SCHEMA_VERSION {
        begin_attempt(limits, remaining)?;
        return Err(WireError::BadVersion);
    }
    encode_enveloped(
        value,
        PROGRAM_MAGIC,
        CT_ARTIFACT_FINGERPRINT_DOMAIN,
        limits,
        remaining,
    )
}
pub fn encode_program(value: &CtArtifactProgram, limits: &Limits) -> WireResult<Vec<u8>> {
    Ok(encode_program_with_usage(value, limits, &mut WireBudget::from_limits(limits))?.0)
}
pub fn decode_program_with_usage(
    bytes: &[u8],
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(CtArtifactProgram, WireUsage)> {
    let (value, usage): (CtArtifactProgram, WireUsage) = decode_enveloped(
        bytes,
        PROGRAM_MAGIC,
        CT_ARTIFACT_FINGERPRINT_DOMAIN,
        limits,
        remaining,
    )?;
    if value.schema_version != CT_ARTIFACT_SCHEMA_VERSION {
        return Err(WireError::BadVersion);
    }
    Ok((value, usage))
}
pub fn decode_program(bytes: &[u8], limits: &Limits) -> WireResult<CtArtifactProgram> {
    Ok(decode_program_with_usage(bytes, limits, &mut WireBudget::from_limits(limits))?.0)
}
pub fn encode_value_with_usage(
    value: &CtValue,
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(Vec<u8>, WireUsage)> {
    encode_enveloped(value, VALUE_MAGIC, VALUE_DOMAIN, limits, remaining)
}
pub fn decode_value_with_usage(
    bytes: &[u8],
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(CtValue, WireUsage)> {
    decode_enveloped(bytes, VALUE_MAGIC, VALUE_DOMAIN, limits, remaining)
}
pub fn encode_value(value: &CtValue, limits: &Limits) -> WireResult<Vec<u8>> {
    Ok(encode_value_with_usage(value, limits, &mut WireBudget::from_limits(limits))?.0)
}
pub fn decode_value(bytes: &[u8], limits: &Limits) -> WireResult<CtValue> {
    Ok(decode_value_with_usage(bytes, limits, &mut WireBudget::from_limits(limits))?.0)
}
pub fn encode_registry_with_usage(
    value: &CtLockedProviderRegistry,
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(Vec<u8>, WireUsage)> {
    if value.schema_version != CT_ARTIFACT_SCHEMA_VERSION {
        begin_attempt(limits, remaining)?;
        return Err(WireError::BadVersion);
    }
    encode_enveloped(
        value,
        REGISTRY_MAGIC,
        CT_PROVIDER_FINGERPRINT_DOMAIN,
        limits,
        remaining,
    )
}
pub fn decode_registry_with_usage(
    bytes: &[u8],
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(CtLockedProviderRegistry, WireUsage)> {
    let (value, usage): (CtLockedProviderRegistry, WireUsage) = decode_enveloped(
        bytes,
        REGISTRY_MAGIC,
        CT_PROVIDER_FINGERPRINT_DOMAIN,
        limits,
        remaining,
    )?;
    if value.schema_version != CT_ARTIFACT_SCHEMA_VERSION {
        return Err(WireError::BadVersion);
    }
    Ok((value, usage))
}
pub fn encode_registry(value: &CtLockedProviderRegistry, limits: &Limits) -> WireResult<Vec<u8>> {
    Ok(encode_registry_with_usage(value, limits, &mut WireBudget::from_limits(limits))?.0)
}
pub fn decode_registry(bytes: &[u8], limits: &Limits) -> WireResult<CtLockedProviderRegistry> {
    Ok(decode_registry_with_usage(bytes, limits, &mut WireBudget::from_limits(limits))?.0)
}
pub fn program_fingerprint_with_usage(
    value: &CtArtifactProgram,
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(Fingerprint, WireUsage)> {
    let (bytes, usage) = encode_program_with_usage(value, limits, remaining)?;
    Ok((
        Fingerprint(
            bytes[bytes.len() - 32..]
                .try_into()
                .map_err(|_| WireError::Truncated)?,
        ),
        usage,
    ))
}
pub fn program_fingerprint(value: &CtArtifactProgram, limits: &Limits) -> WireResult<Fingerprint> {
    Ok(program_fingerprint_with_usage(value, limits, &mut WireBudget::from_limits(limits))?.0)
}
pub fn function_fingerprint_with_usage(
    value: &CtFunction,
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(Fingerprint, WireUsage)> {
    let (bytes, usage) =
        encode_enveloped(value, FUNCTION_MAGIC, FUNCTION_DOMAIN, limits, remaining)?;
    Ok((
        Fingerprint(
            bytes[bytes.len() - 32..]
                .try_into()
                .map_err(|_| WireError::Truncated)?,
        ),
        usage,
    ))
}
pub fn function_fingerprint(value: &CtFunction, limits: &Limits) -> WireResult<Fingerprint> {
    Ok(function_fingerprint_with_usage(value, limits, &mut WireBudget::from_limits(limits))?.0)
}

/// Canonical structural transport identity, charged to the remaining request
/// budget. Schema/symbol/ambient references remain raw indices here; this is
/// not a contextual semantic key for values that depend on those references.
pub fn value_fingerprint_with_usage(
    value: &CtValue,
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(Fingerprint, WireUsage)> {
    let (bytes, usage) = encode_value_with_usage(value, limits, remaining)?;
    Ok((
        Fingerprint(
            bytes[bytes.len() - 32..]
                .try_into()
                .map_err(|_| WireError::Truncated)?,
        ),
        usage,
    ))
}

/// Hash the borrowed token tree without first cloning it into a CtValue.
pub fn token_tree_fingerprint_with_usage(
    value: &CtTokenTree,
    limits: &Limits,
    remaining: &mut WireBudget,
) -> WireResult<(Fingerprint, WireUsage)> {
    let (bytes, usage) = encode_enveloped(
        value,
        b"ZILCTT01",
        b"zil/ct-token-tree/v1",
        limits,
        remaining,
    )?;
    Ok((
        Fingerprint(
            bytes[bytes.len() - 32..]
                .try_into()
                .map_err(|_| WireError::Truncated)?,
        ),
        usage,
    ))
}

impl Wire for CtFunctionId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtBlockId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtLocalId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtValueId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtConstantId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtTemplateId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtSyntaxNodeId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtTokenNodeId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtOriginId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtInputTokenId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtRecordSchemaId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtEnumSchemaId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for CtTypeNodeId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl Wire for Constant {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Unit => {
                    0u8.encode(e)?;
                }
                Self::Bool(v0) => {
                    1u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::I32(v0) => {
                    2u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::I64(v0) => {
                    3u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::U64(v0) => {
                    9u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Usize(v0) => {
                    10u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::F32(bits) => {
                    11u8.encode(e)?;
                    bits.encode(e)?;
                }
                Self::F64(bits) => {
                    12u8.encode(e)?;
                    bits.encode(e)?;
                }
                Self::U8(v0) => {
                    4u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::U16(v0) => {
                    5u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::NullHandle => {
                    6u8.encode(e)?;
                }
                Self::NullWideString => {
                    7u8.encode(e)?;
                }
                Self::WideString(v0) => {
                    8u8.encode(e)?;
                    v0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Unit),
            1 => Ok(Self::Bool(<bool as Wire>::decode(d)?)),
            2 => Ok(Self::I32(<i32 as Wire>::decode(d)?)),
            3 => Ok(Self::I64(<i64 as Wire>::decode(d)?)),
            9 => Ok(Self::U64(<u64 as Wire>::decode(d)?)),
            10 => Ok(Self::Usize(<u64 as Wire>::decode(d)?)),
            11 => Ok(Self::F32(<u32 as Wire>::decode(d)?)),
            12 => Ok(Self::F64(<u64 as Wire>::decode(d)?)),
            4 => Ok(Self::U8(<u8 as Wire>::decode(d)?)),
            5 => Ok(Self::U16(<u16 as Wire>::decode(d)?)),
            6 => Ok(Self::NullHandle),
            7 => Ok(Self::NullWideString),
            8 => Ok(Self::WideString(<Vec<u16> as Wire>::decode(d)?)),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for UnaryOp {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Negate => {
                    0u8.encode(e)?;
                }
                Self::Not => {
                    1u8.encode(e)?;
                }
                Self::BitNot => {
                    2u8.encode(e)?;
                }
                Self::Dereference => {
                    3u8.encode(e)?;
                }
                Self::Borrow => {
                    4u8.encode(e)?;
                }
                Self::BorrowMut => {
                    5u8.encode(e)?;
                }
                Self::Give => {
                    6u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Negate),
            1 => Ok(Self::Not),
            2 => Ok(Self::BitNot),
            3 => Ok(Self::Dereference),
            4 => Ok(Self::Borrow),
            5 => Ok(Self::BorrowMut),
            6 => Ok(Self::Give),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for BinaryOp {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Add => {
                    0u8.encode(e)?;
                }
                Self::Subtract => {
                    1u8.encode(e)?;
                }
                Self::Multiply => {
                    2u8.encode(e)?;
                }
                Self::Divide => {
                    3u8.encode(e)?;
                }
                Self::Remainder => {
                    4u8.encode(e)?;
                }
                Self::AddWrapping => {
                    5u8.encode(e)?;
                }
                Self::SubtractWrapping => {
                    6u8.encode(e)?;
                }
                Self::MultiplyWrapping => {
                    7u8.encode(e)?;
                }
                Self::Equal => {
                    8u8.encode(e)?;
                }
                Self::NotEqual => {
                    9u8.encode(e)?;
                }
                Self::Less => {
                    10u8.encode(e)?;
                }
                Self::LessEqual => {
                    11u8.encode(e)?;
                }
                Self::Greater => {
                    12u8.encode(e)?;
                }
                Self::GreaterEqual => {
                    13u8.encode(e)?;
                }
                Self::And => {
                    14u8.encode(e)?;
                }
                Self::Or => {
                    15u8.encode(e)?;
                }
                Self::BitAnd => {
                    16u8.encode(e)?;
                }
                Self::BitOr => {
                    17u8.encode(e)?;
                }
                Self::BitXor => {
                    18u8.encode(e)?;
                }
                Self::ShiftLeft => {
                    19u8.encode(e)?;
                }
                Self::ShiftRight => {
                    20u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Add),
            1 => Ok(Self::Subtract),
            2 => Ok(Self::Multiply),
            3 => Ok(Self::Divide),
            4 => Ok(Self::Remainder),
            5 => Ok(Self::AddWrapping),
            6 => Ok(Self::SubtractWrapping),
            7 => Ok(Self::MultiplyWrapping),
            8 => Ok(Self::Equal),
            9 => Ok(Self::NotEqual),
            10 => Ok(Self::Less),
            11 => Ok(Self::LessEqual),
            12 => Ok(Self::Greater),
            13 => Ok(Self::GreaterEqual),
            14 => Ok(Self::And),
            15 => Ok(Self::Or),
            16 => Ok(Self::BitAnd),
            17 => Ok(Self::BitOr),
            18 => Ok(Self::BitXor),
            19 => Ok(Self::ShiftLeft),
            20 => Ok(Self::ShiftRight),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtSymbolId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.module.encode(e)?;
            self.declaration.encode(e)?;
            self.generation_path.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                module: <ModuleId as Wire>::decode(d)?,
                declaration: <NodeId as Wire>::decode(d)?,
                generation_path: <Option<Fingerprint> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtScalarTy {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Unit => {
                    0u8.encode(e)?;
                }
                Self::Bool => {
                    1u8.encode(e)?;
                }
                Self::U8 => {
                    2u8.encode(e)?;
                }
                Self::U16 => {
                    3u8.encode(e)?;
                }
                Self::I32 => {
                    4u8.encode(e)?;
                }
                Self::I64 => {
                    5u8.encode(e)?;
                }
                Self::U64 => {
                    6u8.encode(e)?;
                }
                Self::Usize => {
                    7u8.encode(e)?;
                }
                Self::F32 => {
                    8u8.encode(e)?;
                }
                Self::F64 => {
                    9u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Unit),
            1 => Ok(Self::Bool),
            2 => Ok(Self::U8),
            3 => Ok(Self::U16),
            4 => Ok(Self::I32),
            5 => Ok(Self::I64),
            6 => Ok(Self::U64),
            7 => Ok(Self::Usize),
            8 => Ok(Self::F32),
            9 => Ok(Self::F64),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtSyntaxCategory {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Expr => {
                    0u8.encode(e)?;
                }
                Self::Type => {
                    1u8.encode(e)?;
                }
                Self::StmtList => {
                    2u8.encode(e)?;
                }
                Self::ItemList => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Expr),
            1 => Ok(Self::Type),
            2 => Ok(Self::StmtList),
            3 => Ok(Self::ItemList),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for crate::StorageBinder {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        match self {
            Self::Fixed(i) => {
                0u8.encode(e)?;
                i.encode(e)
            }
            Self::Invocation(i) => {
                1u8.encode(e)?;
                i.encode(e)
            }
        }
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        match u8::decode(d)? {
            0 => Ok(Self::Fixed(u32::decode(d)?)),
            1 => Ok(Self::Invocation(u32::decode(d)?)),
            _ => Err(WireError::UnknownTag),
        }
    }
}
impl Wire for CtLifetimeArgument {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.owner.encode(e)?;
            self.binder.encode(e)
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                owner: CtTypeNodeId::decode(d)?,
                binder: crate::StorageBinder::decode(d)?,
            })
        })
    }
}

impl Wire for CtSchemaArgument {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| match self {
            Self::Type(ty) => {
                0u8.encode(e)?;
                ty.encode(e)
            }
            Self::Lifetime(value) => {
                2u8.encode(e)?;
                value.encode(e)
            }
            Self::TypeInContext(value) => {
                3u8.encode(e)?;
                value.encode(e)
            }
            Self::Scalar(value) => {
                1u8.encode(e)?;
                value.encode(e)
            }
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Type(CtTypeRef::decode(d)?)),
            1 => Ok(Self::Scalar(Constant::decode(d)?)),
            2 => Ok(Self::Lifetime(CtLifetimeArgument::decode(d)?)),
            3 => Ok(Self::TypeInContext(CtTypeNodeId::decode(d)?)),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtRecordSchema {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.name.encode(e)?;
            self.life_context.encode(e)?;
            self.arguments.encode(e)?;
            self.fields.encode(e)?;
            self.origin.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                name: <CtSymbolId as Wire>::decode(d)?,
                life_context: Option::<CtTypeRef>::decode(d)?,
                arguments: <Vec<CtSchemaArgument> as Wire>::decode(d)?,
                fields: <Vec<CtTy> as Wire>::decode(d)?,
                origin: <CtSchemaOrigin as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtEnumSchema {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.name.encode(e)?;
            self.life_context.encode(e)?;
            self.arguments.encode(e)?;
            self.variants.encode(e)?;
            self.origin.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                name: <CtSymbolId as Wire>::decode(d)?,
                life_context: Option::<CtTypeRef>::decode(d)?,
                arguments: <Vec<CtSchemaArgument> as Wire>::decode(d)?,
                variants: <Vec<Vec<CtTy>> as Wire>::decode(d)?,
                origin: <CtSchemaOrigin as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtSchemaOrigin {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::UserPure => {
                    0u8.encode(e)?;
                }
                Self::BuiltinQuery => {
                    1u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::UserPure),
            1 => Ok(Self::BuiltinQuery),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtBuiltinType {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::ByteBuffer => {
                    0u8.encode(e)?;
                }
                Self::ForeignHandle => {
                    1u8.encode(e)?;
                }
                Self::CWideString => {
                    2u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::ByteBuffer),
            1 => Ok(Self::ForeignHandle),
            2 => Ok(Self::CWideString),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtTypeRef {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.nodes.encode(e)?;
            self.root.encode(e)?;
            self.ambient_classes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                nodes: <Vec<CtTypeNode> as Wire>::decode(d)?,
                root: <CtTypeNodeId as Wire>::decode(d)?,
                ambient_classes: Vec::<CtAmbientClassBinding>::decode(d)?,
            })
        })
    }
}

impl Wire for crate::VariantProjection {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.variant.encode(e)?;
        self.index.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            variant: u32::decode(d)?,
            index: u32::decode(d)?,
        })
    }
}
impl Wire for crate::PlaceProjection {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Field(i) => {
                    0u8.encode(e)?;
                    i.encode(e)?
                }
                Self::Variant(v) => {
                    1u8.encode(e)?;
                    v.encode(e)?
                }
                Self::ErrorArm(b) => {
                    2u8.encode(e)?;
                    b.encode(e)?
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Field(u32::decode(d)?)),
            1 => Ok(Self::Variant(crate::VariantProjection::decode(d)?)),
            2 => Ok(Self::ErrorArm(bool::decode(d)?)),
            _ => Err(WireError::UnknownTag),
        })
    }
}
impl Wire for crate::StoredBorrowMode {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        (*self as u8).encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        match u8::decode(d)? {
            0 => Ok(Self::SharedAddress),
            1 => Ok(Self::ExclusiveAddress),
            2 => Ok(Self::ReadOnlyDescription),
            3 => Ok(Self::WritableDescription),
            _ => Err(WireError::UnknownTag),
        }
    }
}
impl Wire for crate::StorageLifeSlot {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            if self.path.len() > e.max_depth {
                return Err(WireError::Limit);
            }
            self.path.encode(e)?;
            self.binder.encode(e)?;
            self.mode.encode(e)
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            let count = u32::decode(d)? as usize;
            if count > d.max_depth || count > d.nodes_left {
                return Err(WireError::Limit);
            }
            let allocation = count
                .checked_mul(std::mem::size_of::<crate::PlaceProjection>())
                .ok_or(WireError::Limit)?;
            d.allocation(allocation)?;
            let mut path = Vec::new();
            path.try_reserve_exact(count)
                .map_err(|_| WireError::Limit)?;
            for _ in 0..count {
                d.nodes(1)?;
                path.push(crate::PlaceProjection::decode(d)?);
            }
            Ok(Self {
                path,
                binder: u32::decode(d)?,
                mode: crate::StoredBorrowMode::decode(d)?,
            })
        })
    }
}
impl Wire for crate::LifeTypeProjection {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Place(p) => {
                    0u8.encode(e)?;
                    p.encode(e)?;
                }
                Self::TypeArgument(i) => {
                    1u8.encode(e)?;
                    i.encode(e)?;
                }
                Self::Pointee => 2u8.encode(e)?,
                Self::Element => 3u8.encode(e)?,
                Self::Parameter(i) => {
                    4u8.encode(e)?;
                    i.encode(e)?;
                }
                Self::Result => 5u8.encode(e)?,
                Self::Port(i) => {
                    6u8.encode(e)?;
                    i.encode(e)?;
                }
                Self::AssociatedType(i) => {
                    7u8.encode(e)?;
                    i.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Place(crate::PlaceProjection::decode(d)?)),
            1 => Ok(Self::TypeArgument(u32::decode(d)?)),
            2 => Ok(Self::Pointee),
            3 => Ok(Self::Element),
            4 => Ok(Self::Parameter(u32::decode(d)?)),
            5 => Ok(Self::Result),
            6 => Ok(Self::Port(u32::decode(d)?)),
            7 => Ok(Self::AssociatedType(u32::decode(d)?)),
            _ => Err(WireError::UnknownTag),
        })
    }
}
fn encode_life_type_path(path: &[crate::LifeTypeProjection], e: &mut Encoder) -> Result<()> {
    if path.len() > e.max_depth || path.len() > e.nodes_left {
        return Err(WireError::Limit);
    }
    u32::try_from(path.len())
        .map_err(|_| WireError::Limit)?
        .encode(e)?;
    for step in path {
        e.nodes(1)?;
        step.encode(e)?;
    }
    Ok(())
}
fn decode_life_type_path(d: &mut Decoder<'_>) -> Result<Vec<crate::LifeTypeProjection>> {
    let count = u32::decode(d)? as usize;
    if count > d.max_depth || count > d.nodes_left {
        return Err(WireError::Limit);
    }
    d.allocation(
        count
            .checked_mul(std::mem::size_of::<crate::LifeTypeProjection>())
            .ok_or(WireError::Limit)?,
    )?;
    let mut path = Vec::new();
    path.try_reserve_exact(count)
        .map_err(|_| WireError::Limit)?;
    for _ in 0..count {
        d.nodes(1)?;
        path.push(crate::LifeTypeProjection::decode(d)?);
    }
    Ok(path)
}
impl Wire for crate::StorageRetainedBinding {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            encode_life_type_path(&self.path, e)?;
            self.class.encode(e)?;
            self.binder.encode(e)
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                path: decode_life_type_path(d)?,
                class: u32::decode(d)?,
                binder: u32::decode(d)?,
            })
        })
    }
}
impl Wire for crate::LifeDeclarationId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.module.encode(e)?;
            self.declaration.encode(e)?;
            self.compiler.encode(e)
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                module: ModuleId::decode(d)?,
                declaration: NodeId::decode(d)?,
                compiler: bool::decode(d)?,
            })
        })
    }
}
impl Wire for crate::NominalLifeArgument {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            encode_life_type_path(&self.path, e)?;
            self.declaration.encode(e)?;
            self.argument.encode(e)?;
            self.binder.encode(e)
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                path: decode_life_type_path(d)?,
                declaration: crate::LifeDeclarationId::decode(d)?,
                argument: u32::decode(d)?,
                binder: u32::decode(d)?,
            })
        })
    }
}
impl Wire for crate::StorageLifeShape {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            e.nodes(self.binder_count as usize)?;
            self.binder_count.encode(e)?;
            self.slots.encode(e)?;
            self.retained.encode(e)?;
            self.nominal_arguments.encode(e)
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            let binder_count = u32::decode(d)?;
            d.nodes(binder_count as usize)?;
            Ok(Self {
                binder_count,
                slots: Vec::<crate::StorageLifeSlot>::decode(d)?,
                retained: Vec::<crate::StorageRetainedBinding>::decode(d)?,
                nominal_arguments: Vec::<crate::NominalLifeArgument>::decode(d)?,
            })
        })
    }
}

impl Wire for CtTypeNode {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::BorrowedStorage { storage, life } => {
                    16u8.encode(e)?;
                    storage.encode(e)?;
                    life.encode(e)?;
                }
                Self::Primitive(v0) => {
                    0u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Builtin(v0) => {
                    1u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Nominal { symbol, arguments } => {
                    2u8.encode(e)?;
                    symbol.encode(e)?;
                    arguments.encode(e)?;
                }
                Self::Tuple(v0) => {
                    3u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Array { element, length } => {
                    4u8.encode(e)?;
                    element.encode(e)?;
                    length.encode(e)?;
                }
                Self::FunctionPointer { parameters, result } => {
                    5u8.encode(e)?;
                    parameters.encode(e)?;
                    result.encode(e)?;
                }
                Self::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life,
                } => {
                    14u8.encode(e)?;
                    parameters.encode(e)?;
                    result.encode(e)?;
                    life.encode(e)?;
                }
                Self::Address { mutable, inner } => {
                    6u8.encode(e)?;
                    mutable.encode(e)?;
                    inner.encode(e)?;
                }
                Self::Slice(v0) => {
                    7u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Managed(v0) => {
                    8u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Desc(v0) => {
                    9u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::OwnedStore(v0) => {
                    10u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::ForeignCallback { parameters, result } => {
                    11u8.encode(e)?;
                    parameters.encode(e)?;
                    result.encode(e)?;
                }
                Self::Opaque { identity } => {
                    12u8.encode(e)?;
                    identity.encode(e)?;
                }
                Self::ErrorUnion { error, result } => {
                    13u8.encode(e)?;
                    error.encode(e)?;
                    result.encode(e)?;
                }
                Self::AnonymousSum(members) => {
                    15u8.encode(e)?;
                    members.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            16 => Ok(Self::BorrowedStorage {
                storage: CtTypeNodeId::decode(d)?,
                life: crate::StorageLifeShape::decode(d)?,
            }),
            0 => Ok(Self::Primitive(<CtScalarTy as Wire>::decode(d)?)),
            1 => Ok(Self::Builtin(<CtBuiltinType as Wire>::decode(d)?)),
            2 => Ok(Self::Nominal {
                symbol: <CtSymbolId as Wire>::decode(d)?,
                arguments: <Vec<CtTypeArgument> as Wire>::decode(d)?,
            }),
            3 => Ok(Self::Tuple(<Vec<CtTypeNodeId> as Wire>::decode(d)?)),
            4 => Ok(Self::Array {
                element: <CtTypeNodeId as Wire>::decode(d)?,
                length: <u32 as Wire>::decode(d)?,
            }),
            5 => Ok(Self::FunctionPointer {
                parameters: <Vec<CtTypeNodeId> as Wire>::decode(d)?,
                result: <CtTypeNodeId as Wire>::decode(d)?,
            }),
            14 => Ok(Self::BorrowedFunctionPointer {
                parameters: Vec::<CtTypeNodeId>::decode(d)?,
                result: CtTypeNodeId::decode(d)?,
                life: StorageLifeContract::decode(d)?,
            }),
            6 => Ok(Self::Address {
                mutable: <bool as Wire>::decode(d)?,
                inner: <CtTypeNodeId as Wire>::decode(d)?,
            }),
            7 => Ok(Self::Slice(<CtTypeNodeId as Wire>::decode(d)?)),
            8 => Ok(Self::Managed(<CtTypeNodeId as Wire>::decode(d)?)),
            9 => Ok(Self::Desc(<CtTypeNodeId as Wire>::decode(d)?)),
            10 => Ok(Self::OwnedStore(<CtTypeNodeId as Wire>::decode(d)?)),
            11 => Ok(Self::ForeignCallback {
                parameters: <Vec<CtTypeNodeId> as Wire>::decode(d)?,
                result: <CtTypeNodeId as Wire>::decode(d)?,
            }),
            12 => Ok(Self::Opaque {
                identity: <CtSymbolId as Wire>::decode(d)?,
            }),
            13 => Ok(Self::ErrorUnion {
                error: <CtTypeNodeId as Wire>::decode(d)?,
                result: <CtTypeNodeId as Wire>::decode(d)?,
            }),
            15 => Ok(Self::AnonymousSum(<Vec<CtTypeNodeId> as Wire>::decode(d)?)),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtTypeArgument {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Type(v0) => {
                    0u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Lifetime(v0) => {
                    2u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Scalar(v0) => {
                    1u8.encode(e)?;
                    v0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Type(<CtTypeNodeId as Wire>::decode(d)?)),
            1 => Ok(Self::Scalar(<Constant as Wire>::decode(d)?)),
            2 => Ok(Self::Lifetime(CtLifetimeArgument::decode(d)?)),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtNameNamespace {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Unbound => {
                    0u8.encode(e)?;
                }
                Self::Value => {
                    1u8.encode(e)?;
                }
                Self::Type => {
                    2u8.encode(e)?;
                }
                Self::Member => {
                    3u8.encode(e)?;
                }
                Self::Module => {
                    4u8.encode(e)?;
                }
                Self::Label => {
                    5u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Unbound),
            1 => Ok(Self::Value),
            2 => Ok(Self::Type),
            3 => Ok(Self::Member),
            4 => Ok(Self::Module),
            5 => Ok(Self::Label),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtNameSource {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Input(v0) => {
                    0u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Definition(v0) => {
                    1u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Fresh { invocation, binder } => {
                    2u8.encode(e)?;
                    invocation.encode(e)?;
                    binder.encode(e)?;
                }
                Self::Export => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Input(<CtInputTokenId as Wire>::decode(d)?)),
            1 => Ok(Self::Definition(<CtSymbolId as Wire>::decode(d)?)),
            2 => Ok(Self::Fresh {
                invocation: <Fingerprint as Wire>::decode(d)?,
                binder: <u32 as Wire>::decode(d)?,
            }),
            3 => Ok(Self::Export),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtIdent {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.text.encode(e)?;
            self.namespace.encode(e)?;
            self.source.encode(e)?;
            self.member_owner.encode(e)?;
            self.member_owner_type.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                text: <String as Wire>::decode(d)?,
                namespace: <CtNameNamespace as Wire>::decode(d)?,
                source: <CtNameSource as Wire>::decode(d)?,
                member_owner: <Option<CtSymbolId> as Wire>::decode(d)?,
                member_owner_type: <Option<CtTypeRef> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtDelimiter {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Parenthesis => {
                    0u8.encode(e)?;
                }
                Self::Bracket => {
                    1u8.encode(e)?;
                }
                Self::Brace => {
                    2u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Parenthesis),
            1 => Ok(Self::Bracket),
            2 => Ok(Self::Brace),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtTokenClass {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Identifier => {
                    0u8.encode(e)?;
                }
                Self::Integer => {
                    1u8.encode(e)?;
                }
                Self::String => {
                    2u8.encode(e)?;
                }
                Self::Punctuation => {
                    3u8.encode(e)?;
                }
                Self::Keyword => {
                    4u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Identifier),
            1 => Ok(Self::Integer),
            2 => Ok(Self::String),
            3 => Ok(Self::Punctuation),
            4 => Ok(Self::Keyword),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtOriginRole {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Input => {
                    0u8.encode(e)?;
                }
                Self::QuoteDefinition => {
                    1u8.encode(e)?;
                }
                Self::Generated => {
                    2u8.encode(e)?;
                }
                Self::Splice => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Input),
            1 => Ok(Self::QuoteDefinition),
            2 => Ok(Self::Generated),
            3 => Ok(Self::Splice),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtOrigin {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.span.encode(e)?;
            self.parent.encode(e)?;
            self.role.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                span: <Span as Wire>::decode(d)?,
                parent: <Option<CtOriginId> as Wire>::decode(d)?,
                role: <CtOriginRole as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtToken {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.class.encode(e)?;
            self.text.encode(e)?;
            self.origin.encode(e)?;
            self.name.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                class: <CtTokenClass as Wire>::decode(d)?,
                text: <String as Wire>::decode(d)?,
                origin: <CtOriginId as Wire>::decode(d)?,
                name: <Option<CtIdent> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtTokenNode {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Token(v0) => {
                    0u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Group {
                    delimiter,
                    children,
                    origin,
                } => {
                    1u8.encode(e)?;
                    delimiter.encode(e)?;
                    children.encode(e)?;
                    origin.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Token(<CtToken as Wire>::decode(d)?)),
            1 => Ok(Self::Group {
                delimiter: <CtDelimiter as Wire>::decode(d)?,
                children: <Vec<CtTokenNodeId> as Wire>::decode(d)?,
                origin: <CtOriginId as Wire>::decode(d)?,
            }),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtTokenTree {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.nodes.encode(e)?;
            self.roots.encode(e)?;
            self.origins.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                nodes: <Vec<CtTokenNode> as Wire>::decode(d)?,
                roots: <Vec<CtTokenNodeId> as Wire>::decode(d)?,
                origins: <Vec<CtOrigin> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtTokenCursor {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.tree_fingerprint.encode(e)?;
            self.node.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                tree_fingerprint: <Fingerprint as Wire>::decode(d)?,
                node: <CtTokenNodeId as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtSyntaxNode {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Token(v0) => {
                    0u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Group {
                    delimiter,
                    children,
                    origin,
                } => {
                    1u8.encode(e)?;
                    delimiter.encode(e)?;
                    children.encode(e)?;
                    origin.encode(e)?;
                }
                Self::Hole { index, origin } => {
                    2u8.encode(e)?;
                    index.encode(e)?;
                    origin.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Token(<CtToken as Wire>::decode(d)?)),
            1 => Ok(Self::Group {
                delimiter: <CtDelimiter as Wire>::decode(d)?,
                children: <Vec<CtSyntaxNodeId> as Wire>::decode(d)?,
                origin: <CtOriginId as Wire>::decode(d)?,
            }),
            2 => Ok(Self::Hole {
                index: <u32 as Wire>::decode(d)?,
                origin: <CtOriginId as Wire>::decode(d)?,
            }),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtSyntaxTree {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.category.encode(e)?;
            self.nodes.encode(e)?;
            self.roots.encode(e)?;
            self.origins.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                category: <CtSyntaxCategory as Wire>::decode(d)?,
                nodes: <Vec<CtSyntaxNode> as Wire>::decode(d)?,
                roots: <Vec<CtSyntaxNodeId> as Wire>::decode(d)?,
                origins: <Vec<CtOrigin> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtHoleKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Expr => {
                    0u8.encode(e)?;
                }
                Self::Type => {
                    1u8.encode(e)?;
                }
                Self::StmtList => {
                    2u8.encode(e)?;
                }
                Self::ItemList => {
                    3u8.encode(e)?;
                }
                Self::Ident(v0) => {
                    4u8.encode(e)?;
                    v0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Expr),
            1 => Ok(Self::Type),
            2 => Ok(Self::StmtList),
            3 => Ok(Self::ItemList),
            4 => Ok(Self::Ident(<CtNameNamespace as Wire>::decode(d)?)),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtTemplate {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.definition.encode(e)?;
            self.syntax.encode(e)?;
            self.holes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                definition: <CtSymbolId as Wire>::decode(d)?,
                syntax: <CtSyntaxTree as Wire>::decode(d)?,
                holes: <Vec<CtHoleKind> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtInputToken {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.span.encode(e)?;
            self.class.encode(e)?;
            self.text.encode(e)?;
            self.scope_fingerprint.encode(e)?;
            self.binding.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                span: <Span as Wire>::decode(d)?,
                class: <CtTokenClass as Wire>::decode(d)?,
                text: <String as Wire>::decode(d)?,
                scope_fingerprint: <Fingerprint as Wire>::decode(d)?,
                binding: <Option<CtSymbolId> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtInputTable {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.invocation_fingerprint.encode(e)?;
            self.tokens.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                invocation_fingerprint: <Fingerprint as Wire>::decode(d)?,
                tokens: <Vec<CtInputToken> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtFieldInfo {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.owner.encode(e)?;
            self.owner_type.encode(e)?;
            self.field.encode(e)?;
            self.name.encode(e)?;
            self.ty.encode(e)?;
            self.managed.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                owner: <CtSymbolId as Wire>::decode(d)?,
                owner_type: <CtTypeRef as Wire>::decode(d)?,
                field: <CtSymbolId as Wire>::decode(d)?,
                name: <CtIdent as Wire>::decode(d)?,
                ty: <CtTypeRef as Wire>::decode(d)?,
                managed: <bool as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtShapeInfo {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.nominal.encode(e)?;
            self.visible_fields.encode(e)?;
            self.complete.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                nominal: <CtSymbolId as Wire>::decode(d)?,
                visible_fields: <Vec<CtFieldInfo> as Wire>::decode(d)?,
                complete: <bool as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtImplInfo {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.trait_symbol.encode(e)?;
            self.implementation.encode(e)?;
            self.for_type.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                trait_symbol: <CtSymbolId as Wire>::decode(d)?,
                implementation: <CtSymbolId as Wire>::decode(d)?,
                for_type: <CtTypeRef as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtLayoutInfo {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.target_fingerprint.encode(e)?;
            self.size.encode(e)?;
            self.alignment.encode(e)?;
            self.field_offsets.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                target_fingerprint: <Fingerprint as Wire>::decode(d)?,
                size: <u64 as Wire>::decode(d)?,
                alignment: <u64 as Wire>::decode(d)?,
                field_offsets: <Vec<u64> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtConstant {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Scalar(v0) => {
                    0u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Text(v0) => {
                    1u8.encode(e)?;
                    v0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Scalar(<Constant as Wire>::decode(d)?)),
            1 => Ok(Self::Text(<String as Wire>::decode(d)?)),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtFunction {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.id.encode(e)?;
            self.definition.encode(e)?;
            self.parameters.encode(e)?;
            self.parameter_types.encode(e)?;
            self.result_type.encode(e)?;
            self.locals.encode(e)?;
            self.values.encode(e)?;
            self.blocks.encode(e)?;
            self.entry.encode(e)?;
            self.span.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                id: <CtFunctionId as Wire>::decode(d)?,
                definition: <CtSymbolId as Wire>::decode(d)?,
                parameters: <Vec<CtLocalId> as Wire>::decode(d)?,
                parameter_types: <Vec<CtTy> as Wire>::decode(d)?,
                result_type: <CtTy as Wire>::decode(d)?,
                locals: <Vec<CtTy> as Wire>::decode(d)?,
                values: <Vec<CtTy> as Wire>::decode(d)?,
                blocks: <Vec<CtBlock> as Wire>::decode(d)?,
                entry: <CtBlockId as Wire>::decode(d)?,
                span: <Span as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtBlock {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.id.encode(e)?;
            self.instructions.encode(e)?;
            self.terminator.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                id: <CtBlockId as Wire>::decode(d)?,
                instructions: <Vec<CtInstruction> as Wire>::decode(d)?,
                terminator: <CtTerminator as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtInstruction {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.result.encode(e)?;
            self.kind.encode(e)?;
            self.span.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                result: <Option<CtValueId> as Wire>::decode(d)?,
                kind: <CtInstructionKind as Wire>::decode(d)?,
                span: <Span as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtTerminator {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Return(v0) => {
                    0u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Jump(v0) => {
                    1u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    2u8.encode(e)?;
                    condition.encode(e)?;
                    then_block.encode(e)?;
                    else_block.encode(e)?;
                }
                Self::Trap => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Return(<Option<CtValueId> as Wire>::decode(d)?)),
            1 => Ok(Self::Jump(<CtBlockId as Wire>::decode(d)?)),
            2 => Ok(Self::Branch {
                condition: <CtValueId as Wire>::decode(d)?,
                then_block: <CtBlockId as Wire>::decode(d)?,
                else_block: <CtBlockId as Wire>::decode(d)?,
            }),
            3 => Ok(Self::Trap),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtTypeConstructor {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::BorrowedStorage { life } => {
                    12u8.encode(e)?;
                    life.encode(e)?;
                }
                Self::BorrowedFunctionPointer { life } => {
                    10u8.encode(e)?;
                    life.encode(e)?;
                }
                Self::Array => {
                    0u8.encode(e)?;
                }
                Self::Tuple => {
                    1u8.encode(e)?;
                }
                Self::FunctionPointer => {
                    2u8.encode(e)?;
                }
                Self::Address { mutable } => {
                    3u8.encode(e)?;
                    mutable.encode(e)?;
                }
                Self::Slice => {
                    4u8.encode(e)?;
                }
                Self::Managed => {
                    5u8.encode(e)?;
                }
                Self::Desc => {
                    6u8.encode(e)?;
                }
                Self::OwnedStore => {
                    7u8.encode(e)?;
                }
                Self::ErrorUnion => {
                    8u8.encode(e)?;
                }
                Self::AnonymousSum => {
                    11u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            12 => Ok(Self::BorrowedStorage {
                life: crate::StorageLifeShape::decode(d)?,
            }),
            10 => Ok(Self::BorrowedFunctionPointer {
                life: StorageLifeContract::decode(d)?,
            }),
            0 => Ok(Self::Array),
            1 => Ok(Self::Tuple),
            2 => Ok(Self::FunctionPointer),
            3 => Ok(Self::Address {
                mutable: <bool as Wire>::decode(d)?,
            }),
            4 => Ok(Self::Slice),
            5 => Ok(Self::Managed),
            6 => Ok(Self::Desc),
            7 => Ok(Self::OwnedStore),
            8 => Ok(Self::ErrorUnion),
            11 => Ok(Self::AnonymousSum),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtSyntaxBuilder {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Literal => {
                    0u8.encode(e)?;
                }
                Self::TypeRef => {
                    1u8.encode(e)?;
                }
                Self::NameExpr => {
                    2u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Literal),
            1 => Ok(Self::TypeRef),
            2 => Ok(Self::NameExpr),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtNameBuilder {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Fresh => {
                    0u8.encode(e)?;
                }
                Self::Export => {
                    1u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Fresh),
            1 => Ok(Self::Export),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtTextOp {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Equal => {
                    0u8.encode(e)?;
                }
                Self::Length => {
                    1u8.encode(e)?;
                }
                Self::Slice => {
                    2u8.encode(e)?;
                }
                Self::Concatenate => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Equal),
            1 => Ok(Self::Length),
            2 => Ok(Self::Slice),
            3 => Ok(Self::Concatenate),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtTokenProjection {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::RootCount => {
                    0u8.encode(e)?;
                }
                Self::RootAt => {
                    1u8.encode(e)?;
                }
                Self::Class => {
                    2u8.encode(e)?;
                }
                Self::Text => {
                    3u8.encode(e)?;
                }
                Self::Delimiter => {
                    4u8.encode(e)?;
                }
                Self::ChildCount => {
                    5u8.encode(e)?;
                }
                Self::ChildAt => {
                    6u8.encode(e)?;
                }
                Self::InputName => {
                    7u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::RootCount),
            1 => Ok(Self::RootAt),
            2 => Ok(Self::Class),
            3 => Ok(Self::Text),
            4 => Ok(Self::Delimiter),
            5 => Ok(Self::ChildCount),
            6 => Ok(Self::ChildAt),
            7 => Ok(Self::InputName),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtMetaProjection {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::ShapeFields => {
                    0u8.encode(e)?;
                }
                Self::ShapeComplete => {
                    1u8.encode(e)?;
                }
                Self::FieldName => {
                    2u8.encode(e)?;
                }
                Self::FieldType => {
                    3u8.encode(e)?;
                }
                Self::FieldManaged => {
                    4u8.encode(e)?;
                }
                Self::ImplTrait => {
                    5u8.encode(e)?;
                }
                Self::ImplType => {
                    6u8.encode(e)?;
                }
                Self::LayoutSize => {
                    7u8.encode(e)?;
                }
                Self::LayoutAlignment => {
                    8u8.encode(e)?;
                }
                Self::LayoutFieldOffsets => {
                    9u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::ShapeFields),
            1 => Ok(Self::ShapeComplete),
            2 => Ok(Self::FieldName),
            3 => Ok(Self::FieldType),
            4 => Ok(Self::FieldManaged),
            5 => Ok(Self::ImplTrait),
            6 => Ok(Self::ImplType),
            7 => Ok(Self::LayoutSize),
            8 => Ok(Self::LayoutAlignment),
            9 => Ok(Self::LayoutFieldOffsets),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtQueryKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Shape => {
                    0u8.encode(e)?;
                }
                Self::Field => {
                    1u8.encode(e)?;
                }
                Self::ImplIndex => {
                    2u8.encode(e)?;
                }
                Self::Layout => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Shape),
            1 => Ok(Self::Field),
            2 => Ok(Self::ImplIndex),
            3 => Ok(Self::Layout),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtInstructionKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Constant(v0) => {
                    0u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::InvocationInput { index } => {
                    1u8.encode(e)?;
                    index.encode(e)?;
                }
                Self::StorageLive(v0) => {
                    2u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::StorageDead(v0) => {
                    3u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Load(v0) => {
                    4u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::Store { local, value } => {
                    5u8.encode(e)?;
                    local.encode(e)?;
                    value.encode(e)?;
                }
                Self::ConvertNumber { value, target } => {
                    36u8.encode(e)?;
                    value.encode(e)?;
                    target.encode(e)?;
                }
                Self::ConvertInteger { value, target } => {
                    6u8.encode(e)?;
                    value.encode(e)?;
                    target.encode(e)?;
                }
                Self::Unary { op, operand } => {
                    7u8.encode(e)?;
                    op.encode(e)?;
                    operand.encode(e)?;
                }
                Self::Binary { op, left, right } => {
                    8u8.encode(e)?;
                    op.encode(e)?;
                    left.encode(e)?;
                    right.encode(e)?;
                }
                Self::MakeAnonymousSum {
                    members,
                    active,
                    payload,
                } => {
                    37u8.encode(e)?;
                    members.encode(e)?;
                    active.encode(e)?;
                    payload.encode(e)?;
                }
                Self::MakeTuple(v0) => {
                    9u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::MakeArray(v0) => {
                    10u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::MakeList(v0) => {
                    11u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::MakeRecord { schema, fields } => {
                    12u8.encode(e)?;
                    schema.encode(e)?;
                    fields.encode(e)?;
                }
                Self::MakeVariant {
                    schema,
                    tag,
                    fields,
                } => {
                    13u8.encode(e)?;
                    schema.encode(e)?;
                    tag.encode(e)?;
                    fields.encode(e)?;
                }
                Self::Length(v0) => {
                    14u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::VariantTag(v0) => {
                    15u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::VariantField {
                    value,
                    variant,
                    field,
                } => {
                    16u8.encode(e)?;
                    value.encode(e)?;
                    variant.encode(e)?;
                    field.encode(e)?;
                }
                Self::Extract { aggregate, index } => {
                    17u8.encode(e)?;
                    aggregate.encode(e)?;
                    index.encode(e)?;
                }
                Self::ExtractField { aggregate, index } => {
                    18u8.encode(e)?;
                    aggregate.encode(e)?;
                    index.encode(e)?;
                }
                Self::Replace {
                    aggregate,
                    index,
                    value,
                } => {
                    19u8.encode(e)?;
                    aggregate.encode(e)?;
                    index.encode(e)?;
                    value.encode(e)?;
                }
                Self::ReplaceField {
                    aggregate,
                    index,
                    value,
                } => {
                    20u8.encode(e)?;
                    aggregate.encode(e)?;
                    index.encode(e)?;
                    value.encode(e)?;
                }
                Self::Call {
                    function,
                    arguments,
                } => {
                    21u8.encode(e)?;
                    function.encode(e)?;
                    arguments.encode(e)?;
                }
                Self::TypePrimitive(v0) => {
                    22u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::TypeBuiltin(v0) => {
                    23u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::TypeName(v0) => {
                    24u8.encode(e)?;
                    v0.encode(e)?;
                }
                Self::TypeApply { nominal, arguments } => {
                    25u8.encode(e)?;
                    nominal.encode(e)?;
                    arguments.encode(e)?;
                }
                Self::TypeConstruct {
                    constructor,
                    arguments,
                } => {
                    26u8.encode(e)?;
                    constructor.encode(e)?;
                    arguments.encode(e)?;
                }
                Self::Quote { template, holes } => {
                    27u8.encode(e)?;
                    template.encode(e)?;
                    holes.encode(e)?;
                }
                Self::Parse { category, input } => {
                    28u8.encode(e)?;
                    category.encode(e)?;
                    input.encode(e)?;
                }
                Self::ParseIdent { input } => {
                    29u8.encode(e)?;
                    input.encode(e)?;
                }
                Self::TokenInspect {
                    source,
                    projection,
                    index,
                } => {
                    30u8.encode(e)?;
                    source.encode(e)?;
                    projection.encode(e)?;
                    index.encode(e)?;
                }
                Self::Text { op, arguments } => {
                    31u8.encode(e)?;
                    op.encode(e)?;
                    arguments.encode(e)?;
                }
                Self::SyntaxBuild { kind, arguments } => {
                    32u8.encode(e)?;
                    kind.encode(e)?;
                    arguments.encode(e)?;
                }
                Self::NameBuild { kind, text } => {
                    33u8.encode(e)?;
                    kind.encode(e)?;
                    text.encode(e)?;
                }
                Self::Query { kind, arguments } => {
                    34u8.encode(e)?;
                    kind.encode(e)?;
                    arguments.encode(e)?;
                }
                Self::MetaProject { value, projection } => {
                    35u8.encode(e)?;
                    value.encode(e)?;
                    projection.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Constant(<CtConstantId as Wire>::decode(d)?)),
            1 => Ok(Self::InvocationInput {
                index: <u32 as Wire>::decode(d)?,
            }),
            2 => Ok(Self::StorageLive(<CtLocalId as Wire>::decode(d)?)),
            3 => Ok(Self::StorageDead(<CtLocalId as Wire>::decode(d)?)),
            4 => Ok(Self::Load(<CtLocalId as Wire>::decode(d)?)),
            5 => Ok(Self::Store {
                local: <CtLocalId as Wire>::decode(d)?,
                value: <CtValueId as Wire>::decode(d)?,
            }),
            37 => Ok(Self::MakeAnonymousSum {
                members: Vec::<CtTy>::decode(d)?,
                active: u32::decode(d)?,
                payload: CtValueId::decode(d)?,
            }),
            36 => Ok(Self::ConvertNumber {
                value: <CtValueId as Wire>::decode(d)?,
                target: <CtScalarTy as Wire>::decode(d)?,
            }),
            6 => Ok(Self::ConvertInteger {
                value: <CtValueId as Wire>::decode(d)?,
                target: <CtScalarTy as Wire>::decode(d)?,
            }),
            7 => Ok(Self::Unary {
                op: <UnaryOp as Wire>::decode(d)?,
                operand: <CtValueId as Wire>::decode(d)?,
            }),
            8 => Ok(Self::Binary {
                op: <BinaryOp as Wire>::decode(d)?,
                left: <CtValueId as Wire>::decode(d)?,
                right: <CtValueId as Wire>::decode(d)?,
            }),
            9 => Ok(Self::MakeTuple(<Vec<CtValueId> as Wire>::decode(d)?)),
            10 => Ok(Self::MakeArray(<Vec<CtValueId> as Wire>::decode(d)?)),
            11 => Ok(Self::MakeList(<Vec<CtValueId> as Wire>::decode(d)?)),
            12 => Ok(Self::MakeRecord {
                schema: <CtRecordSchemaId as Wire>::decode(d)?,
                fields: <Vec<CtValueId> as Wire>::decode(d)?,
            }),
            13 => Ok(Self::MakeVariant {
                schema: <CtEnumSchemaId as Wire>::decode(d)?,
                tag: <u32 as Wire>::decode(d)?,
                fields: <Vec<CtValueId> as Wire>::decode(d)?,
            }),
            14 => Ok(Self::Length(<CtValueId as Wire>::decode(d)?)),
            15 => Ok(Self::VariantTag(<CtValueId as Wire>::decode(d)?)),
            16 => Ok(Self::VariantField {
                value: <CtValueId as Wire>::decode(d)?,
                variant: <u32 as Wire>::decode(d)?,
                field: <u32 as Wire>::decode(d)?,
            }),
            17 => Ok(Self::Extract {
                aggregate: <CtValueId as Wire>::decode(d)?,
                index: <CtValueId as Wire>::decode(d)?,
            }),
            18 => Ok(Self::ExtractField {
                aggregate: <CtValueId as Wire>::decode(d)?,
                index: <u32 as Wire>::decode(d)?,
            }),
            19 => Ok(Self::Replace {
                aggregate: <CtValueId as Wire>::decode(d)?,
                index: <CtValueId as Wire>::decode(d)?,
                value: <CtValueId as Wire>::decode(d)?,
            }),
            20 => Ok(Self::ReplaceField {
                aggregate: <CtValueId as Wire>::decode(d)?,
                index: <u32 as Wire>::decode(d)?,
                value: <CtValueId as Wire>::decode(d)?,
            }),
            21 => Ok(Self::Call {
                function: <CtFunctionId as Wire>::decode(d)?,
                arguments: <Vec<CtValueId> as Wire>::decode(d)?,
            }),
            22 => Ok(Self::TypePrimitive(<CtScalarTy as Wire>::decode(d)?)),
            23 => Ok(Self::TypeBuiltin(<CtBuiltinType as Wire>::decode(d)?)),
            24 => Ok(Self::TypeName(<CtSymbolId as Wire>::decode(d)?)),
            25 => Ok(Self::TypeApply {
                nominal: <CtSymbolId as Wire>::decode(d)?,
                arguments: <Vec<CtValueId> as Wire>::decode(d)?,
            }),
            26 => Ok(Self::TypeConstruct {
                constructor: <CtTypeConstructor as Wire>::decode(d)?,
                arguments: <Vec<CtValueId> as Wire>::decode(d)?,
            }),
            27 => Ok(Self::Quote {
                template: <CtTemplateId as Wire>::decode(d)?,
                holes: <Vec<CtValueId> as Wire>::decode(d)?,
            }),
            28 => Ok(Self::Parse {
                category: <CtSyntaxCategory as Wire>::decode(d)?,
                input: <CtValueId as Wire>::decode(d)?,
            }),
            29 => Ok(Self::ParseIdent {
                input: <CtValueId as Wire>::decode(d)?,
            }),
            30 => Ok(Self::TokenInspect {
                source: <CtValueId as Wire>::decode(d)?,
                projection: <CtTokenProjection as Wire>::decode(d)?,
                index: <Option<CtValueId> as Wire>::decode(d)?,
            }),
            31 => Ok(Self::Text {
                op: <CtTextOp as Wire>::decode(d)?,
                arguments: <Vec<CtValueId> as Wire>::decode(d)?,
            }),
            32 => Ok(Self::SyntaxBuild {
                kind: <CtSyntaxBuilder as Wire>::decode(d)?,
                arguments: <Vec<CtValueId> as Wire>::decode(d)?,
            }),
            33 => Ok(Self::NameBuild {
                kind: <CtNameBuilder as Wire>::decode(d)?,
                text: <CtValueId as Wire>::decode(d)?,
            }),
            34 => Ok(Self::Query {
                kind: <CtQueryKind as Wire>::decode(d)?,
                arguments: <Vec<CtValueId> as Wire>::decode(d)?,
            }),
            35 => Ok(Self::MetaProject {
                value: <CtValueId as Wire>::decode(d)?,
                projection: <CtMetaProjection as Wire>::decode(d)?,
            }),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtReadyStage {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            match self {
                Self::Tokens => {
                    0u8.encode(e)?;
                }
                Self::Config => {
                    1u8.encode(e)?;
                }
                Self::Shape => {
                    2u8.encode(e)?;
                }
                Self::ImplIndex => {
                    3u8.encode(e)?;
                }
                Self::Layout => {
                    4u8.encode(e)?;
                }
                Self::Contract => {
                    5u8.encode(e)?;
                }
                Self::VerifiedBody => {
                    6u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| match u8::decode(d)? {
            0 => Ok(Self::Tokens),
            1 => Ok(Self::Config),
            2 => Ok(Self::Shape),
            3 => Ok(Self::ImplIndex),
            4 => Ok(Self::Layout),
            5 => Ok(Self::Contract),
            6 => Ok(Self::VerifiedBody),
            _ => Err(WireError::UnknownTag),
        })
    }
}

impl Wire for CtSnapshotRequest {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.fingerprint.encode(e)?;
            self.ready.encode(e)?;
            self.target_fingerprint.encode(e)?;
            self.requested_queries.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                fingerprint: <Fingerprint as Wire>::decode(d)?,
                ready: <CtReadyStage as Wire>::decode(d)?,
                target_fingerprint: <Option<Fingerprint> as Wire>::decode(d)?,
                requested_queries: <Vec<CtQueryKind> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtQueryDependency {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.kind.encode(e)?;
            self.subject_fingerprint.encode(e)?;
            self.result_fingerprint.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                kind: <CtQueryKind as Wire>::decode(d)?,
                subject_fingerprint: <Fingerprint as Wire>::decode(d)?,
                result_fingerprint: <Fingerprint as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtDeclaredBudget {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.max_steps.encode(e)?;
            self.max_call_depth.encode(e)?;
            self.max_live_cells.encode(e)?;
            self.max_temporary_bytes.encode(e)?;
            self.max_generated_nodes.encode(e)?;
            self.max_instances.encode(e)?;
            self.max_analysis_work.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                max_steps: <u64 as Wire>::decode(d)?,
                max_call_depth: <u32 as Wire>::decode(d)?,
                max_live_cells: <u64 as Wire>::decode(d)?,
                max_temporary_bytes: <u64 as Wire>::decode(d)?,
                max_generated_nodes: <u64 as Wire>::decode(d)?,
                max_instances: <u64 as Wire>::decode(d)?,
                max_analysis_work: <u64 as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtUsageReceipt {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.steps.encode(e)?;
            self.peak_call_depth.encode(e)?;
            self.peak_live_cells.encode(e)?;
            self.peak_temporary_bytes.encode(e)?;
            self.generated_nodes.encode(e)?;
            self.instances.encode(e)?;
            self.analysis_work.encode(e)?;
            self.queries.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                steps: <u64 as Wire>::decode(d)?,
                peak_call_depth: <u32 as Wire>::decode(d)?,
                peak_live_cells: <u64 as Wire>::decode(d)?,
                peak_temporary_bytes: <u64 as Wire>::decode(d)?,
                generated_nodes: <u64 as Wire>::decode(d)?,
                instances: <u64 as Wire>::decode(d)?,
                analysis_work: <u64 as Wire>::decode(d)?,
                queries: <Vec<CtQueryDependency> as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtArtifactProgram {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.schema_version.encode(e)?;
            self.target_pointer_width.encode(e)?;
            self.source_fingerprint.encode(e)?;
            self.ambient_life_scopes.encode(e)?;
            self.functions.encode(e)?;
            self.entry.encode(e)?;
            self.constants.encode(e)?;
            self.record_schemas.encode(e)?;
            self.enum_schemas.encode(e)?;
            self.templates.encode(e)?;
            self.input_tokens.encode(e)?;
            self.snapshot.encode(e)?;
            self.budget.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                schema_version: <u32 as Wire>::decode(d)?,
                target_pointer_width: <u16 as Wire>::decode(d)?,
                source_fingerprint: <Fingerprint as Wire>::decode(d)?,
                ambient_life_scopes: Vec::<CtAmbientLifeScope>::decode(d)?,
                functions: <Vec<CtFunction> as Wire>::decode(d)?,
                entry: <CtFunctionId as Wire>::decode(d)?,
                constants: <Vec<CtConstant> as Wire>::decode(d)?,
                record_schemas: <Vec<CtRecordSchema> as Wire>::decode(d)?,
                enum_schemas: <Vec<CtEnumSchema> as Wire>::decode(d)?,
                templates: <Vec<CtTemplate> as Wire>::decode(d)?,
                input_tokens: <CtInputTable as Wire>::decode(d)?,
                snapshot: <CtSnapshotRequest as Wire>::decode(d)?,
                budget: <CtDeclaredBudget as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtProviderEntry {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.path.encode(e)?;
            self.export.encode(e)?;
            self.symbol.encode(e)?;
            self.output.encode(e)?;
            self.delimiters.encode(e)?;
            self.implementation_fingerprint.encode(e)?;
            self.grammar_fingerprint.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                path: <String as Wire>::decode(d)?,
                export: <String as Wire>::decode(d)?,
                symbol: <CtSymbolId as Wire>::decode(d)?,
                output: <CtSyntaxCategory as Wire>::decode(d)?,
                delimiters: <Vec<CtDelimiter> as Wire>::decode(d)?,
                implementation_fingerprint: <Fingerprint as Wire>::decode(d)?,
                grammar_fingerprint: <Fingerprint as Wire>::decode(d)?,
            })
        })
    }
}

impl Wire for CtLockedProviderRegistry {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.schema_version.encode(e)?;
            self.entries.encode(e)?;
            self.fingerprint.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                schema_version: <u32 as Wire>::decode(d)?,
                entries: <Vec<CtProviderEntry> as Wire>::decode(d)?,
                fingerprint: <Fingerprint as Wire>::decode(d)?,
            })
        })
    }
}

// The two recursive CT trees use explicit continuations. Their tags and
// field order match the generated schema-1 enum codecs exactly.
impl Wire for CtTy {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        enum Task<'a> {
            Ty(&'a CtTy),
            Tys(&'a [CtTy]),
            Length(u32),
            Exit,
        }
        let mut pending = Scratch::new();
        pending.push_encode(e, Task::Ty(self))?;
        while let Some(task) = pending.pop() {
            match task {
                Task::Exit => e.depth -= 1,
                Task::Length(n) => n.encode(e)?,
                Task::Tys(items) => {
                    if let Some((first, rest)) = items.split_first() {
                        e.nodes(1)?;
                        pending.push_encode(e, Task::Tys(rest))?;
                        pending.push_encode(e, Task::Ty(first))?;
                    }
                }
                Task::Ty(ty) => {
                    e.enter()?;
                    pending.push_encode(e, Task::Exit)?;
                    match ty {
                        CtTy::Scalar(v) => {
                            0u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtTy::Type => 1u8.encode(e)?,
                        CtTy::Syntax(v) => {
                            2u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtTy::Ident => 3u8.encode(e)?,
                        CtTy::TokenTree => 4u8.encode(e)?,
                        CtTy::TokenCursor => 5u8.encode(e)?,
                        CtTy::Text => 6u8.encode(e)?,
                        CtTy::ShapeInfo => 7u8.encode(e)?,
                        CtTy::FieldInfo => 8u8.encode(e)?,
                        CtTy::ImplInfo => 9u8.encode(e)?,
                        CtTy::LayoutInfo => 10u8.encode(e)?,
                        CtTy::Tuple(items) | CtTy::AnonymousSum(items) => {
                            (if matches!(ty, CtTy::AnonymousSum(_)) {
                                16u8
                            } else {
                                11u8
                            })
                            .encode(e)?;
                            if items.len() > e.nodes_left {
                                return Err(WireError::Limit);
                            }
                            u32::try_from(items.len())
                                .map_err(|_| WireError::Limit)?
                                .encode(e)?;
                            pending.push_encode(e, Task::Tys(items))?;
                        }
                        CtTy::Array { element, length } => {
                            12u8.encode(e)?;
                            pending.push_encode(e, Task::Length(*length))?;
                            pending.push_encode(e, Task::Ty(element))?;
                        }
                        CtTy::List(inner) => {
                            13u8.encode(e)?;
                            pending.push_encode(e, Task::Ty(inner))?;
                        }
                        CtTy::Record(id) => {
                            14u8.encode(e)?;
                            id.encode(e)?;
                        }
                        CtTy::Enum(id) => {
                            15u8.encode(e)?;
                            id.encode(e)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        enum Kind {
            Tuple,
            AnonymousSum,
            Array,
            List,
        }
        struct Frame {
            kind: Kind,
            left: usize,
            items: Vec<CtTy>,
        }
        let mut frames: Scratch<Frame> = Scratch::new();
        let mut root = None;
        loop {
            if frames.last().is_some_and(|f| f.left == 0) {
                let frame = frames.pop().ok_or(WireError::NonCanonical)?;
                let value = match frame.kind {
                    Kind::Tuple => CtTy::Tuple(frame.items),
                    Kind::AnonymousSum => CtTy::AnonymousSum(frame.items),
                    Kind::Array => {
                        let length = u32::decode(d)?;
                        d.allocation(std::mem::size_of::<CtTy>())?;
                        CtTy::Array {
                            element: Box::new(
                                frame
                                    .items
                                    .into_iter()
                                    .next()
                                    .ok_or(WireError::NonCanonical)?,
                            ),
                            length,
                        }
                    }
                    Kind::List => {
                        d.allocation(std::mem::size_of::<CtTy>())?;
                        CtTy::List(Box::new(
                            frame
                                .items
                                .into_iter()
                                .next()
                                .ok_or(WireError::NonCanonical)?,
                        ))
                    }
                };
                d.depth -= 1;
                if let Some(parent) = frames.last_mut() {
                    parent.items.push(value);
                } else {
                    root = Some(value);
                }
                continue;
            }
            if root.is_some() && frames.is_empty() {
                return root.ok_or(WireError::NonCanonical);
            }
            if let Some(frame) = frames.last_mut() {
                if matches!(frame.kind, Kind::Tuple | Kind::AnonymousSum) {
                    d.nodes(1)?;
                }
                frame.left -= 1;
            }
            d.enter()?;
            let tag = u8::decode(d)?;
            let mut nested = None;
            let value = match tag {
                0 => Some(CtTy::Scalar(CtScalarTy::decode(d)?)),
                1 => Some(CtTy::Type),
                2 => Some(CtTy::Syntax(CtSyntaxCategory::decode(d)?)),
                3 => Some(CtTy::Ident),
                4 => Some(CtTy::TokenTree),
                5 => Some(CtTy::TokenCursor),
                6 => Some(CtTy::Text),
                7 => Some(CtTy::ShapeInfo),
                8 => Some(CtTy::FieldInfo),
                9 => Some(CtTy::ImplInfo),
                10 => Some(CtTy::LayoutInfo),
                16 => {
                    let n = u32::decode(d)? as usize;
                    nested = Some((Kind::AnonymousSum, n));
                    None
                }
                11 => {
                    let n = u32::decode(d)? as usize;
                    nested = Some((Kind::Tuple, n));
                    None
                }
                12 => {
                    nested = Some((Kind::Array, 1));
                    None
                }
                13 => {
                    nested = Some((Kind::List, 1));
                    None
                }
                14 => Some(CtTy::Record(CtRecordSchemaId::decode(d)?)),
                15 => Some(CtTy::Enum(CtEnumSchemaId::decode(d)?)),
                _ => return Err(WireError::UnknownTag),
            };
            if let Some((kind, n)) = nested {
                if n > d.nodes_left && matches!(kind, Kind::Tuple | Kind::AnonymousSum) {
                    return Err(WireError::Limit);
                }
                let allocation = n
                    .checked_mul(std::mem::size_of::<CtTy>())
                    .ok_or(WireError::Limit)?;
                d.allocation(allocation)?;
                let mut items = Vec::new();
                if items.try_reserve_exact(n).is_err() {
                    d.allocation_left += allocation;
                    d.work_left += allocation;
                    return Err(WireError::Limit);
                }
                frames.push_decode(
                    d,
                    Frame {
                        kind,
                        left: n,
                        items,
                    },
                )?;
            } else {
                d.depth -= 1;
                let value = value.ok_or(WireError::NonCanonical)?;
                if let Some(parent) = frames.last_mut() {
                    parent.items.push(value);
                } else {
                    root = Some(value);
                }
            }
        }
    }
}

impl Wire for CtValue {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        enum Task<'a> {
            Value(&'a CtValue),
            Values(&'a [CtValue]),
            Exit,
        }
        let mut pending = Scratch::new();
        pending.push_encode(e, Task::Value(self))?;
        while let Some(task) = pending.pop() {
            match task {
                Task::Exit => e.depth -= 1,
                Task::Values(items) => {
                    if let Some((first, rest)) = items.split_first() {
                        e.nodes(1)?;
                        pending.push_encode(e, Task::Values(rest))?;
                        pending.push_encode(e, Task::Value(first))?;
                    }
                }
                Task::Value(value) => {
                    e.enter()?;
                    pending.push_encode(e, Task::Exit)?;
                    let mut collection = None;
                    match value {
                        CtValue::Scalar(v) => {
                            0u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::Text(v) => {
                            1u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::Type(v) => {
                            2u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::Syntax(v) => {
                            3u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::Ident(v) => {
                            4u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::TokenTree(v) => {
                            5u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::TokenCursor(v) => {
                            6u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::ShapeInfo(v) => {
                            7u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::FieldInfo(v) => {
                            8u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::ImplInfo(v) => {
                            9u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::LayoutInfo(v) => {
                            10u8.encode(e)?;
                            v.encode(e)?;
                        }
                        CtValue::AnonymousSum {
                            members,
                            active,
                            payload,
                        } => {
                            16u8.encode(e)?;
                            members.encode(e)?;
                            active.encode(e)?;
                            e.nodes(1)?;
                            pending.push_encode(e, Task::Value(payload))?;
                        }
                        CtValue::Tuple(v) => {
                            11u8.encode(e)?;
                            collection = Some(v.as_slice());
                        }
                        CtValue::Array(v) => {
                            12u8.encode(e)?;
                            collection = Some(v.as_slice());
                        }
                        CtValue::List(v) => {
                            13u8.encode(e)?;
                            collection = Some(v.as_slice());
                        }
                        CtValue::Record { schema, fields } => {
                            14u8.encode(e)?;
                            schema.encode(e)?;
                            collection = Some(fields.as_slice());
                        }
                        CtValue::Variant {
                            schema,
                            tag,
                            fields,
                        } => {
                            15u8.encode(e)?;
                            schema.encode(e)?;
                            tag.encode(e)?;
                            collection = Some(fields.as_slice());
                        }
                    }
                    if let Some(items) = collection {
                        if items.len() > e.nodes_left {
                            return Err(WireError::Limit);
                        }
                        u32::try_from(items.len())
                            .map_err(|_| WireError::Limit)?
                            .encode(e)?;
                        pending.push_encode(e, Task::Values(items))?;
                    }
                }
            }
        }
        Ok(())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        enum Kind {
            Tuple,
            Array,
            List,
            Record(CtRecordSchemaId),
            Variant(CtEnumSchemaId, u32),
            // Box this new descriptor so legacy continuation frames retain
            // their previous inline size and allocation accounting.
            AnonymousSum(Box<(Vec<CtTy>, u32)>),
        }
        struct Frame {
            kind: Kind,
            left: usize,
            items: Vec<CtValue>,
        }
        let mut frames: Scratch<Frame> = Scratch::new();
        let mut root = None;
        loop {
            if frames.last().is_some_and(|f| f.left == 0) {
                let frame = frames.pop().ok_or(WireError::NonCanonical)?;
                let value = match frame.kind {
                    Kind::AnonymousSum(descriptor) => {
                        let (members, active) = *descriptor;
                        d.allocation(std::mem::size_of::<CtValue>())?;
                        CtValue::AnonymousSum {
                            members,
                            active,
                            payload: Box::new(
                                frame
                                    .items
                                    .into_iter()
                                    .next()
                                    .ok_or(WireError::NonCanonical)?,
                            ),
                        }
                    }
                    Kind::Tuple => CtValue::Tuple(frame.items),
                    Kind::Array => CtValue::Array(frame.items),
                    Kind::List => CtValue::List(frame.items),
                    Kind::Record(schema) => CtValue::Record {
                        schema,
                        fields: frame.items,
                    },
                    Kind::Variant(schema, tag) => CtValue::Variant {
                        schema,
                        tag,
                        fields: frame.items,
                    },
                };
                d.depth -= 1;
                if let Some(parent) = frames.last_mut() {
                    parent.items.push(value)
                } else {
                    root = Some(value)
                }
                continue;
            }
            if root.is_some() && frames.is_empty() {
                return root.ok_or(WireError::NonCanonical);
            }
            if let Some(frame) = frames.last_mut() {
                d.nodes(1)?;
                frame.left -= 1;
            }
            d.enter()?;
            let tag = u8::decode(d)?;
            let mut nested = None;
            let mut fixed_one = false;
            let value = match tag {
                0 => Some(CtValue::Scalar(Constant::decode(d)?)),
                1 => Some(CtValue::Text(String::decode(d)?)),
                2 => Some(CtValue::Type(CtTypeRef::decode(d)?)),
                3 => Some(CtValue::Syntax(CtSyntaxTree::decode(d)?)),
                4 => Some(CtValue::Ident(CtIdent::decode(d)?)),
                5 => Some(CtValue::TokenTree(CtTokenTree::decode(d)?)),
                6 => Some(CtValue::TokenCursor(CtTokenCursor::decode(d)?)),
                7 => Some(CtValue::ShapeInfo(CtShapeInfo::decode(d)?)),
                8 => Some(CtValue::FieldInfo(CtFieldInfo::decode(d)?)),
                9 => Some(CtValue::ImplInfo(CtImplInfo::decode(d)?)),
                10 => Some(CtValue::LayoutInfo(CtLayoutInfo::decode(d)?)),
                16 => {
                    let members = Vec::<CtTy>::decode(d)?;
                    let active = u32::decode(d)?;
                    d.allocation(std::mem::size_of::<(Vec<CtTy>, u32)>())?;
                    nested = Some(Kind::AnonymousSum(Box::new((members, active))));
                    fixed_one = true;
                    None
                }
                11 => {
                    nested = Some(Kind::Tuple);
                    None
                }
                12 => {
                    nested = Some(Kind::Array);
                    None
                }
                13 => {
                    nested = Some(Kind::List);
                    None
                }
                14 => {
                    let id = CtRecordSchemaId::decode(d)?;
                    nested = Some(Kind::Record(id));
                    None
                }
                15 => {
                    let id = CtEnumSchemaId::decode(d)?;
                    let tag = u32::decode(d)?;
                    nested = Some(Kind::Variant(id, tag));
                    None
                }
                _ => return Err(WireError::UnknownTag),
            };
            if let Some(kind) = nested {
                let n = if fixed_one {
                    1
                } else {
                    u32::decode(d)? as usize
                };
                if n > d.nodes_left {
                    return Err(WireError::Limit);
                }
                let allocation = n
                    .checked_mul(std::mem::size_of::<CtValue>())
                    .ok_or(WireError::Limit)?;
                d.allocation(allocation)?;
                let mut items = Vec::new();
                if items.try_reserve_exact(n).is_err() {
                    d.allocation_left += allocation;
                    d.work_left += allocation;
                    return Err(WireError::Limit);
                }
                frames.push_decode(
                    d,
                    Frame {
                        kind,
                        left: n,
                        items,
                    },
                )?;
            } else {
                d.depth -= 1;
                let value = value.ok_or(WireError::NonCanonical)?;
                if let Some(parent) = frames.last_mut() {
                    parent.items.push(value)
                } else {
                    root = Some(value)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
