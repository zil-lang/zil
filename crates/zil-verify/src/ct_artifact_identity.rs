//! Independent complete CT identities. A schema index is only a table edge.
use super::*;
#[path = "ct_artifact_graph_identity.rs"]
mod graph;

fn pending_scoped_identity() -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Ct, "CT4 scoped metadata identity is not yet implemented", None)]
}
struct Identity<'a, 'b> {
    raw: &'a CtArtifactProgram,
    limits: &'a Limits,
    meter: &'b mut CtVerificationMeter,
    bytes: usize,
    visiting: Vec<(bool, u32)>,
}
impl Identity<'_, '_> {
    fn reserve(&mut self, cost: usize) -> Check<()> {
        self.meter.charge(cost, None)?;
        self.bytes = self
            .bytes
            .checked_add(cost)
            .ok_or_else(|| limit(None, "complete CT identity byte accounting overflow"))?;
        if self.bytes as u64 > self.raw.budget.max_temporary_bytes {
            return Err(limit(None, "complete CT identity scratch exhausted"));
        }
        Ok(())
    }
    fn put(&mut self, out: &mut Vec<u8>, bytes: &[u8]) -> Check<()> {
        // Account for geometric buffer growth and small initial capacities.
        self.reserve(bytes.len().saturating_mul(2).saturating_add(8))?;
        out.extend_from_slice(bytes);
        Ok(())
    }
    fn number(&mut self, out: &mut Vec<u8>, n: u64) -> Check<()> {
        self.put(out, &n.to_be_bytes())
    }
    fn scalar(&mut self, out: &mut Vec<u8>, value: &Constant) -> Check<()> {
        let (tag, bits) = match value {
            Constant::Unit => (0, 0),
            Constant::Bool(v) => (1, u64::from(*v)),
            Constant::U8(v) => (2, u64::from(*v)),
            Constant::U16(v) => (3, u64::from(*v)),
            Constant::I32(v) => (4, *v as u32 as u64),
            Constant::I64(v) => (5, *v as u64),
            Constant::U64(v) => (6, *v),
            Constant::Usize(v) => {
                if self.raw.target_pointer_width == 32 && *v > u64::from(u32::MAX) {
                    return Err(reject(
                        None,
                        "CT nominal usize argument exceeds target width",
                    ));
                }
                (7, *v)
            }
            Constant::F32(v) => (8, u64::from(*v)),
            Constant::F64(v) => (9, *v),
            _ => return Err(reject(None, "CT nominal argument is not a pure scalar")),
        };
        self.put(out, &[tag])?;
        self.number(out, bits)
    }
    fn child(&mut self, out: &mut Vec<u8>, ty: &CtTy, depth: usize) -> Check<()> {
        let key = self.key(ty, depth)?;
        self.number(out, key.len() as u64)?;
        self.put(out, &key)
    }
    fn nominal(
        &mut self,
        out: &mut Vec<u8>,
        enumeration: bool,
        id: u32,
        symbol: &CtSymbolId,
        arguments: &[CtSchemaArgument],
        _depth: usize,
    ) -> Check<()> {
        self.meter
            .charge(self.visiting.len().saturating_add(1), None)?;
        if self.visiting.contains(&(enumeration, id)) {
            return Err(reject(
                None,
                "CT nominal arguments contain an identity cycle",
            ));
        }
        // The pending path has at most one bounded entry per recursion level.
        self.reserve(16)?;
        self.visiting.push((enumeration, id));
        self.number(out, u64::from(symbol.module.0))?;
        self.number(out, u64::from(symbol.declaration.0))?;
        self.put(out, &[u8::from(symbol.generation_path.is_some())])?;
        if let Some(path) = symbol.generation_path {
            self.put(out, &path.0)?;
        }
        self.number(out, arguments.len() as u64)?;
        for argument in arguments {
            match argument {
                CtSchemaArgument::Type(ty) => {
                    self.put(out, &[0])?;
                    let key = self.graph(ty)?;
                    self.number(out, key.len() as u64)?;
                    self.put(out, &key)?;
                }
                CtSchemaArgument::TypeInContext(_) | CtSchemaArgument::Lifetime(_) => return Err(pending_scoped_identity()),
                CtSchemaArgument::Scalar(value) => {
                    self.put(out, &[1])?;
                    self.scalar(out, value)?;
                }
            }
        }
        self.visiting.pop();
        Ok(())
    }
    fn members(&mut self, members: &[CtTy], depth: usize, out: &mut Vec<u8>) -> Check<()> {
        if members.len() < 2 {
            return Err(reject(
                None,
                "anonymous CT sum requires at least two members",
            ));
        }
        self.number(out, members.len() as u64)?;
        let mut previous: Option<Vec<u8>> = None;
        for member in members {
            if matches!(member, CtTy::AnonymousSum(_)) {
                return Err(reject(None, "anonymous CT sum is not flattened"));
            }
            let key = self.key(member, depth + 1)?;
            if let Some(prior) = &previous {
                self.meter
                    .charge(prior.len().saturating_add(key.len()), None)?;
                if prior >= &key {
                    return Err(reject(
                        None,
                        "anonymous CT members repeat or are not canonical",
                    ));
                }
            }
            self.number(out, key.len() as u64)?;
            self.put(out, &key)?;
            previous = Some(key);
        }
        Ok(())
    }
    fn key(&mut self, ty: &CtTy, depth: usize) -> Check<Vec<u8>> {
        self.meter.charge(1, None)?;
        if depth > self.limits.max_call_depth || depth > self.raw.budget.max_call_depth as usize {
            return Err(limit(None, "complete CT identity depth exceeded"));
        }
        let mut out = Vec::new();
        let tag = match ty {
            CtTy::Scalar(_) => 0,
            CtTy::Type => 1,
            CtTy::Syntax(_) => 2,
            CtTy::Ident => 3,
            CtTy::TokenTree => 4,
            CtTy::TokenCursor => 5,
            CtTy::Text => 6,
            CtTy::ShapeInfo => 7,
            CtTy::FieldInfo => 8,
            CtTy::ImplInfo => 9,
            CtTy::LayoutInfo => 10,
            CtTy::Tuple(_) => 11,
            CtTy::Array { .. } => 12,
            CtTy::List(_) => 13,
            CtTy::Record(_) => 14,
            CtTy::Enum(_) => 15,
            CtTy::AnonymousSum(_) => 16,
        };
        self.put(&mut out, &[tag])?;
        match ty {
            CtTy::Scalar(s) => self.put(&mut out, &[*s as u8])?,
            CtTy::Syntax(s) => self.put(&mut out, &[*s as u8])?,
            CtTy::Tuple(fields) => {
                self.number(&mut out, fields.len() as u64)?;
                for field in fields {
                    self.child(&mut out, field, depth + 1)?;
                }
            }
            CtTy::Array { element, length } => {
                self.number(&mut out, u64::from(*length))?;
                self.child(&mut out, element, depth + 1)?;
            }
            CtTy::List(element) => self.child(&mut out, element, depth + 1)?,
            CtTy::Record(id) => {
                let records = &self.raw.record_schemas;
                let schema = records
                    .get(id.0 as usize)
                    .ok_or_else(|| reject(None, "CT complete record schema is missing"))?;
                if schema.life_context.is_some() { return Err(pending_scoped_identity()); }

                self.nominal(
                    &mut out,
                    false,
                    id.0,
                    &schema.name,
                    &schema.arguments,
                    depth,
                )?;
            }
            CtTy::Enum(id) => {
                let enums = &self.raw.enum_schemas;
                let schema = enums
                    .get(id.0 as usize)
                    .ok_or_else(|| reject(None, "CT complete enum schema is missing"))?;
                if schema.life_context.is_some() { return Err(pending_scoped_identity()); }

                self.nominal(&mut out, true, id.0, &schema.name, &schema.arguments, depth)?;
            }
            CtTy::AnonymousSum(members) => self.members(members, depth, &mut out)?,
            _ => {}
        }
        Ok(out)
    }
}
pub(super) fn check_members(
    members: &[CtTy],
    raw: &CtArtifactProgram,
    limits: &Limits,
    meter: &mut CtVerificationMeter,
) -> Check<()> {
    let mut identity = Identity {
        raw,
        limits,
        meter,
        bytes: 0,
        visiting: Vec::new(),
    };
    identity.members(members, 0, &mut Vec::new())
}
pub(super) fn check_instances(
    raw: &CtArtifactProgram,
    limits: &Limits,
    meter: &mut CtVerificationMeter,
) -> Check<()> {
    let mut identity = Identity {
        raw,
        limits,
        meter,
        bytes: 0,
        visiting: Vec::new(),
    };
    let mut prior: Vec<Vec<u8>> = Vec::new();
    for (enumeration, count) in [
        (false, raw.record_schemas.len()),
        (true, raw.enum_schemas.len()),
    ] {
        for index in 0..count {
            let ty = if enumeration {
                CtTy::Enum(CtEnumSchemaId(index as u32))
            } else {
                CtTy::Record(CtRecordSchemaId(index as u32))
            };
            let key = identity.key(&ty, 0)?;
            for old in &prior {
                identity
                    .meter
                    .charge(old.len().saturating_add(key.len()), None)?;
                if old == &key {
                    return Err(reject(None, "CT nominal schema instance is duplicated"));
                }
            }
            identity.reserve(48)?;
            prior.push(key);
        }
    }
    Ok(())
}
pub(super) fn check_argument_graphs(
    raw: &CtArtifactProgram,
    limits: &Limits,
    meter: &mut CtVerificationMeter,
) -> Check<()> {
    for (arguments, context) in raw.record_schemas.iter().map(|s| (s.arguments.as_slice(), &s.life_context))
        .chain(raw.enum_schemas.iter().map(|s| (s.arguments.as_slice(), &s.life_context)))
    {
        if let Some(context) = context {
            meter.charge(1, None)?;
            if context.nodes.len() > limits.max_nodes || context.nodes.len() as u64 > raw.budget.max_generated_nodes {
                return Err(limit(None, "CT4 context exceeds node quota"));
            }
            return Err(pending_scoped_identity());
        }
        for arg in arguments {
            match arg {
                CtSchemaArgument::Type(graph) => {
                    let mut identity = Identity { raw, limits, meter, bytes: 0, visiting: Vec::new() };
                    identity.graph(graph)?;
                }
                CtSchemaArgument::TypeInContext(_) | CtSchemaArgument::Lifetime(_) => return Err(reject(None, "scoped schema argument has no joint context")),
                CtSchemaArgument::Scalar(_) => {},
            }
        }
    }
    Ok(())
}
