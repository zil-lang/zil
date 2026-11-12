//! Borrowed, bounded transport inventory for raw runtime dyn tables. This
//! grants no CT execution, type, contract, or borrowing authority.
use super::*;

fn quota() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "Core dyn metadata transport budget exceeded",
        None,
    )]
}

struct Walk<'a> {
    limits: &'a Limits,
    nodes: usize,
    work: usize,
}
impl Walk<'_> {
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        // Type trees are cloned recursively by Core. Keep the transport depth
        // below a fixed stack-safe ceiling even when a caller raises its limit.
        if depth > self.limits.max_call_depth.min(128) || self.nodes >= self.limits.max_nodes {
            return Err(quota());
        }
        self.nodes += 1;
        self.work = self.work.checked_sub(1).ok_or_else(quota)?;
        Ok(())
    }
    fn text(&mut self, value: &str) -> CompileResult<()> {
        self.work = self.work.checked_sub(value.len()).ok_or_else(quota)?;
        Ok(())
    }
    fn path(&mut self, length: usize, depth: usize) -> CompileResult<()> {
        if depth.saturating_add(length) > self.limits.max_call_depth {
            return Err(quota());
        }
        for _ in 0..length {
            self.node(depth)?;
        }
        Ok(())
    }
    fn ty(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        let bounded = Limits {
            max_call_depth: self.limits.max_call_depth.min(128),
            ..self.limits.clone()
        };
        hint_type(ty, depth, &mut self.nodes, &mut self.work, &bounded).map(|_| ())
    }
    fn constant(&mut self, value: &Constant, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if let Constant::WideString(units) = value {
            self.path(units.len(), depth + 1)?;
        }
        Ok(())
    }
    fn identity(&mut self, id: &LifeDeclarationIdentity, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        self.text(&id.package)?;
        for part in id.module_path.iter().chain(&id.declaration_path) {
            self.node(depth + 1)?;
            self.text(part)?;
        }
        self.path(id.generation_path.len(), depth + 1)
    }
    fn parameter(&mut self, p: &LifeParameterKind, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if let LifeParameterKind::Const(ty) = p {
            self.ty(ty, depth + 1)?;
        }
        Ok(())
    }
    fn self_pattern(&mut self, pattern: &DynSelfPattern, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        for actual in &pattern.arguments {
            self.node(depth + 1)?;
            match actual {
                DynPatternActual::Type(ty) => self.pattern_type(ty, depth + 2)?,
                DynPatternActual::Const(value) => self.pattern_const(value, depth + 2)?,
                DynPatternActual::LifeParameter(_) => {}
            }
        }
        Ok(())
    }
    fn pattern_const(&mut self, value: &DynConstPattern, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if let DynConstPattern::Value(value) = value {
            self.constant(value, depth + 1)?;
        }
        Ok(())
    }
    fn pattern_type(&mut self, value: &DynTypePattern, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        match value {
            DynTypePattern::Concrete(value) => self.ty(value, depth + 1)?,
            DynTypePattern::Nominal(value) => self.self_pattern(value, depth + 1)?,
            DynTypePattern::Parameter(_) => {}
            DynTypePattern::Address { inner, .. }
            | DynTypePattern::Slice(inner)
            | DynTypePattern::Managed(inner)
            | DynTypePattern::Desc(inner)
            | DynTypePattern::OwnedStore(inner) => self.pattern_type(inner, depth + 1)?,
            DynTypePattern::Array { element, length } => {
                self.pattern_const(length, depth + 1)?;
                self.pattern_type(element, depth + 1)?;
            }
            DynTypePattern::Tuple(values) => {
                for value in values {
                    self.pattern_type(value, depth + 1)?;
                }
            }
            DynTypePattern::ErrorUnion { error, result } => {
                self.pattern_type(error, depth + 1)?;
                self.pattern_type(result, depth + 1)?;
            }
        }
        Ok(())
    }
    fn actual(&mut self, a: &DynGenericActual, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        match a {
            DynGenericActual::Type(ty) => self.ty(ty, depth + 1),
            DynGenericActual::Const(c) => self.constant(c, depth + 1),
            DynGenericActual::Life(_) => Ok(()),
        }
    }
    fn application(&mut self, a: &DynTraitApplication, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        for actual in &a.actuals {
            self.actual(actual, depth + 1)?;
        }
        Ok(())
    }
    fn member(&mut self, m: &DynMember, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        self.application(&m.application, depth + 1)
    }
    fn position(&mut self, p: &StoragePosition, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        self.path(p.path.len(), depth + 1)
    }
    fn storage_life(&mut self, life: &StorageLifeContract, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        for binding in &life.bindings {
            self.node(depth + 1)?;
            self.position(&binding.position, depth + 2)?;
        }
        for binding in &life.classes {
            self.node(depth + 1)?;
            self.path(binding.position.path.len(), depth + 2)?;
        }
        Ok(())
    }
    fn retained(&mut self, life: &RetainedLifeShape, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        for port in &life.ports {
            self.node(depth + 1)?;
            self.ty(&port.leaf, depth + 2)?;
        }
        for binding in &life.retained {
            self.node(depth + 1)?;
            self.path(binding.path.len(), depth + 2)?;
        }
        for binding in &life.nominal_arguments {
            self.node(depth + 1)?;
            self.path(binding.path.len(), depth + 2)?;
        }
        Ok(())
    }
    fn requirement(&mut self, r: &DynRequirement, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        match r {
            DynRequirement::Read(p) | DynRequirement::Write(p) | DynRequirement::Manage(p) => {
                self.position(p, depth + 1)
            }
            DynRequirement::Apart(a, b) => {
                self.position(a, depth + 1)?;
                self.position(b, depth + 1)
            }
            DynRequirement::State { position, ty, .. } => {
                self.position(position, depth + 1)?;
                self.ty(ty, depth + 1)
            }
        }
    }
    fn guarantee(&mut self, g: &DynGuarantee, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        match g {
            DynGuarantee::State { position, ty, .. } => {
                self.position(position, depth + 1)?;
                self.ty(ty, depth + 1)?;
            }
            DynGuarantee::StorageSources(binding) => {
                self.position(&binding.position, depth + 1)?;
                for source in &binding.sources {
                    self.position(source, depth + 1)?;
                }
            }
            DynGuarantee::ManageResult => {}
        }
        Ok(())
    }
    fn effect(&mut self, e: &DynEffect, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        match e {
            DynEffect::Call(p) => self.position(p, depth + 1),
            DynEffect::Cleanup(ty) => self.ty(ty, depth + 1),
            _ => Ok(()),
        }
    }
    fn access(&mut self, a: &DynCallAccess, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        for p in a.reads.iter().chain(&a.writes) {
            self.position(p, depth + 1)?;
        }
        Ok(())
    }
    fn result(&mut self, r: &DynResultSource, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if let DynResultSource::Borrow { sources, .. } = r {
            for p in sources {
                self.position(p, depth + 1)?;
            }
        }
        Ok(())
    }
    fn cleanup(&mut self, c: &DynCleanupContract, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        for p in &c.positions {
            self.position(p, depth + 1)?;
        }
        self.access(&c.access, depth + 1)?;
        for e in &c.effects {
            self.effect(e, depth + 1)?;
        }
        Ok(())
    }
    fn contract(&mut self, c: &DynContract, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if let Some(rows) = &c.requires {
            self.node(depth + 1)?;
            for r in rows {
                self.requirement(r, depth + 2)?;
            }
        }
        if let Some(rows) = &c.guarantees {
            self.node(depth + 1)?;
            for g in rows {
                self.guarantee(g, depth + 2)?;
            }
        }
        if let Some(rows) = &c.hold {
            self.node(depth + 1)?;
            for p in rows {
                self.position(p, depth + 2)?;
            }
        }
        if let Some(a) = &c.call {
            self.access(a, depth + 1)?;
        }
        if let Some(r) = &c.result {
            self.result(r, depth + 1)?;
        }
        if let Some(c) = &c.cleanup {
            self.cleanup(c, depth + 1)?;
        }
        if let Some(rows) = &c.effects {
            self.node(depth + 1)?;
            for e in rows {
                self.effect(e, depth + 2)?;
            }
        }
        if c.resume.is_some() {
            self.node(depth + 1)?;
        }
        if c.threads.is_some() {
            self.node(depth + 1)?;
        }
        Ok(())
    }
}

pub(super) fn inventory(core: &CoreModule, limits: &Limits) -> CompileResult<(usize, usize)> {
    let mut w = Walk {
        limits,
        nodes: 0,
        work: limits.max_analysis_steps,
    };
    for function in &core.functions {
        w.node(0)?;
        if let Some(bindings) = &function.retained_return_sources {
            for binding in bindings {
                w.node(1)?;
                w.position(&binding.position, 2)?;
                for source in &binding.sources {
                    w.position(source, 2)?;
                }
            }
        }
    }
    for declaration in &core.dyn_declarations {
        w.node(1)?;
        if let Some(pattern) = &declaration.self_pattern {
            w.self_pattern(pattern, 2)?;
        }
        w.node(1)?;
        if let Some(pattern) = &declaration.implemented_trait {
            w.self_pattern(pattern, 2)?;
        }
        w.node(0)?;
        w.identity(&declaration.identity, 1)?;
        for p in &declaration.parameters {
            w.parameter(p, 1)?;
        }
        for m in &declaration.members {
            w.node(1)?;
            w.text(&m.name)?;
            match &m.kind {
                DynMemberKind::Type => {}
                DynMemberKind::Constant(ty) => w.ty(ty, 2)?,
                DynMemberKind::Method(parameters) => {
                    for p in parameters {
                        w.parameter(p, 2)?;
                    }
                }
            }
        }
    }
    for interface in &core.dyn_interfaces {
        w.node(0)?;
        w.node(1)?;
        if let Some(context) = &interface.context {
            w.ty(context, 2)?;
        }
        w.node(1)?;
        if let Some(cleanup) = &interface.owning_cleanup {
            w.cleanup(cleanup, 2)?;
        }
        w.application(&interface.application, 1)?;
        for app in &interface.inherited {
            w.application(app, 1)?;
        }
        for assoc in &interface.associated_types {
            w.node(1)?;
            w.member(&assoc.member, 2)?;
            w.ty(&assoc.ty, 2)?;
        }
        for assoc in &interface.associated_constants {
            w.node(1)?;
            w.member(&assoc.member, 2)?;
            w.ty(&assoc.ty, 2)?;
            w.constant(&assoc.value, 2)?;
        }
        w.retained(&interface.retained_life, 1)?;
        for slot in &interface.slots {
            w.node(1)?;
            w.member(&slot.member, 2)?;
            for actual in &slot.actuals {
                w.actual(actual, 2)?;
            }
            for ty in &slot.parameters {
                w.ty(ty, 2)?;
            }
            w.path(slot.parameter_modes.len(), 2)?;
            w.ty(&slot.result, 2)?;
            if let Some(life) = &slot.storage_life {
                w.storage_life(life, 2)?;
            }
            w.contract(&slot.contract, 2)?;
        }
    }
    for table in &core.dyn_vtables {
        w.node(0)?;
        w.ty(&table.self_type, 1)?;
        w.path(table.self_classes.len(), 1)?;
        for actual in &table.actuals {
            w.actual(actual, 1)?;
        }
        for port in &table.ports {
            w.node(1)?;
            for source in &port.sources {
                w.position(source, 2)?;
            }
        }
        w.path(table.entries.len(), 1)?;
        if table.cleanup.is_some() {
            w.node(1)?;
        }
    }
    Ok((w.nodes, limits.max_analysis_steps - w.work))
}
