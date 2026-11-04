//! Pure structural CT data identity. This never grants source or runtime access.
use crate::CtExecutionBudget;
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ir::{Constant, ct_artifact::*};
fn bad(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-ARTIFACT", None, message)]
}
fn limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "CT complete type identity exhausted",
        None,
    )]
}
struct Keys<'a> {
    records: &'a [CtRecordSchema],
    enums: &'a [CtEnumSchema],
    budget: &'a mut CtExecutionBudget,
    visiting: Vec<(bool, u32)>,
    owned: u64,
}
impl Keys<'_> {
    fn release(&mut self, bytes: u64) {
        self.budget.release_temporary_bytes(bytes);
        self.owned = self.owned.saturating_sub(bytes);
    }
    fn reserve(&mut self, bytes: u64) -> CompileResult<()> {
        self.budget.reserve_temporary_bytes(bytes)?;
        self.owned = self.owned.saturating_add(bytes);
        Ok(())
    }
    fn bytes(&mut self, out: &mut Vec<u8>, bytes: &[u8]) -> CompileResult<()> {
        self.budget.reserve_analysis_work(bytes.len() as u64)?;
        let needed = out.len().checked_add(bytes.len()).ok_or_else(limit)?;
        if needed > out.capacity() {
            let capacity = needed.max(out.capacity().saturating_mul(2)).max(8);
            self.budget.reserve_analysis_work(out.len() as u64)?;
            self.reserve((capacity - out.capacity()) as u64)?;
            out.reserve_exact(capacity - out.len());
        }
        out.extend_from_slice(bytes);
        Ok(())
    }
    fn finish(mut self, result: CompileResult<Vec<u8>>) -> CompileResult<Vec<u8>> {
        let retained = result.as_ref().map_or(0, |key| key.capacity() as u64);
        self.release(self.owned.saturating_sub(retained));
        result
    }
    fn number(&mut self, out: &mut Vec<u8>, n: u64) -> CompileResult<()> {
        self.bytes(out, &n.to_be_bytes())
    }
    fn symbol(&mut self, out: &mut Vec<u8>, s: &CtSymbolId) -> CompileResult<()> {
        self.number(out, u64::from(s.module.0))?;
        self.number(out, u64::from(s.declaration.0))?;
        self.bytes(out, &[u8::from(s.generation_path.is_some())])?;
        if let Some(fp) = s.generation_path {
            self.bytes(out, &fp.0)?;
        }
        Ok(())
    }
    fn scalar(&mut self, out: &mut Vec<u8>, c: &Constant) -> CompileResult<()> {
        let (tag, bits) = match c {
            Constant::Unit => (0, 0),
            Constant::Bool(v) => (1, u64::from(*v)),
            Constant::U8(v) => (2, u64::from(*v)),
            Constant::U16(v) => (3, u64::from(*v)),
            Constant::I32(v) => (4, *v as u32 as u64),
            Constant::I64(v) => (5, *v as u64),
            Constant::U64(v) => (6, *v),
            Constant::Usize(v) => (7, *v),
            Constant::F32(v) => (8, u64::from(*v)),
            Constant::F64(v) => (9, *v),
            _ => return Err(bad("nominal argument is not a pure scalar")),
        };
        self.bytes(out, &[tag])?;
        self.number(out, bits)
    }
    fn key(&mut self, ty: &CtTy, depth: usize) -> CompileResult<Vec<u8>> {
        self.budget.reserve_analysis_work(1)?;
        if depth > self.budget.remaining().max_call_depth as usize {
            return Err(limit());
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
        self.bytes(&mut out, &[tag])?;
        match ty {
            CtTy::Scalar(s) => self.bytes(&mut out, &[*s as u8])?,
            CtTy::Syntax(s) => self.bytes(&mut out, &[*s as u8])?,
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
                let schema = self
                    .records
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("CT record identity schema is missing"))?;

                self.nominal(
                    &mut out,
                    false,
                    id.0,
                    &schema.name,
                    &schema.arguments,
                    schema.life_context.as_ref(),
                )?;
            }
            CtTy::Enum(id) => {
                let schema = self
                    .enums
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("CT enum identity schema is missing"))?;

                self.nominal(
                    &mut out,
                    true,
                    id.0,
                    &schema.name,
                    &schema.arguments,
                    schema.life_context.as_ref(),
                )?;
            }
            CtTy::AnonymousSum(members) => self.members(&mut out, members, depth)?,

            _ => {}
        }
        Ok(out)
    }
    fn members(&mut self, out: &mut Vec<u8>, members: &[CtTy], depth: usize) -> CompileResult<()> {
        if members.len() < 2 {
            return Err(bad("anonymous CT sum needs two complete members"));
        }
        self.number(out, members.len() as u64)?;
        let mut previous: Option<Vec<u8>> = None;
        for member in members {
            if matches!(member, CtTy::AnonymousSum(_)) {
                return Err(bad("anonymous CT sum is not flattened"));
            }
            let key = self.key(member, depth + 1)?;
            if let Some(prior) = &previous {
                self.budget
                    .reserve_analysis_work((prior.len() as u64).saturating_add(key.len() as u64))?;
                if prior >= &key {
                    return Err(bad("anonymous CT members repeat or are not canonical"));
                }
            }
            self.number(out, key.len() as u64)?;
            self.bytes(out, &key)?;
            if let Some(prior) = previous.replace(key) {
                self.release(prior.capacity() as u64);
            }
        }
        if let Some(prior) = previous {
            self.release(prior.capacity() as u64);
        }
        Ok(())
    }
    fn child(&mut self, out: &mut Vec<u8>, ty: &CtTy, depth: usize) -> CompileResult<()> {
        let key = self.key(ty, depth)?;
        self.number(out, key.len() as u64)?;
        self.bytes(out, &key)?;
        self.release(key.capacity() as u64);
        Ok(())
    }
    fn context(
        &mut self,
        out: &mut Vec<u8>,
        symbol: &CtSymbolId,
        args: &[CtSchemaArgument],
        context: &CtTypeRef,
    ) -> CompileResult<()> {
        self.budget
            .reserve_analysis_work(context.nodes.len() as u64 + args.len() as u64 + 1)?;
        let root = context
            .nodes
            .get(context.root.0 as usize)
            .ok_or_else(|| bad("joint context root is missing"))?;
        let CtTypeNode::BorrowedStorage { storage, .. } = root else {
            return Err(bad("joint nominal context has no actual fixed owner"));
        };
        let CtTypeNode::Nominal {
            symbol: actual,
            arguments,
        } = context
            .nodes
            .get(storage.0 as usize)
            .ok_or_else(|| bad("joint nominal application is missing"))?
        else {
            return Err(bad(
                "joint context root does not select a nominal application",
            ));
        };
        if symbol != actual || args.len() != arguments.len() {
            return Err(bad("joint nominal context owner or argument arity differs"));
        }
        for (argument, actual) in args.iter().zip(arguments) {
            let same = match (argument, actual) {
                (CtSchemaArgument::TypeInContext(a), CtTypeArgument::Type(b)) => a == b,
                (CtSchemaArgument::Scalar(a), CtTypeArgument::Scalar(b)) => {
                    self.scalar(&mut Vec::new(), a)?;
                    a == b
                }
                (CtSchemaArgument::Lifetime(a), CtTypeArgument::Lifetime(b)) => a == b,
                _ => false,
            };
            if !same {
                return Err(bad(
                    "joint schema argument differs from its exact ordered context projection",
                ));
            }
        }
        let (canonical, key) = crate::anonymous_type::canonical_type_key(context, self.budget)?;
        self.budget.reserve_analysis_work(
            (canonical.nodes.len() + context.nodes.len()) as u64 + key.len() as u64,
        )?;
        let result = if &canonical != context {
            Err(bad("joint context is not canonical"))
        } else {
            self.bytes(out, &[2])
                .and_then(|()| self.number(out, key.len() as u64))
                .and_then(|()| self.bytes(out, &key))
        };
        self.budget.release_temporary_bytes(
            crate::anonymous_type::retained_bytes(&canonical).saturating_add(key.capacity() as u64),
        );
        result
    }
    fn nominal(
        &mut self,
        out: &mut Vec<u8>,
        enumeration: bool,
        id: u32,
        symbol: &CtSymbolId,
        args: &[CtSchemaArgument],
        context: Option<&CtTypeRef>,
    ) -> CompileResult<()> {
        self.budget
            .reserve_analysis_work(self.visiting.len() as u64 + args.len() as u64 + 1)?;
        if self.visiting.contains(&(enumeration, id)) {
            return Err(bad("nominal instance arguments contain an identity cycle"));
        }
        if self.visiting.len() == self.visiting.capacity() {
            let capacity = self.visiting.capacity().saturating_mul(2).max(4);
            self.reserve(
                ((capacity - self.visiting.capacity()) * std::mem::size_of::<(bool, u32)>()) as u64,
            )?;
            self.visiting.reserve_exact(capacity - self.visiting.len());
        }
        self.visiting.push((enumeration, id));
        self.symbol(out, symbol)?;
        self.number(out, args.len() as u64)?;
        if let Some(context) = context {
            self.context(out, symbol, args, context)?;
            self.visiting.pop();
            return Ok(());
        }
        for arg in args {
            match arg {
                CtSchemaArgument::Type(ty) => {
                    self.bytes(out, &[0])?;
                    let (canonical, key) =
                        crate::anonymous_type::canonical_type_key(ty, self.budget)?;
                    self.budget.reserve_analysis_work(
                        (ty.nodes.len() as u64)
                            .saturating_add(canonical.nodes.len() as u64)
                            .saturating_add(key.len() as u64),
                    )?;
                    let same = ty == &canonical;
                    let fixed = crate::anonymous_type::scoped_graph(ty)
                        && ty
                            .nodes
                            .iter()
                            .any(|n| matches!(n, CtTypeNode::BorrowedStorage { .. }));
                    let copied = if fixed {
                        Err(bad("fixed nominal argument requires its one joint context"))
                    } else if same {
                        self.number(out, key.len() as u64)
                            .and_then(|()| self.bytes(out, &key))
                    } else {
                        Err(bad("nominal type argument graph is not canonical"))
                    };
                    self.budget.release_temporary_bytes(
                        crate::anonymous_type::retained_bytes(&canonical).saturating_add(
                            if crate::anonymous_type::scoped_graph(&canonical) {
                                key.capacity() as u64
                            } else {
                                key.len() as u64
                            },
                        ),
                    );
                    copied?;
                }
                CtSchemaArgument::TypeInContext(_) | CtSchemaArgument::Lifetime(_) => {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Ct,
                        "CT4 scoped schema identity is not yet implemented",
                        None,
                    )]);
                }
                CtSchemaArgument::Scalar(c) => {
                    self.bytes(out, &[1])?;
                    self.scalar(out, c)?;
                }
            }
        }
        self.visiting.pop();
        Ok(())
    }
}
/// Complete constructor/nominal key; numeric schema table IDs never enter it.
/// The returned key's capacity charge belongs to the caller until drop.
pub fn artifact_type_key(
    ty: &CtTy,
    records: &[CtRecordSchema],
    enums: &[CtEnumSchema],
    budget: &mut CtExecutionBudget,
) -> CompileResult<Vec<u8>> {
    let mut keys = Keys {
        records,
        enums,
        budget,
        visiting: Vec::new(),
        owned: 0,
    };
    let result = keys.key(ty, 0);
    keys.finish(result)
}
/// Validate and key a borrowed sum descriptor without cloning untrusted data.
pub fn artifact_sum_members_key(
    members: &[CtTy],
    records: &[CtRecordSchema],
    enums: &[CtEnumSchema],
    budget: &mut CtExecutionBudget,
) -> CompileResult<Vec<u8>> {
    let mut keys = Keys {
        records,
        enums,
        budget,
        visiting: Vec::new(),
        owned: 0,
    };
    let result = (|| {
        let mut out = Vec::new();
        keys.bytes(&mut out, &[16])?;
        keys.members(&mut out, members, 0)?;
        Ok(out)
    })();
    keys.finish(result)
}

/// Resolve canonical member order from full nominal arguments, retaining all
/// inactive members. This transformation supplies no source/syntax authority.
pub fn canonical_artifact_sum(
    members: Vec<CtTy>,
    records: &[CtRecordSchema],
    enums: &[CtEnumSchema],
    budget: &mut CtExecutionBudget,
) -> CompileResult<CtTy> {
    let mut todo = members;
    let mut flat = Vec::new();
    let mut scratch = 0u64;
    while let Some(member) = todo.pop() {
        budget.reserve_analysis_work(1)?;
        if let CtTy::AnonymousSum(members) = member {
            let bytes = (members.len() as u64)
                .saturating_mul(std::mem::size_of::<CtTy>() as u64)
                .saturating_mul(4);
            budget.reserve_temporary_bytes(bytes)?;
            scratch = scratch.saturating_add(bytes);
            todo.extend(members);
        } else {
            budget.reserve_temporary_bytes(256)?;
            scratch = scratch.saturating_add(256);
            let key = artifact_type_key(&member, records, enums, budget)?;
            flat.push((key, member));
        }
    }
    if flat.len() < 2 {
        return Err(bad("anonymous CT sum needs two complete members"));
    }
    let key_bytes = flat
        .iter()
        .fold(0u64, |n, (key, _)| n.saturating_add(key.len() as u64));
    budget.reserve_analysis_work(key_bytes.saturating_mul(flat.len().saturating_add(2) as u64))?;
    flat.sort_by(|a, b| a.0.cmp(&b.0));
    if flat.windows(2).any(|v| v[0].0 == v[1].0) {
        return Err(bad("anonymous CT sum repeats a complete member identity"));
    }
    let key_capacity = flat
        .iter()
        .fold(0u64, |n, (key, _)| n.saturating_add(key.capacity() as u64));
    let result = CtTy::AnonymousSum(flat.into_iter().map(|(_, member)| member).collect());
    budget.release_temporary_bytes(scratch.saturating_add(key_capacity));
    Ok(result)
}
