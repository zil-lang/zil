//! Compare the selected query to its exact original expression. Group and CT
//! markers are stage transport; callee spelling may change only after the real
//! selected Core target and the full original mixed solution have been checked.
use super::*;

pub(super) struct Compare<'a, 'i> {
    pub source: &'a CoreProgram,
    pub selected: &'a SemanticModule,
    pub program: &'a SemanticModule,
    pub owner: &'a SemanticInstance,
    pub original: &'a original::Checked,
    pub inventory: &'a Inventory<'i>,
}
impl Compare<'_, '_> {
    fn step(&self, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unsupported("CT selected payload exceeds its bounded depth"));
        }
        Ok(())
    }
    pub(super) fn target(
        &self,
        path: &[String],
        b: &mut Budget,
    ) -> CompileResult<Option<&SemanticInstance>> {
        b.spend(self.source.modules.len().saturating_add(path.len()), WORK)?;
        let caller = self
            .source
            .modules
            .iter()
            .find(|m| m.core.module_id == self.owner.id.module)
            .ok_or_else(|| fail("CT selected name has no actual source module"))?;
        let (module, name) = match path {
            [name] => (self.owner.id.module, name),
            [alias, name] => {
                let mut target = None;
                for import in &caller.imports {
                    b.spend(import.alias.len().saturating_add(alias.len()), WORK)?;
                    if &import.alias == alias && target.replace(import.module).is_some() {
                        return Err(fail("CT selected name has ambiguous actual import edges"));
                    }
                }
                let Some(module) = target else {
                    return Ok(None);
                };
                (module, name)
            }
            _ => return Ok(None),
        };
        let source = self
            .source
            .modules
            .iter()
            .find(|m| m.core.module_id == module)
            .ok_or_else(|| fail("CT selected name has no actual imported module"))?;
        let mut function = None;
        for candidate in &source.core.functions {
            b.spend(candidate.name.len().saturating_add(name.len()), WORK)?;
            if &candidate.name == name && function.replace(candidate.id).is_some() {
                return Err(fail("CT selected name has multiple actual Core functions"));
            }
        }
        let Some(function) = function else {
            return Ok(None);
        };
        let mut original = None;
        for instance in &self.program.instances {
            b.spend(1, WORK)?;
            if matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Core {module:actual,function:id}) if actual==module&&id==function)
                && original.replace(instance).is_some()
            {
                return Err(fail("CT selected Core target has multiple original worlds"));
            }
        }
        let original =
            original.ok_or_else(|| fail("CT selected Core target lacks its original instance"))?;
        let mut selected = None;
        for instance in &self.selected.instances {
            b.spend(1, WORK)?;
            if instance.id == original.id && selected.replace(instance).is_some() {
                return Err(fail("CT selected target repeats an actual world"));
            }
        }
        let selected = selected
            .ok_or_else(|| fail("CT selected callee name is not in the actual execution slice"))?;
        if selected.definition != original.definition
            || selected.parent != original.parent
            || selected.environment != original.environment
            || selected.self_type != original.self_type
            || selected.header_embedding != original.header_embedding
            || selected.lifetime_instantiation != original.lifetime_instantiation
            || !matches!(
                selected.target,
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_))
            )
        {
            return Err(fail(
                "CT selected callee changes its complete lexical world",
            ));
        }
        Ok(Some(selected))
    }
    fn callee(
        &self,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
    ) -> CompileResult<Option<&SemanticInstance>> {
        crate::dyn_metadata::recipe(value, b)?;
        let path = match &value.kind {
            CoreExprKind::Name(name) => vec![name.clone()],
            CoreExprKind::Member {
                object,
                member,
                owner: None,
            } => match &object.kind {
                CoreExprKind::Name(alias) => vec![alias.clone(), member.clone()],
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        self.target(&path, b)
    }
    fn annotation(
        &self,
        expected: &SemanticTypeRecipe,
        actual: &mut SemanticTypeRecipe,
        b: &mut Budget,
    ) -> CompileResult<()> {
        crate::dyn_metadata::recipe(expected, b)?;
        crate::dyn_metadata::recipe(actual, b)?;
        // Core-generated atom/nominal annotations need no lexical shift. Any
        // surviving lexical recipe belongs to the real child wrapper frame.
        names::ty(actual, b, 0)?;
        if expected != actual {
            return Err(fail("CT payload changes its complete original annotation"));
        }
        Ok(())
    }
    fn exprs(
        &self,
        a: &mut [SemanticOriginalExpr],
        z: &mut [SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        if a.len() != z.len() {
            return Err(fail("CT payload changes its ordered operand arity"));
        }
        for (a, z) in a.iter_mut().zip(z) {
            self.expr(a, z, b, depth + 1)?;
        }
        Ok(())
    }
    pub(super) fn expr(
        &self,
        a: &mut SemanticOriginalExpr,
        z: &mut SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        // The input subtrees were fully traversed and costed before cloning.
        // Iterative peeling does not hide any operation or choose a branch.
        for value in [&mut *a, &mut *z] {
            while matches!(
                value.kind,
                CoreExprKind::Group(_) | CoreExprKind::CompileTime(_)
            ) {
                self.step(b, depth)?;
                let inner = match std::mem::replace(&mut value.kind, CoreExprKind::Unit) {
                    CoreExprKind::Group(inner) | CoreExprKind::CompileTime(inner) => inner,
                    _ => unreachable!(),
                };
                *value = *inner;
            }
        }
        let original_node = a.id;
        match (&mut a.kind, &mut z.kind) {
            (
                CoreExprKind::Call {
                    callee: ac,
                    arguments: av,
                },
                CoreExprKind::Call {
                    callee: zc,
                    arguments: zv,
                },
            ) => {
                if let Some(target) = self.callee(zc, b)? {
                    self.original.frozen_call(
                        self.owner,
                        original_node,
                        target,
                        self.program,
                        self.inventory,
                        b,
                    )?;
                    // Both original target and every mixed actual were checked
                    // before erasing these private comparison-only spellings.
                    ac.kind = CoreExprKind::Unit;
                    zc.kind = CoreExprKind::Unit;
                } else {
                    self.expr(ac, zc, b, depth + 1)?;
                }
                self.exprs(av, zv, b, depth + 1)?;
            }
            (CoreExprKind::Unary { operand: a, .. }, CoreExprKind::Unary { operand: z, .. })
            | (CoreExprKind::Try(a), CoreExprKind::Try(z)) => self.expr(a, z, b, depth + 1)?,
            (
                CoreExprKind::Ascribed { ty: at, value: a },
                CoreExprKind::Ascribed { ty: zt, value: z },
            ) => {
                self.annotation(at, zt, b)?;
                self.expr(a, z, b, depth + 1)?;
            }
            (
                CoreExprKind::NumericConvert {
                    value: a,
                    target: at,
                },
                CoreExprKind::NumericConvert {
                    value: z,
                    target: zt,
                },
            )
            | (
                CoreExprKind::IntegerConvert {
                    value: a,
                    target: at,
                },
                CoreExprKind::IntegerConvert {
                    value: z,
                    target: zt,
                },
            ) => {
                self.annotation(at, zt, b)?;
                self.expr(a, z, b, depth + 1)?;
            }
            (
                CoreExprKind::Binary {
                    left: a, right: av, ..
                },
                CoreExprKind::Binary {
                    left: z, right: zv, ..
                },
            )
            | (
                CoreExprKind::Index {
                    object: a,
                    index: av,
                },
                CoreExprKind::Index {
                    object: z,
                    index: zv,
                },
            ) => {
                self.expr(a, z, b, depth + 1)?;
                self.expr(av, zv, b, depth + 1)?;
            }
            (CoreExprKind::Tuple(a), CoreExprKind::Tuple(z))
            | (CoreExprKind::Array(a), CoreExprKind::Array(z)) => self.exprs(a, z, b, depth + 1)?,
            (
                CoreExprKind::Member {
                    object: a,
                    owner: at,
                    ..
                },
                CoreExprKind::Member {
                    object: z,
                    owner: zt,
                    ..
                },
            ) => {
                match (at, zt) {
                    (Some(a), Some(z)) => self.annotation(a, z, b)?,
                    (None, None) => {}
                    _ => return Err(fail("CT payload changes its reflected member owner")),
                }
                self.expr(a, z, b, depth + 1)?
            }
            (
                CoreExprKind::If {
                    condition: a,
                    then_value: av,
                    else_value: ae,
                },
                CoreExprKind::If {
                    condition: z,
                    then_value: zv,
                    else_value: ze,
                },
            ) => {
                self.expr(a, z, b, depth + 1)?;
                self.expr(av, zv, b, depth + 1)?;
                self.expr(ae, ze, b, depth + 1)?;
            }
            (
                CoreExprKind::Record { ty: at, fields: a },
                CoreExprKind::Record { ty: zt, fields: z },
            ) => {
                self.annotation(at, zt, b)?;
                self.fields(a, z, b, depth + 1)?;
            }
            (
                CoreExprKind::RecordConstructor { fields: a, .. },
                CoreExprKind::RecordConstructor { fields: z, .. },
            ) => self.fields(a, z, b, depth + 1)?,
            (
                CoreExprKind::RecordUpdate {
                    ty: at,
                    base: a,
                    fields: av,
                },
                CoreExprKind::RecordUpdate {
                    ty: zt,
                    base: z,
                    fields: zv,
                },
            ) => {
                match (at, zt) {
                    (Some(a), Some(z)) => self.annotation(a, z, b)?,
                    (None, None) => {}
                    _ => return Err(fail("CT payload changes its record-update annotation")),
                }
                self.expr(a, z, b, depth + 1)?;
                self.fields(av, zv, b, depth + 1)?;
            }
            (
                CoreExprKind::Slice {
                    object: a,
                    start: av,
                    end: ae,
                },
                CoreExprKind::Slice {
                    object: z,
                    start: zv,
                    end: ze,
                },
            ) => {
                self.expr(a, z, b, depth + 1)?;
                for (a, z) in [(av, zv), (ae, ze)] {
                    match (a, z) {
                        (Some(a), Some(z)) => self.expr(a, z, b, depth + 1)?,
                        (None, None) => {}
                        _ => return Err(fail("CT payload changes a slice endpoint")),
                    }
                }
            }
            (
                CoreExprKind::GenericApply {
                    callee: a,
                    arguments: av,
                },
                CoreExprKind::GenericApply {
                    callee: z,
                    arguments: zv,
                },
            ) => {
                self.expr(a, z, b, depth + 1)?;
                if av.len() != zv.len() {
                    return Err(fail("CT payload changes written mixed argument arity"));
                }
                for (a, z) in av.iter_mut().zip(zv) {
                    match (a, z) {
                        (CoreGenericArgument::Type(a), CoreGenericArgument::Type(z)) => {
                            self.annotation(a, z, b)?
                        }
                        (CoreGenericArgument::Const(a), CoreGenericArgument::Const(z)) => {
                            self.expr(a, z, b, depth + 1)?
                        }
                        (CoreGenericArgument::Lifetime(_), CoreGenericArgument::Lifetime(_)) => {}
                        _ => return Err(fail("CT payload changes a written mixed argument kind")),
                    }
                }
            }
            _ => {}
        }
        // All unhandled constructors retain strict structural equality after
        // ordinary node/span normalization; no absent consumer is success.
        Ok(())
    }
    fn fields(
        &self,
        a: &mut [(String, SemanticOriginalExpr)],
        z: &mut [(String, SemanticOriginalExpr)],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        if a.len() != z.len() {
            return Err(fail("CT payload changes its field inventory"));
        }
        for ((an, a), (zn, z)) in a.iter_mut().zip(z) {
            b.spend(an.len().saturating_add(zn.len()), WORK)?;
            if an != zn {
                return Err(fail("CT payload changes a field name"));
            }
            self.expr(a, z, b, depth + 1)?;
        }
        Ok(())
    }
}
