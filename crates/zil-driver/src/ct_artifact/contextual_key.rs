//! Contextual data identity. This never grants visibility, construction or
//! borrowing rights; candidate data still passes the ordinary independent gates.
use super::{execute::value_context, invocation_context::FrozenInvocationContext};
use zil_base::{CompileResult, Diagnostic, Fingerprint, Limits, ModuleId, NodeId, Phase};
use zil_ct::{CtExecutionBudget, canonicalize_anonymous_type};
use zil_ir::{
    Constant,
    ct_artifact::*,
    ct_wire::{self, WireBudget},
};
fn bad(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-CONTEXT", None, message)]
}
struct Key<'a, 'b> {
    context: &'a FrozenInvocationContext,
    records: &'a [CtRecordSchema],
    enums: &'a [CtEnumSchema],
    scopes: &'a [CtAmbientLifeScope],
    limits: &'a Limits,
    budget: &'b mut CtExecutionBudget,
    out: Vec<u8>,
    owned: u64,
    visiting: Vec<(bool, u32)>,
    scope_visiting: Vec<u32>,
}
impl Key<'_, '_> {
    fn reserve(&mut self, bytes: u64) -> CompileResult<()> {
        self.budget.reserve_temporary_bytes(bytes)?;
        self.owned = self.owned.saturating_add(bytes);
        Ok(())
    }
    fn release(&mut self, bytes: u64) {
        self.budget.release_temporary_bytes(bytes);
        self.owned = self.owned.saturating_sub(bytes);
    }
    fn bytes(&mut self, value: &[u8]) -> CompileResult<()> {
        self.budget.reserve_analysis_work(value.len() as u64)?;
        let needed = self
            .out
            .len()
            .checked_add(value.len())
            .ok_or_else(|| bad("context key length overflow"))?;
        if needed > self.out.capacity() {
            let capacity = needed.max(self.out.capacity().saturating_mul(2)).max(8);
            self.budget.reserve_analysis_work(self.out.len() as u64)?;
            self.reserve(capacity.saturating_sub(self.out.capacity()) as u64)?;
            self.out
                .reserve_exact(capacity.saturating_sub(self.out.len()));
        }
        self.out.extend_from_slice(value);
        Ok(())
    }
    fn number(&mut self, n: u64) -> CompileResult<()> {
        self.bytes(&n.to_be_bytes())
    }
    fn text(&mut self, s: &str) -> CompileResult<()> {
        self.number(s.len() as u64)?;
        self.bytes(s.as_bytes())
    }
    fn symbol(&mut self, symbol: &CtSymbolId) -> CompileResult<()> {
        let key = self.context.symbol_key(symbol, self.budget)?;
        let bytes = key.capacity() as u64;
        self.owned = self.owned.saturating_add(bytes);
        self.number(key.len() as u64)?;
        self.bytes(&key)?;
        drop(key);
        self.release(bytes);
        Ok(())
    }
    fn scalar(&mut self, value: &Constant) -> CompileResult<()> {
        let (tag, bits) = match value {
            Constant::Unit => (0, 0),
            Constant::Bool(v) => (1, u64::from(*v)),
            Constant::U8(v) => (2, u64::from(*v)),
            Constant::U16(v) => (3, u64::from(*v)),
            Constant::U32(v) => (10, *v as u64),
            Constant::I32(v) => (4, *v as u32 as u64),
            Constant::I8(v) => (11, *v as u64),
            Constant::I16(v) => (12, *v as u64),
            Constant::Isize(v) => (13, *v as u64),
            Constant::I64(v) => (5, *v as u64),
            Constant::U64(v) => (6, *v),
            Constant::Usize(v) => (7, *v),
            Constant::F32(v) => (8, u64::from(*v)),
            Constant::F64(v) => (9, *v),
            _ => return Err(bad("contextual scalar is not pure")),
        };
        self.bytes(&[tag])?;
        self.number(bits)
    }
    fn scope_key(&mut self, id: u32) -> CompileResult<Vec<u8>> {
        let instantiated = matches!(
            self.scopes.get(id as usize).map(|s| &s.instance),
            Some(CtScopeInstance::Instantiated(_))
        );
        if !instantiated
            && self
                .scopes
                .get(id as usize)
                .is_some_and(|scope| scope.enclosing.is_none())
        {
            return self.scope_key_inner(id);
        }
        self.budget
            .reserve_analysis_work(self.scope_visiting.len() as u64 + 1)?;
        if self.scope_visiting.contains(&id) {
            return Err(bad("CT instantiated contextual scope cycle"));
        }
        if self.scope_visiting.len() >= self.limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT instantiated contextual scope depth exceeded",
                None,
            )]);
        }
        self.reserve(4)?;
        self.scope_visiting.reserve_exact(1);
        self.scope_visiting.push(id);
        let result = self.scope_key_inner(id);
        self.scope_visiting.pop();
        self.release(4);
        result
    }
    fn scope_key_inner(&mut self, id: u32) -> CompileResult<Vec<u8>> {
        let scope = self
            .scopes
            .get(id as usize)
            .ok_or_else(|| bad("context key has no ambient scope"))?;
        self.context.validate_scope(scope, self.budget)?;
        let outer = std::mem::take(&mut self.out);
        let result = (|| {
            if let Some(parent) = scope.enclosing {
                self.bytes(&[1])?;
                let key = self.scope_key(parent.0)?;
                let bytes = key.capacity() as u64;
                self.number(key.len() as u64)?;
                self.bytes(&key)?;
                drop(key);
                self.release(bytes);
            } else {
                self.bytes(&[0])?;
            }
            self.symbol(&scope.declaration)?;
            self.number(scope.binder_site.len() as u64)?;
            for n in &scope.binder_site {
                self.number(u64::from(*n))?;
            }
            match &scope.instance {
                CtScopeInstance::Definition => self.bytes(&[0])?,
                CtScopeInstance::Instantiated(actuals) => {
                    self.bytes(&[1])?;
                    self.number(actuals.len() as u64)?;
                    for actual in actuals {
                        match actual {
                            CtScopeActual::Type(graph) => {
                                self.bytes(&[0])?;
                                self.graph(graph)?;
                            }
                            CtScopeActual::Scalar(value) => {
                                self.bytes(&[1])?;
                                self.scalar(value)?;
                            }
                            CtScopeActual::Lifetime(reference) => {
                                self.bytes(&[2])?;
                                self.context.validate_reference(
                                    self.scopes,
                                    *reference,
                                    self.budget,
                                )?;
                                let key = self.scope_key(reference.scope.0)?;
                                let bytes = key.capacity() as u64;
                                self.number(key.len() as u64)?;
                                self.bytes(&key)?;
                                self.number(u64::from(reference.parameter))?;
                                drop(key);
                                self.release(bytes);
                            }
                        }
                    }
                }
            }
            let count = self
                .context
                .parameter_count_paid(scope, self.budget)?
                .ok_or_else(|| bad("context key source parameter inventory absent"))?;
            self.budget.reserve_analysis_work(count as u64)?;
            self.number(count as u64)?;
            for ordinal in 0..count {
                match self
                    .context
                    .parameter_kind_paid(scope, ordinal, self.budget)?
                    .ok_or_else(|| bad("context key full parameter ordinal absent"))?
                {
                    zil_verify::CtScopeParameterKind::Type => self.bytes(&[0])?,
                    zil_verify::CtScopeParameterKind::Scalar(ty) => self.bytes(&[1, ty as u8])?,
                    zil_verify::CtScopeParameterKind::Lifetime => self.bytes(&[2])?,
                }
            }
            Ok(())
        })();
        let key = std::mem::replace(&mut self.out, outer);
        result.map(|_| key)
    }
    fn graph(&mut self, graph: &CtTypeRef) -> CompileResult<()> {
        self.budget
            .reserve_analysis_work(graph.nodes.len() as u64)?;
        let references = graph.nodes.iter().fold(0usize, |n, node| {
            n.saturating_add(match node {
                CtTypeNode::Nominal { .. } | CtTypeNode::Opaque { .. } => 1,
                CtTypeNode::BorrowedStorage { life, .. } => life.nominal_arguments.len(),
                _ => 0,
            })
        });
        self.reserve(references as u64 * std::mem::size_of::<(CtSymbolId, Vec<u8>)>() as u64)?;
        let mut symbols: Vec<(CtSymbolId, Vec<u8>)> = Vec::with_capacity(references);
        fn add(
            key: &mut Key<'_, '_>,
            symbols: &mut Vec<(CtSymbolId, Vec<u8>)>,
            symbol: &CtSymbolId,
        ) -> CompileResult<()> {
            key.budget
                .reserve_analysis_work(symbols.len() as u64 * 32)?;
            if symbols.iter().any(|(old, _)| old == symbol) {
                return Ok(());
            }
            let bytes = key.context.symbol_key(symbol, key.budget)?;
            key.owned = key.owned.saturating_add(bytes.capacity() as u64);
            symbols.push((symbol.clone(), bytes));
            Ok(())
        }
        for node in &graph.nodes {
            match node {
                CtTypeNode::Nominal { symbol, .. } | CtTypeNode::Opaque { identity: symbol } => {
                    add(self, &mut symbols, symbol)?
                }
                CtTypeNode::BorrowedStorage { life, .. } => {
                    self.budget
                        .reserve_analysis_work(life.nominal_arguments.len() as u64)?;
                    for row in &life.nominal_arguments {
                        if row.declaration.compiler {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Ct,
                                "compiler nominal metadata needs its authentic generation inventory",
                                None,
                            )]);
                        }
                        add(
                            self,
                            &mut symbols,
                            &CtSymbolId {
                                module: row.declaration.module,
                                declaration: row.declaration.declaration,
                                generation_path: None,
                            },
                        )?;
                    }
                }
                _ => {}
            }
        }
        let symbol_bytes = symbols
            .iter()
            .fold(0u64, |n, (_, key)| n.saturating_add(key.len() as u64));
        self.budget
            .reserve_analysis_work(symbol_bytes.saturating_mul(symbols.len() as u64 + 1))?;
        symbols.sort_by(|a, b| a.1.cmp(&b.1));
        if symbols.windows(2).any(|rows| rows[0].1 == rows[1].1) {
            return Err(bad(
                "different raw symbols resolve to one source declaration identity",
            ));
        }
        self.reserve(
            graph.ambient_classes.len() as u64 * std::mem::size_of::<(u32, Vec<u8>)>() as u64,
        )?;
        let mut scopes: Vec<(u32, Vec<u8>)> = Vec::with_capacity(graph.ambient_classes.len());
        for row in &graph.ambient_classes {
            self.context
                .validate_reference(self.scopes, row.source, self.budget)?;
            self.budget.reserve_analysis_work(scopes.len() as u64)?;
            if !scopes.iter().any(|(id, _)| *id == row.source.scope.0) {
                scopes.push((row.source.scope.0, self.scope_key(row.source.scope.0)?));
            }
        }
        let scope_bytes = scopes
            .iter()
            .fold(0u64, |n, (_, key)| n.saturating_add(key.len() as u64));
        self.budget
            .reserve_analysis_work(scope_bytes.saturating_mul(scopes.len() as u64 + 1))?;
        scopes.sort_by(|a, b| a.1.cmp(&b.1));
        if scopes.windows(2).any(|rows| rows[0].1 == rows[1].1) {
            return Err(bad(
                "ambient scope inventory contains equivalent duplicate rows",
            ));
        }
        let copied_bytes = value_context::graph_cost(graph, self.budget)?;
        self.reserve(copied_bytes)?;
        let mut copied = graph.clone();
        fn replace(
            symbol: &mut CtSymbolId,
            symbols: &[(CtSymbolId, Vec<u8>)],
            budget: &mut CtExecutionBudget,
        ) -> CompileResult<()> {
            budget.reserve_analysis_work(symbols.len() as u64 * 32)?;
            let index = symbols
                .iter()
                .position(|(actual, _)| actual == symbol)
                .ok_or_else(|| bad("semantic symbol disappeared during key relocation"))?;
            *symbol = CtSymbolId {
                module: ModuleId(0),
                declaration: NodeId(
                    u32::try_from(index).map_err(|_| bad("semantic symbol key index overflow"))?,
                ),
                generation_path: None,
            };
            Ok(())
        }
        for node in &mut copied.nodes {
            match node {
                CtTypeNode::Nominal { symbol, .. } | CtTypeNode::Opaque { identity: symbol } => {
                    replace(symbol, &symbols, self.budget)?
                }
                CtTypeNode::BorrowedStorage { life, .. } => {
                    for row in &mut life.nominal_arguments {
                        let mut symbol = CtSymbolId {
                            module: row.declaration.module,
                            declaration: row.declaration.declaration,
                            generation_path: None,
                        };
                        replace(&mut symbol, &symbols, self.budget)?;
                        row.declaration.module = symbol.module;
                        row.declaration.declaration = symbol.declaration;
                    }
                }
                _ => {}
            }
        }
        for row in &mut copied.ambient_classes {
            self.budget.reserve_analysis_work(scopes.len() as u64)?;
            let index = scopes
                .iter()
                .position(|(id, _)| *id == row.source.scope.0)
                .ok_or_else(|| bad("ambient scope disappeared during key relocation"))?;
            row.source.scope = CtAmbientScopeId(
                u32::try_from(index).map_err(|_| bad("ambient key index overflow"))?,
            );
        }
        let normalized = canonicalize_anonymous_type(&copied, self.budget);
        drop(copied);
        self.release(copied_bytes);
        let normalized = normalized?;
        let graph_value = CtValue::Type(normalized);
        let result = (|| {
            self.bytes(&[0x47])?;
            self.number(symbols.len() as u64)?;
            for (_, key) in &symbols {
                self.number(key.len() as u64)?;
                self.bytes(key)?;
            }
            self.number(scopes.len() as u64)?;
            for (_, key) in &scopes {
                self.number(key.len() as u64)?;
                self.bytes(key)?;
            }
            let remaining = self.budget.remaining();
            let available = remaining
                .max_temporary_bytes
                .saturating_sub(self.budget.live_temporary_bytes());
            let cap = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
            let mut wire = WireBudget {
                bytes: self.limits.max_source_bytes.min(cap(available)),
                nodes: self
                    .limits
                    .max_nodes
                    .min(cap(remaining.max_generated_nodes)),
                work: self
                    .limits
                    .max_analysis_steps
                    .min(cap(remaining.max_analysis_work)),
                allocation: self.limits.max_source_bytes.min(cap(available)),
                max_depth: self
                    .limits
                    .max_call_depth
                    .min(remaining.max_call_depth as usize),
            };
            let before = wire;
            // This relocated graph is key data only; it is never executed or
            // presented as an authenticated source candidate.
            let encoded = ct_wire::encode_value_with_usage(&graph_value, self.limits, &mut wire);
            self.budget
                .reserve_analysis_work(before.work.saturating_sub(wire.work) as u64)?;
            self.budget
                .reserve_generated_nodes(before.nodes.saturating_sub(wire.nodes) as u64)?;
            let allocated = before.allocation.saturating_sub(wire.allocation) as u64;
            self.reserve(allocated)?;
            let output = match encoded {
                Ok((bytes, _)) => {
                    self.number(bytes.len() as u64)?;
                    self.bytes(&bytes)
                }
                Err(ct_wire::WireError::Limit) => Err(vec![Diagnostic::limit(
                    Phase::Ct,
                    "contextual graph key exhausted wire reservation",
                    None,
                )]),
                Err(_) => Err(bad("contextual graph key is malformed")),
            };
            self.release(allocated);
            output
        })();
        let CtValue::Type(normalized) = &graph_value else {
            unreachable!()
        };
        self.budget.release_type(normalized);
        drop(graph_value);
        let scratch = symbols.capacity() as u64
            * std::mem::size_of::<(CtSymbolId, Vec<u8>)>() as u64
            + scopes.capacity() as u64 * std::mem::size_of::<(u32, Vec<u8>)>() as u64
            + symbols
                .iter()
                .map(|(_, key)| key.capacity() as u64)
                .sum::<u64>()
            + scopes
                .iter()
                .map(|(_, key)| key.capacity() as u64)
                .sum::<u64>();
        drop(symbols);
        drop(scopes);
        self.release(scratch);
        result
    }
    fn ty(&mut self, ty: &CtTy, depth: usize) -> CompileResult<()> {
        self.depth(depth)?;
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
        self.bytes(&[tag])?;
        match ty {
            CtTy::Scalar(ty) => self.bytes(&[*ty as u8])?,
            CtTy::Syntax(category) => self.bytes(&[*category as u8])?,
            CtTy::Tuple(fields) => {
                self.number(fields.len() as u64)?;
                for ty in fields {
                    self.ty(ty, depth + 1)?;
                }
            }
            CtTy::AnonymousSum(members) => {
                let keys = self.member_keys(members, depth + 1)?;
                self.number(keys.len() as u64)?;
                for key in &keys {
                    self.number(key.len() as u64)?;
                    self.bytes(key)?;
                }
                self.drop_keys(keys);
            }
            CtTy::Array { element, length } => {
                self.number(u64::from(*length))?;
                self.ty(element, depth + 1)?;
            }
            CtTy::List(element) => self.ty(element, depth + 1)?,
            CtTy::Record(id) => {
                let schema = self
                    .records
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("context key record schema absent"))?;
                self.nominal(
                    false,
                    id.0,
                    &schema.name,
                    &schema.arguments,
                    schema.life_context.as_ref(),
                    depth + 1,
                )?;
            }
            CtTy::Enum(id) => {
                let schema = self
                    .enums
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("context key enum schema absent"))?;
                self.nominal(
                    true,
                    id.0,
                    &schema.name,
                    &schema.arguments,
                    schema.life_context.as_ref(),
                    depth + 1,
                )?;
            }
            _ => {}
        };
        Ok(())
    }
    fn nominal(
        &mut self,
        enumeration: bool,
        id: u32,
        symbol: &CtSymbolId,
        args: &[CtSchemaArgument],
        context: Option<&CtTypeRef>,
        depth: usize,
    ) -> CompileResult<()> {
        self.depth(depth)?;
        self.budget
            .reserve_analysis_work(self.visiting.len() as u64)?;
        if self.visiting.contains(&(enumeration, id)) {
            return Err(bad("context key nominal schema is cyclic"));
        }
        if self.visiting.len() == self.visiting.capacity() {
            let capacity = self.visiting.capacity().saturating_mul(2).max(1);
            self.reserve(
                capacity.saturating_sub(self.visiting.capacity()) as u64
                    * std::mem::size_of::<(bool, u32)>() as u64,
            )?;
            self.visiting
                .reserve_exact(capacity.saturating_sub(self.visiting.len()));
        }
        self.visiting.push((enumeration, id));
        self.symbol(symbol)?;
        if let Some(context) = context {
            self.bytes(&[1])?;
            self.graph(context)?;
        } else {
            self.bytes(&[0])?;
            self.number(args.len() as u64)?;
            for arg in args {
                match arg {
                    CtSchemaArgument::Type(graph) => {
                        self.bytes(&[0])?;
                        self.graph(graph)?;
                    }
                    CtSchemaArgument::Scalar(value) => {
                        self.bytes(&[1])?;
                        self.scalar(value)?;
                    }
                    _ => return Err(bad("context key nominal argument lost its joint context")),
                }
            }
        }
        self.visiting.pop();
        Ok(())
    }
    fn member_keys(&mut self, members: &[CtTy], depth: usize) -> CompileResult<Vec<Vec<u8>>> {
        self.reserve(members.len() as u64 * std::mem::size_of::<Vec<u8>>() as u64)?;
        let mut keys = Vec::with_capacity(members.len());
        for ty in members {
            let outer = std::mem::take(&mut self.out);
            let result = self.ty(ty, depth + 1);
            let key = std::mem::replace(&mut self.out, outer);
            result?;
            keys.push(key);
        }
        let bytes = keys.iter().map(|key| key.len() as u64).sum::<u64>();
        self.budget
            .reserve_analysis_work(bytes.saturating_mul(keys.len() as u64 + 1))?;
        keys.sort();
        if keys.windows(2).any(|rows| rows[0] == rows[1]) {
            return Err(bad("context key anonymous members repeat"));
        }
        Ok(keys)
    }
    fn drop_keys(&mut self, keys: Vec<Vec<u8>>) {
        let bytes = keys.capacity() as u64 * std::mem::size_of::<Vec<u8>>() as u64
            + keys.iter().map(|key| key.capacity() as u64).sum::<u64>();
        drop(keys);
        self.release(bytes);
    }
    fn depth(&mut self, depth: usize) -> CompileResult<()> {
        self.budget.reserve_analysis_work(1)?;
        if depth > self.limits.max_call_depth
            || depth > self.budget.remaining().max_call_depth as usize
        {
            Err(vec![Diagnostic::limit(
                Phase::Ct,
                "context key depth exceeded",
                None,
            )])
        } else {
            Ok(())
        }
    }
    fn ident(&mut self, value: &CtIdent, depth: usize) -> CompileResult<()> {
        self.depth(depth)?;
        self.text(&value.text)?;
        self.bytes(&[value.namespace as u8])?;
        match &value.source {
            CtNameSource::Input(id) => {
                self.bytes(&[0])?;
                self.number(u64::from(id.0))?;
            }
            CtNameSource::Definition(symbol) => {
                self.bytes(&[1])?;
                self.symbol(symbol)?;
            }
            CtNameSource::Fresh { invocation, binder } => {
                self.bytes(&[2])?;
                self.bytes(&invocation.0)?;
                self.number(u64::from(*binder))?;
            }
            CtNameSource::Export => self.bytes(&[3])?,
        }
        self.bytes(&[u8::from(value.member_owner.is_some())])?;
        if let Some(owner) = &value.member_owner {
            self.symbol(owner)?;
        }
        self.bytes(&[u8::from(value.member_owner_type.is_some())])?;
        if let Some(owner) = &value.member_owner_type {
            self.graph(owner)?;
        }
        Ok(())
    }
    fn field(&mut self, value: &CtFieldInfo, depth: usize) -> CompileResult<()> {
        self.depth(depth)?;
        self.symbol(&value.owner)?;
        self.graph(&value.owner_type)?;
        self.symbol(&value.field)?;
        self.ident(&value.name, depth + 1)?;
        self.graph(&value.ty)?;
        self.bytes(&[u8::from(value.managed)])
    }
    fn token(&mut self, value: &CtToken, depth: usize) -> CompileResult<()> {
        self.depth(depth)?;
        self.bytes(&[value.class as u8])?;
        self.text(&value.text)?;
        self.number(u64::from(value.origin.0))?;
        self.bytes(&[u8::from(value.name.is_some())])?;
        if let Some(name) = &value.name {
            self.ident(name, depth + 1)?;
        }
        Ok(())
    }
    fn origins(&mut self, origins: &[CtOrigin]) -> CompileResult<()> {
        self.number(origins.len() as u64)?;
        for origin in origins {
            self.bytes(&[origin.role as u8])?;
            self.number(u64::from(origin.span.file.0))?;
            self.number(origin.span.start as u64)?;
            self.number(origin.span.end as u64)?;
            self.bytes(&[u8::from(origin.parent.is_some())])?;
            if let Some(parent) = origin.parent {
                self.number(u64::from(parent.0))?;
            }
        }
        Ok(())
    }
    fn value(&mut self, value: &CtValue, depth: usize) -> CompileResult<()> {
        self.depth(depth)?;
        let tag = match value {
            CtValue::Scalar(_) => 0,
            CtValue::Text(_) => 1,
            CtValue::Type(_) => 2,
            CtValue::Syntax(_) => 3,
            CtValue::Ident(_) => 4,
            CtValue::TokenTree(_) => 5,
            CtValue::TokenCursor(_) => 6,
            CtValue::ShapeInfo(_) => 7,
            CtValue::FieldInfo(_) => 8,
            CtValue::ImplInfo(_) => 9,
            CtValue::LayoutInfo(_) => 10,
            CtValue::Tuple(_) => 11,
            CtValue::Array(_) => 12,
            CtValue::List(_) => 13,
            CtValue::Record { .. } => 14,
            CtValue::Variant { .. } => 15,
            CtValue::AnonymousSum { .. } => 16,
        };
        self.bytes(&[tag])?;
        match value {
            CtValue::Scalar(value) => self.scalar(value)?,
            CtValue::Text(value) => self.text(value)?,
            CtValue::Type(value) => self.graph(value)?,
            CtValue::Ident(value) => self.ident(value, depth + 1)?,
            CtValue::Tuple(v) | CtValue::Array(v) | CtValue::List(v) => {
                self.number(v.len() as u64)?;
                for value in v {
                    self.value(value, depth + 1)?;
                }
            }
            CtValue::Record { schema, fields } => {
                self.ty(&CtTy::Record(*schema), depth + 1)?;
                self.number(fields.len() as u64)?;
                for value in fields {
                    self.value(value, depth + 1)?;
                }
            }
            CtValue::Variant {
                schema,
                tag,
                fields,
            } => {
                self.ty(&CtTy::Enum(*schema), depth + 1)?;
                self.number(u64::from(*tag))?;
                self.number(fields.len() as u64)?;
                for value in fields {
                    self.value(value, depth + 1)?;
                }
            }
            CtValue::AnonymousSum {
                members,
                active,
                payload,
            } => {
                let ty = members
                    .get(*active as usize)
                    .ok_or_else(|| bad("context key anonymous active member is outside range"))?;
                self.bytes(&[16])?;
                let keys = self.member_keys(members, depth + 1)?;
                self.number(keys.len() as u64)?;
                for key in &keys {
                    self.number(key.len() as u64)?;
                    self.bytes(key)?;
                }
                self.drop_keys(keys);
                self.ty(ty, depth + 1)?;
                self.value(payload, depth + 1)?;
            }
            CtValue::ShapeInfo(value) => {
                self.symbol(&value.nominal)?;
                self.bytes(&[u8::from(value.complete)])?;
                self.number(value.visible_fields.len() as u64)?;
                for field in &value.visible_fields {
                    self.field(field, depth + 1)?;
                }
            }
            CtValue::FieldInfo(value) => self.field(value, depth + 1)?,
            CtValue::ImplInfo(value) => {
                self.symbol(&value.trait_symbol)?;
                self.symbol(&value.implementation)?;
                self.graph(&value.for_type)?;
            }
            CtValue::LayoutInfo(value) => {
                self.bytes(&value.target_fingerprint.0)?;
                self.number(value.size)?;
                self.number(value.alignment)?;
                self.number(value.field_offsets.len() as u64)?;
                for n in &value.field_offsets {
                    self.number(*n)?;
                }
            }
            CtValue::Syntax(value) => {
                self.bytes(&[value.category as u8])?;
                self.origins(&value.origins)?;
                self.number(value.nodes.len() as u64)?;
                for node in &value.nodes {
                    match node {
                        CtSyntaxNode::Token(token) => {
                            self.bytes(&[0])?;
                            self.token(token, depth + 1)?;
                        }
                        CtSyntaxNode::Group {
                            delimiter,
                            children,
                            origin,
                        } => {
                            self.bytes(&[1, *delimiter as u8])?;
                            self.number(u64::from(origin.0))?;
                            self.number(children.len() as u64)?;
                            for id in children {
                                self.number(u64::from(id.0))?;
                            }
                        }
                        CtSyntaxNode::Hole { .. } => {
                            return Err(bad("context key evaluated syntax contains a hole"));
                        }
                    }
                }
                self.number(value.roots.len() as u64)?;
                for id in &value.roots {
                    self.number(u64::from(id.0))?;
                }
            }
            CtValue::TokenTree(value) => {
                self.origins(&value.origins)?;
                self.number(value.nodes.len() as u64)?;
                for node in &value.nodes {
                    match node {
                        CtTokenNode::Token(token) => {
                            self.bytes(&[0])?;
                            self.token(token, depth + 1)?;
                        }
                        CtTokenNode::Group {
                            delimiter,
                            children,
                            origin,
                        } => {
                            self.bytes(&[1, *delimiter as u8])?;
                            self.number(u64::from(origin.0))?;
                            self.number(children.len() as u64)?;
                            for id in children {
                                self.number(u64::from(id.0))?;
                            }
                        }
                    }
                }
                self.number(value.roots.len() as u64)?;
                for id in &value.roots {
                    self.number(u64::from(id.0))?;
                }
            }
            CtValue::TokenCursor(value) => {
                self.bytes(&value.tree_fingerprint.0)?;
                self.number(u64::from(value.node.0))?;
            }
        };
        Ok(())
    }
}
pub(super) fn contextual_value_key(
    value: &CtValue,
    records: &[CtRecordSchema],
    enums: &[CtEnumSchema],
    scopes: &[CtAmbientLifeScope],
    context: &FrozenInvocationContext,
    limits: &Limits,
    budget: &mut CtExecutionBudget,
) -> CompileResult<Fingerprint> {
    let mut key = Key {
        context,
        records,
        enums,
        scopes,
        limits,
        budget,
        out: Vec::new(),
        owned: 0,
        visiting: Vec::new(),
        scope_visiting: Vec::new(),
    };
    let result = (|| {
        key.bytes(b"zil/ct-contextual-value/v1\0")?;
        key.value(value, 0)?;
        key.budget.reserve_analysis_work(key.out.len() as u64)?;
        Ok(Fingerprint::of(&key.out))
    })();
    key.budget.release_temporary_bytes(key.owned);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ct_artifact::{compiler::Entry, source_index::SourceIndex};
    use std::{collections::BTreeMap, sync::Arc};
    use zil_base::{FileId, SourceUnit};
    use zil_expand::source_context::{
        self, DeclarationPlacement, ModuleDeclarationOrigins, SourceIdentityContext,
    };
    use zil_ir::{LifeDeclarationId, NominalLifeArgument, StorageBinder, StorageLifeShape};
    use zil_syntax::ParseResult;
    struct Fixture {
        parsed: Vec<Arc<ParseResult>>,
        index: SourceIndex,
        origins: Arc<Vec<ModuleDeclarationOrigins>>,
        module: ModuleId,
    }
    fn budget() -> CtExecutionBudget {
        let limits = Limits::default();
        CtExecutionBudget::new(CtDeclaredBudget {
            max_steps: limits.max_ct_steps,
            max_call_depth: limits.max_call_depth as u32,
            max_live_cells: limits.max_values as u64,
            max_temporary_bytes: limits.max_source_bytes as u64,
            max_generated_nodes: limits.max_nodes as u64,
            max_instances: limits.max_functions as u64,
            max_analysis_work: limits.max_analysis_steps as u64,
        })
    }
    fn fixture(module: u32, shared_span: bool, invalid_const: bool) -> Fixture {
        let text = if invalid_const {
            "struct Phantom<life a>{n:i32} fn first<T:Copy,life a,#N:Missing>() ->i32{return 0} fn second<T:Copy,life a,life b>()->i32{return 0}"
        } else {
            "struct Phantom<life a>{n:i32} fn first<T:Copy,life a,life b>() ->i32{return 0} fn second<T:Copy,life a,life b>()->i32{return 0}"
        };
        fixture_text(module, shared_span, text)
    }
    fn fixture_text(module: u32, shared_span: bool, text: &str) -> Fixture {
        let module = ModuleId(module);
        let sources = vec![SourceUnit::new(
            FileId(module.0 + 17),
            module,
            "ignored/native/path.zil",
            text,
        )];
        let mut parsed = zil_syntax::parse(&sources[0]);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        if shared_span {
            let span = parsed.module.span;
            parsed.module.items[1].span = span;
            parsed.module.items[2].span = span;
            if let zil_syntax::ast::ItemKind::Aggregate(a) = &mut parsed.module.items[2].kind {
                for member in &mut a.methods {
                    member.span = span;
                }
            }
        }
        let parsed = vec![Arc::new(parsed)];
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let paths = BTreeMap::new();
        let (origins, _) = source_context::capture_original(
            &sources,
            &syntax,
            SourceIdentityContext::ClosedSnapshot {
                root: module,
                aliases: &paths,
                declarations: DeclarationPlacement::Original,
            },
            &Limits::default(),
        )
        .unwrap();
        let index = SourceIndex::freeze(&sources, &syntax, &paths).unwrap();
        Fixture {
            parsed,
            index,
            origins: Arc::new(origins.expect("phantom metadata needs original identities")),
            module,
        }
    }
    fn symbol(fixture: &Fixture, ordinal: u32) -> CtSymbolId {
        CtSymbolId {
            module: fixture.module,
            declaration: NodeId(ordinal),
            generation_path: None,
        }
    }
    fn context(fixture: &Fixture, ordinal: u32) -> Arc<FrozenInvocationContext> {
        FrozenInvocationContext::freeze(
            &Entry::Function(symbol(fixture, ordinal)),
            &fixture.parsed,
            &fixture.index,
            Some(&fixture.origins),
            &mut budget(),
        )
        .unwrap()
        .unwrap()
    }
    fn graph(fixture: &Fixture, parameter: u32) -> CtTypeRef {
        CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![
                CtTypeNode::Nominal {
                    symbol: symbol(fixture, 0),
                    arguments: vec![CtTypeArgument::Lifetime(CtLifetimeArgument {
                        owner: CtTypeNodeId(1),
                        binder: StorageBinder::Fixed(0),
                    })],
                },
                CtTypeNode::BorrowedStorage {
                    storage: CtTypeNodeId(0),
                    life: StorageLifeShape {
                        binder_count: 1,
                        slots: vec![],
                        retained: vec![],
                        nominal_arguments: vec![NominalLifeArgument {
                            path: vec![],
                            declaration: LifeDeclarationId {
                                module: fixture.module,
                                declaration: NodeId(0),
                                compiler: false,
                            },
                            argument: 0,
                            binder: 0,
                        }],
                    },
                },
            ],
            root: CtTypeNodeId(1),
            ambient_classes: vec![CtAmbientClassBinding {
                owner: CtTypeNodeId(1),
                class: 0,
                source: CtAmbientLifeReference {
                    scope: CtAmbientScopeId(0),
                    parameter,
                },
            }],
        }
    }
    fn scopes(fixture: &Fixture, ordinal: u32) -> Vec<CtAmbientLifeScope> {
        vec![CtAmbientLifeScope {
            enclosing: None,
            declaration: symbol(fixture, ordinal),
            binder_site: vec![],
            instance: CtScopeInstance::Definition,
        }]
    }
    fn key(value: &CtValue, fixture: &Fixture, ordinal: u32) -> CompileResult<Fingerprint> {
        let context = context(fixture, ordinal);
        let mut meter = budget();
        let result = contextual_value_key(
            value,
            &[],
            &[],
            &scopes(fixture, ordinal),
            &context,
            &Limits::default(),
            &mut meter,
        );
        assert_eq!(meter.live_temporary_bytes(), 0);
        result
    }
    #[test]
    fn ct5_contextual_identity_uses_actual_owner_full_ordinal_and_source_relocation() {
        let original = fixture(7, false, false);
        let shared = fixture(7, true, false);
        let relocated = fixture(91, false, false);
        let first = CtValue::Type(graph(&original, 1));
        let same_local_other = CtValue::Type(graph(&original, 2));
        assert_ne!(
            key(&first, &original, 1).unwrap(),
            key(&first, &original, 2).unwrap()
        );
        assert_ne!(
            key(&first, &original, 1).unwrap(),
            key(&same_local_other, &original, 1).unwrap()
        );
        assert_eq!(
            key(&first, &original, 1).unwrap(),
            key(&CtValue::Type(graph(&relocated, 1)), &relocated, 1).unwrap()
        );
        assert_eq!(
            key(&first, &shared, 1).unwrap(),
            key(&first, &original, 1).unwrap()
        );
        assert_ne!(
            key(&first, &shared, 1).unwrap(),
            key(&first, &shared, 2).unwrap()
        );
        let context = context(&original, 1);
        assert!(
            contextual_value_key(
                &first,
                &[],
                &[],
                &scopes(&original, 2),
                &context,
                &Limits::default(),
                &mut budget()
            )
            .is_err()
        );
        assert!(
            key(&CtValue::Type(graph(&original, 0)), &original, 1).is_err(),
            "a Type parameter is not a Lifetime full ordinal"
        );
    }
    #[test]
    fn ct5_member_context_keeps_full_ordinal_siblings_relocation_and_shared_spans() {
        let text = "struct Phantom<life a>{n:i32} fn padding(){} struct Reader{n:i32;fn first<T:Copy,life a,life b>()->i32{return 0}fn second<T:Copy,life a,life b>()->i32{return 0}}";
        let original = fixture_text(7, false, text);
        let shared = fixture_text(7, true, text);
        let relocated = fixture_text(91, false, text);
        // Top rows 0..3; Phantom.n=3, Reader.n=4, methods=5,6.
        let value = CtValue::Type(graph(&original, 1));
        let first = key(&value, &original, 5).unwrap();
        assert_ne!(first, key(&value, &original, 6).unwrap());
        assert_ne!(
            first,
            key(&CtValue::Type(graph(&original, 2)), &original, 5).unwrap()
        );
        assert_eq!(first, key(&value, &shared, 5).unwrap());
        assert_eq!(
            first,
            key(&CtValue::Type(graph(&relocated, 1)), &relocated, 5).unwrap()
        );
        assert!(key(&CtValue::Type(graph(&original, 0)), &original, 5).is_err());
        for fixture in [&original, &shared] {
            for (ordinal, expected) in [(0, 5), (1, 6)] {
                assert_eq!(
                    super::super::compiler::source_scope::original_member(
                        &fixture.parsed[0].module,
                        NodeId(2),
                        ordinal,
                        Limits::default().max_call_depth,
                        |_| Ok(()),
                    )
                    .unwrap(),
                    Some(NodeId(expected))
                );
            }
        }
        let context = context(&original, 5);
        assert!(
            contextual_value_key(
                &value,
                &[],
                &[],
                &scopes(&original, 6),
                &context,
                &Limits::default(),
                &mut budget()
            )
            .is_err()
        );
    }
    #[test]
    fn ct5_contextual_descriptors_keep_nested_owner_context_and_budget_neighbors() {
        let fixture = fixture(7, false, false);
        let owner = symbol(&fixture, 0);
        let owner_type = graph(&fixture, 1);
        let value = CtValue::FieldInfo(CtFieldInfo {
            owner: owner.clone(),
            owner_type: owner_type.clone(),
            field: symbol(&fixture, 3),
            name: CtIdent {
                text: "n".into(),
                namespace: CtNameNamespace::Member,
                source: CtNameSource::Definition(symbol(&fixture, 3)),
                member_owner: Some(owner),
                member_owner_type: Some(owner_type),
            },
            ty: CtTypeRef {
                scoped_catalogue: None,
                nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                root: CtTypeNodeId(0),
                ambient_classes: vec![],
            },
            managed: false,
        });
        assert_ne!(
            key(&value, &fixture, 1).unwrap(),
            key(&value, &fixture, 2).unwrap()
        );
        let context = context(&fixture, 1);
        let scopes = scopes(&fixture, 1);
        let limits = Limits::default();
        let mut first = budget();
        let expected =
            contextual_value_key(&value, &[], &[], &scopes, &context, &limits, &mut first).unwrap();
        let exact = first.usage().analysis_work;
        assert_eq!(first.live_temporary_bytes(), 0);
        for (work, good) in [(exact, true), (exact - 1, false), (exact, true)] {
            let mut meter = CtExecutionBudget::new(CtDeclaredBudget {
                max_analysis_work: work,
                ..budget().remaining()
            });
            let result =
                contextual_value_key(&value, &[], &[], &scopes, &context, &limits, &mut meter);
            if good {
                assert_eq!(result.unwrap(), expected);
            } else {
                assert_eq!(result.unwrap_err()[0].code, "E-RESOURCE-LIMIT");
            }
            assert_eq!(meter.live_temporary_bytes(), 0);
        }
        eprintln!("CT5 contextual descriptor work boundary {exact}");
    }
    #[test]
    fn ct5_context_freeze_and_symbol_key_failures_release_only_their_allocations() {
        let invalid = fixture(7, false, true);
        let mut meter = budget();
        meter.reserve_temporary_bytes(37).unwrap();
        let before = meter.live_temporary_bytes();
        assert!(
            FrozenInvocationContext::freeze(
                &Entry::Function(symbol(&invalid, 1)),
                &invalid.parsed,
                &invalid.index,
                Some(&invalid.origins),
                &mut meter
            )
            .is_err()
        );
        assert_eq!(meter.live_temporary_bytes(), before);
        let fixture = fixture(7, false, false);
        let context = context(&fixture, 1);
        let mut first = budget();
        let output = context
            .symbol_key(&symbol(&fixture, 3), &mut first)
            .unwrap();
        let exact = first.usage().analysis_work;
        first.release_temporary_bytes(output.capacity() as u64);
        drop(output);
        assert_eq!(first.live_temporary_bytes(), 0);
        let mut below = CtExecutionBudget::new(CtDeclaredBudget {
            max_analysis_work: exact - 1,
            ..budget().remaining()
        });
        below.reserve_temporary_bytes(37).unwrap();
        assert!(
            context
                .symbol_key(&symbol(&fixture, 3), &mut below)
                .is_err()
        );
        assert!(below.usage().peak_temporary_bytes > 37);
        assert_eq!(below.live_temporary_bytes(), 37);
    }
}
