//! Bounded Core dyn transport before any owned metadata snapshot.
use super::*;
struct Work<'a, 'b> {
    limits: &'a Limits,
    left: &'b mut usize,
    copies: usize,
    nodes: usize,
}
impl Work<'_, '_> {
    fn fee(&mut self, n: usize) -> R<()> {
        let n = n
            .checked_mul(self.copies)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "dyn metadata work overflow", None))?;
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "dyn metadata snapshot budget exceeded", None)
        })?;
        Ok(())
    }
    fn rows(&mut self, n: usize) -> R<()> {
        self.nodes = self
            .nodes
            .checked_add(n)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "dyn metadata node overflow", None))?;
        if self.nodes > self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "dyn metadata node limit exceeded",
                None,
            ));
        }
        self.fee(n.saturating_add(1))
    }
    fn path(&mut self, n: usize) -> R<()> {
        if n > self.limits.max_call_depth.min(128) {
            return Err(Diagnostic::limit(
                Phase::Types,
                "dyn metadata path depth exceeded",
                None,
            ));
        }
        self.rows(n)
    }
    fn ty(&mut self, ty: &Type) -> R<()> {
        inference::charge_type_snapshot(
            ty,
            Span::default(),
            &Limits {
                max_call_depth: self.limits.max_call_depth.min(128),
                ..self.limits.clone()
            },
            self.left,
            self.copies,
        )
    }
    fn constant(&mut self, c: &Constant) -> R<()> {
        self.rows(1)?;
        if let Constant::WideString(units) = c {
            self.rows(units.len())?;
        }
        Ok(())
    }
    fn actual(&mut self, a: &DynGenericActual) -> R<()> {
        self.rows(1)?;
        match a {
            DynGenericActual::Type(ty) => self.ty(ty)?,
            DynGenericActual::Const(c) => self.constant(c)?,
            DynGenericActual::Life(_) => {}
        }
        Ok(())
    }
    fn application(&mut self, a: &DynTraitApplication) -> R<()> {
        self.rows(a.actuals.len())?;
        for a in &a.actuals {
            self.actual(a)?;
        }
        Ok(())
    }
    fn position(&mut self, p: &StoragePosition) -> R<()> {
        self.path(p.path.len())
    }
    fn access(&mut self, a: &DynCallAccess) -> R<()> {
        self.rows(a.reads.len().saturating_add(a.writes.len()))?;
        for p in a.reads.iter().chain(&a.writes) {
            self.position(p)?;
        }
        Ok(())
    }
    fn effects(&mut self, e: &[DynEffect]) -> R<()> {
        self.rows(e.len())?;
        for e in e {
            match e {
                DynEffect::Call(p) => self.position(p)?,
                DynEffect::Cleanup(t) => self.ty(t)?,
                _ => {}
            }
        }
        Ok(())
    }
    fn cleanup(&mut self, c: &DynCleanupContract) -> R<()> {
        self.rows(c.positions.len())?;
        for p in &c.positions {
            self.position(p)?;
        }
        self.access(&c.access)?;
        self.effects(&c.effects)
    }
    fn contract(&mut self, c: &DynContract) -> R<()> {
        self.rows(9)?;
        if let Some(rows) = &c.requires {
            self.rows(rows.len())?;
            for r in rows {
                match r {
                    DynRequirement::Read(p)
                    | DynRequirement::Write(p)
                    | DynRequirement::Manage(p) => self.position(p)?,
                    DynRequirement::Apart(a, b) => {
                        self.position(a)?;
                        self.position(b)?;
                    }
                    DynRequirement::State { position, ty, .. } => {
                        self.position(position)?;
                        self.ty(ty)?;
                    }
                }
            }
        }
        if let Some(rows) = &c.guarantees {
            self.rows(rows.len())?;
            for r in rows {
                match r {
                    DynGuarantee::State { position, ty, .. } => {
                        self.position(position)?;
                        self.ty(ty)?;
                    }
                    DynGuarantee::StorageSources(binding) => {
                        self.position(&binding.position)?;
                        self.rows(binding.sources.len())?;
                        for source in &binding.sources {
                            self.position(crate::exact_storage_source(source)?)?;
                        }
                    }
                    DynGuarantee::ManageResult => {}
                }
            }
        }
        if let Some(rows) = &c.hold {
            self.rows(rows.len())?;
            for p in rows {
                self.position(p)?;
            }
        }
        if let Some(a) = &c.call {
            self.access(a)?;
        }
        if let Some(DynResultSource::Borrow { sources, .. }) = &c.result {
            self.rows(sources.len())?;
            for p in sources {
                self.position(crate::exact_storage_source(p)?)?;
            }
        }
        if let Some(c) = &c.cleanup {
            self.cleanup(c)?;
        }
        if let Some(e) = &c.effects {
            self.effects(e)?;
        }
        Ok(())
    }
    fn pattern(&mut self, root: &DynSelfPattern) -> R<()> {
        enum Node<'a> {
            SelfType(&'a DynSelfPattern),
            Actual(&'a DynPatternActual),
            Ty(&'a DynTypePattern),
            Const(&'a DynConstPattern),
        }
        let mut queue = vec![(Node::SelfType(root), 0usize)];
        while let Some((node, depth)) = queue.pop() {
            self.rows(1)?;
            if depth >= self.limits.max_call_depth.min(128) {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "dyn Self recipe depth exceeded",
                    None,
                ));
            }
            match node {
                Node::SelfType(p) => {
                    self.rows(p.arguments.len())?;
                    for a in &p.arguments {
                        queue.push((Node::Actual(a), depth + 1));
                    }
                }
                Node::Actual(DynPatternActual::Type(t)) => queue.push((Node::Ty(t), depth + 1)),
                Node::Actual(DynPatternActual::Const(c)) => queue.push((Node::Const(c), depth + 1)),
                Node::Actual(DynPatternActual::LifeParameter(_)) => {}
                Node::Const(DynConstPattern::Value(v)) => self.constant(v)?,
                Node::Const(_) => {}
                Node::Ty(t) => match t {
                    DynTypePattern::Concrete(t) => self.ty(t)?,
                    DynTypePattern::Parameter(_) => {}
                    DynTypePattern::Nominal(p) => queue.push((Node::SelfType(p), depth + 1)),
                    DynTypePattern::Address { inner, .. }
                    | DynTypePattern::Managed(inner)
                    | DynTypePattern::Desc(inner)
                    | DynTypePattern::Slice(inner)
                    | DynTypePattern::OwnedStore(inner) => queue.push((Node::Ty(inner), depth + 1)),
                    DynTypePattern::Array { element, length } => {
                        queue.push((Node::Ty(element), depth + 1));
                        queue.push((Node::Const(length), depth + 1));
                    }
                    DynTypePattern::Tuple(ts) => {
                        self.rows(ts.len())?;
                        for t in ts {
                            queue.push((Node::Ty(t), depth + 1));
                        }
                    }
                    DynTypePattern::ErrorUnion { error, result } => {
                        queue.push((Node::Ty(error), depth + 1));
                        queue.push((Node::Ty(result), depth + 1));
                    }
                },
            }
        }
        Ok(())
    }
}
pub(super) fn snapshot(
    core: &CoreModule,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    if core.dyn_declarations.is_empty()
        && core.dyn_interfaces.is_empty()
        && core.dyn_vtables.is_empty()
    {
        return Ok(());
    }
    let mut w = Work {
        limits,
        left,
        copies,
        nodes: 0,
    };
    w.rows(
        core.dyn_declarations
            .len()
            .saturating_add(core.dyn_interfaces.len())
            .saturating_add(core.dyn_vtables.len()),
    )?;
    for d in &core.dyn_declarations {
        w.fee(d.identity.package.len())?;
        w.rows(
            d.identity
                .module_path
                .len()
                .saturating_add(d.identity.declaration_path.len())
                .saturating_add(d.identity.generation_path.len()),
        )?;
        for text in d
            .identity
            .module_path
            .iter()
            .chain(&d.identity.declaration_path)
        {
            w.fee(text.len())?;
        }
        w.rows(1)?;
        if let Some(p) = &d.self_pattern {
            w.pattern(p)?;
        }
        w.rows(1)?;
        if let Some(p) = &d.implemented_trait {
            w.pattern(p)?;
        }
        w.rows(d.parameters.len().saturating_add(d.members.len()))?;
        for p in &d.parameters {
            if let LifeParameterKind::Const(t) = p {
                w.ty(t)?;
            }
        }
        for m in &d.members {
            w.fee(m.name.len())?;
            match &m.kind {
                DynMemberKind::Type => {}
                DynMemberKind::Constant(t) => w.ty(t)?,
                DynMemberKind::Method(ps) => {
                    w.rows(ps.len())?;
                    for p in ps {
                        if let LifeParameterKind::Const(t) = p {
                            w.ty(t)?;
                        }
                    }
                }
            }
        }
    }
    for i in &core.dyn_interfaces {
        w.rows(2)?;
        if let Some(t) = &i.context {
            w.ty(t)?;
        }
        if let Some(c) = &i.owning_cleanup {
            w.cleanup(c)?;
        }
        w.application(&i.application)?;
        w.rows(i.inherited.len())?;
        for a in &i.inherited {
            w.application(a)?;
        }
        w.rows(
            i.associated_types
                .len()
                .saturating_add(i.associated_constants.len()),
        )?;
        for a in &i.associated_types {
            w.application(&a.member.application)?;
            w.ty(&a.ty)?;
        }
        for a in &i.associated_constants {
            w.application(&a.member.application)?;
            w.ty(&a.ty)?;
            w.constant(&a.value)?;
        }
        w.rows(
            i.retained_life
                .ports
                .len()
                .saturating_add(i.retained_life.retained.len())
                .saturating_add(i.retained_life.nominal_arguments.len()),
        )?;
        for p in &i.retained_life.ports {
            w.ty(&p.leaf)?;
        }
        for p in &i.retained_life.retained {
            w.path(p.path.len())?;
        }
        for p in &i.retained_life.nominal_arguments {
            w.path(p.path.len())?;
        }
        w.rows(i.slots.len())?;
        for s in &i.slots {
            w.application(&s.member.application)?;
            w.rows(
                s.actuals
                    .len()
                    .saturating_add(s.parameters.len())
                    .saturating_add(s.parameter_modes.len()),
            )?;
            for a in &s.actuals {
                w.actual(a)?;
            }
            for t in &s.parameters {
                w.ty(t)?;
            }
            w.ty(&s.result)?;
            if let Some(c) = &s.storage_life {
                w.fee(borrowed_fp::metadata_work(c))?;
                for r in &c.bindings {
                    w.path(r.position.path.len())?;
                }
                for r in &c.classes {
                    w.path(r.position.path.len())?;
                }
            }
            w.contract(&s.contract)?;
        }
    }
    for t in &core.dyn_vtables {
        w.ty(&t.self_type)?;
        w.rows(
            t.self_classes
                .len()
                .saturating_add(t.actuals.len())
                .saturating_add(t.ports.len())
                .saturating_add(t.entries.len()),
        )?;
        for a in &t.actuals {
            w.actual(a)?;
        }
        for p in &t.ports {
            w.rows(p.sources.len())?;
            for p in &p.sources {
                w.position(p)?;
            }
        }
        w.rows(2)?;
    }
    Ok(())
}
