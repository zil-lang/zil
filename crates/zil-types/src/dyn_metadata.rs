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
            DynGenericActual::QualifiedType(value) => value.visit_dyn_metadata(self, 0)?,
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
        root.visit_dyn_metadata(self, 0)
    }
}
impl Work<'_, '_> {
    fn declaration(&mut self, d: &DynDeclaration) -> R<()> {
        self.rows(1)?;
        if let Some(values) = &d.method_definitions {
            self.rows(values.len())?;
            for value in values {
                value.visit_dyn_metadata(self, 0)?;
            }
        }
        self.rows(1)?;
        if let Some(values) = &d.method_signatures {
            self.rows(values.len())?;
            for value in values {
                value.visit_dyn_metadata(self, 0)?;
            }
        }
        self.rows(1)?;
        if let Some(values) = &d.associated_definitions {
            self.rows(values.len())?;
            for value in values {
                value.visit_dyn_metadata(self, 0)?;
            }
        }
        self.rows(1)?;
        if let Some(values) = &d.requirements {
            self.rows(values.len())?;
            for value in values {
                value.visit_dyn_metadata(self, 0)?;
            }
        }
        self.rows(1)?;
        if let Some(values) = &d.implicit_lifetimes {
            self.rows(values.len())?;
            for value in values {
                value.visit_dyn_metadata(self, 0)?;
            }
        }
        self.rows(1)?;
        if let Some(values) = &d.direct_supertraits {
            self.rows(values.len())?;
            for value in values {
                value.visit_dyn_metadata(self, 0)?;
            }
        }
        self.fee(d.identity.package.len())?;
        self.rows(
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
            self.fee(text.len())?;
        }
        self.rows(1)?;
        if let Some(p) = &d.self_pattern {
            p.visit_dyn_metadata(self, 0)?;
        }
        self.rows(1)?;
        if let Some(p) = &d.implemented_trait {
            self.pattern(p)?;
        }
        self.rows(d.parameters.len().saturating_add(d.members.len()))?;
        for p in &d.parameters {
            if let LifeParameterKind::Const(t) = p {
                self.ty(t)?;
            }
        }
        for m in &d.members {
            self.fee(m.name.len())?;
            match &m.kind {
                DynMemberKind::Type => {}
                DynMemberKind::Constant => {}
                DynMemberKind::Method(ps) => {
                    self.rows(ps.len())?;
                    for p in ps {
                        if let LifeParameterKind::Const(t) = p {
                            self.ty(t)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn interface(&mut self, i: &DynInterface) -> R<()> {
        self.rows(2)?;
        if let Some(t) = &i.context {
            self.ty(t)?;
        }
        if let Some(c) = &i.owning_cleanup {
            self.cleanup(c)?;
        }
        self.application(&i.application)?;
        self.rows(i.inherited.len())?;
        for a in &i.inherited {
            self.application(a)?;
        }
        self.rows(
            i.associated_types
                .len()
                .saturating_add(i.associated_constants.len()),
        )?;
        for a in &i.associated_types {
            self.application(&a.member.application)?;
            self.ty(&a.ty)?;
            DynRecipeVisitor::life(self, &a.life, 0)?;
            self.rows(a.classes.len())?;
        }
        for a in &i.associated_constants {
            self.application(&a.member.application)?;
            self.ty(&a.ty)?;
            self.constant(&a.value)?;
        }
        self.rows(
            i.retained_life
                .ports
                .len()
                .saturating_add(i.retained_life.retained.len())
                .saturating_add(i.retained_life.nominal_arguments.len()),
        )?;
        for p in &i.retained_life.ports {
            self.ty(&p.leaf)?;
        }
        for p in &i.retained_life.retained {
            self.path(p.path.len())?;
        }
        for p in &i.retained_life.nominal_arguments {
            self.path(p.path.len())?;
        }
        self.rows(i.slots.len())?;
        for s in &i.slots {
            self.slot(s)?;
        }
        Ok(())
    }
    fn slot(&mut self, s: &DynSlot) -> R<()> {
        self.application(&s.member.application)?;
        self.rows(
            s.actuals
                .len()
                .saturating_add(s.parameters.len())
                .saturating_add(s.parameter_modes.len()),
        )?;
        for a in &s.actuals {
            self.actual(a)?;
        }
        for t in &s.parameters {
            self.ty(t)?;
        }
        self.ty(&s.result)?;
        if let Some(c) = &s.storage_life {
            self.fee(borrowed_fp::metadata_work(c))?;
            for r in &c.bindings {
                self.path(r.position.path.len())?;
            }
            self.rows(c.static_bindings.len())?;
            for position in &c.static_bindings {
                self.path(position.path.len())?;
            }
            for r in &c.classes {
                self.path(r.position.path.len())?;
            }
        }
        self.contract(&s.contract)?;
        Ok(())
    }
}

pub(super) fn snapshot(
    core: &CoreModule,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    {
        let mut w = Work {
            limits,
            left,
            copies,
            nodes: 0,
        };
        if let Some(semantic) = &core.semantic {
            semantic.visit_dyn_metadata(&mut w, 0)?;
        }
        for function in &core.functions {
            w.rows(1)?;
            if let Some(instance) = &function.dyn_instance {
                instance.visit_dyn_metadata(&mut w, 0)?;
            }
        }
    }
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
        w.declaration(d)?;
    }
    for i in &core.dyn_interfaces {
        w.interface(i)?;
    }
    for t in &core.dyn_vtables {
        w.rows(2)?;
        if let Some(shape) = &t.self_life {
            DynRecipeVisitor::life(&mut w, shape, 0)?;
        }
        if let Some(values) = &t.implementation_witnesses {
            w.rows(values.len())?;
            for value in values {
                value.visit_dyn_metadata(&mut w, 0)?;
            }
        }
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

impl DynRecipeVisitor for Work<'_, '_> {
    type Error = Diagnostic;
    fn node(&mut self, depth: usize) -> R<()> {
        if depth > self.limits.max_call_depth.min(128) {
            return Err(Diagnostic::limit(
                Phase::Types,
                "dynamic recipe depth exceeded",
                None,
            ));
        }
        Work::rows(self, 1)
    }
    fn rows(&mut self, n: usize, depth: usize) -> R<()> {
        self.node(depth)?;
        Work::rows(self, n)
    }
    fn ty(&mut self, value: &Type, depth: usize) -> R<()> {
        self.node(depth)?;
        inference::charge_type_snapshot(
            value,
            Span::default(),
            &Limits {
                max_call_depth: self.limits.max_call_depth.min(128).saturating_sub(depth),
                ..self.limits.clone()
            },
            self.left,
            self.copies,
        )
    }
    fn life(&mut self, value: &StorageLifeShape, depth: usize) -> R<()> {
        DynRecipeVisitor::rows(self, value.binder_count as usize, depth)?;
        DynRecipeVisitor::rows(
            self,
            value
                .slots
                .len()
                .saturating_add(value.retained.len())
                .saturating_add(value.nominal_arguments.len()),
            depth,
        )?;
        for row in &value.slots {
            self.node(depth.saturating_add(row.path.len()))?;
            self.path(row.path.len())?;
        }
        for row in &value.retained {
            self.node(depth.saturating_add(row.path.len()))?;
            self.path(row.path.len())?;
        }
        for row in &value.nominal_arguments {
            self.node(depth.saturating_add(row.path.len()))?;
            self.path(row.path.len())?;
        }
        Ok(())
    }
    fn identity(&mut self, value: &LifeDeclarationIdentity, depth: usize) -> R<()> {
        self.node(depth)?;
        self.fee(value.package.len())?;
        Work::rows(
            self,
            value
                .module_path
                .len()
                .saturating_add(value.declaration_path.len())
                .saturating_add(value.generation_path.len()),
        )?;
        for part in value.module_path.iter().chain(&value.declaration_path) {
            self.fee(part.len())?;
        }
        Ok(())
    }
    fn position(&mut self, value: &StoragePosition, depth: usize) -> R<()> {
        self.node(depth.saturating_add(value.path.len()))?;
        self.path(value.path.len())
    }
    fn constant(&mut self, value: &Constant, depth: usize) -> R<()> {
        self.node(depth)?;
        Work::constant(self, value)
    }
    fn text(&mut self, value: &str, depth: usize) -> R<()> {
        self.node(depth)?;
        self.fee(value.len())
    }
}

pub(super) fn semantic_snapshot(
    value: &SemanticModule,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    let mut work = Work {
        limits,
        left,
        copies,
        nodes: 0,
    };
    value.visit_dyn_metadata(&mut work, 0)
}

/// Prepay every nested semantic operand before cloning or interpreting it.
pub(crate) fn semantic_operand_snapshot(
    value: &impl DynRecipeMetadata,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    let mut work = Work {
        limits,
        left,
        copies,
        nodes: 0,
    };
    value.visit_dyn_metadata(&mut work, 0)
}

/// Bounded snapshots of actual runtime Dyn data use the same traversal as
/// Core transport. Recipe metadata visitors cannot stand in for these fields.
pub(crate) fn actual_declaration_snapshot(
    value: &DynDeclaration,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    Work {
        limits,
        left,
        copies,
        nodes: 0,
    }
    .declaration(value)
}
pub(crate) fn actual_interface_snapshot(
    value: &DynInterface,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    Work {
        limits,
        left,
        copies,
        nodes: 0,
    }
    .interface(value)
}
pub(crate) fn actual_slot_snapshot(
    value: &DynSlot,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    Work {
        limits,
        left,
        copies,
        nodes: 0,
    }
    .slot(value)
}
