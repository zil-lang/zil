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
        if let DynGuarantee::State { position, ty, .. } = g {
            self.position(position, depth + 1)?;
            self.ty(ty, depth + 1)?;
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
    if core.dyn_declarations.is_empty()
        && core.dyn_interfaces.is_empty()
        && core.dyn_vtables.is_empty()
    {
        return Ok((0, 0));
    }
    let mut w = Walk {
        limits,
        nodes: 0,
        work: limits.max_analysis_steps,
    };
    for declaration in &core.dyn_declarations {
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
        for actual in &table.actuals {
            w.actual(actual, 1)?;
        }
        for port in &table.ports {
            w.node(1)?;
            w.position(&port.source, 2)?;
        }
        w.path(table.entries.len(), 1)?;
        if table.cleanup.is_some() {
            w.node(1)?;
        }
    }
    Ok((w.nodes, limits.max_analysis_steps - w.work))
}
