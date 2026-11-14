//! Borrowed actual emitter products. Building views contain real partial
//! functions and formed closure rows, never a fabricated TypedModule.
use super::*;
use std::borrow::Cow;

enum Functions<'a> {
    Complete(&'a [TypedFunction]),
    Building(Vec<&'a TypedFunction>),
}
pub(in crate::semantics) struct ActualDirectory<'a> {
    complete: Option<&'a TypedModule>,
    ordinary: std::cell::RefCell<Option<stored::OrdinaryContext<'a>>>,
    functions: Functions<'a>,
    pub(in crate::semantics) closures: Cow<'a, [ClosureDefinition]>,
    pub(in crate::semantics) life_instances: &'a [LifeNominalInstance],
    pub(in crate::semantics) dyn_declarations: &'a [DynDeclaration],
    pub(in crate::semantics) dyn_interfaces: &'a [DynInterface],
    pub(in crate::semantics) instances: BTreeMap<SemanticInstanceId, SemanticInstance>,
}
impl<'a> ActualDirectory<'a> {
    pub(in crate::semantics) fn from_module(module: &'a TypedModule) -> Self {
        Self {
            complete: Some(module),
            ordinary: std::cell::RefCell::new(Some(stored::OrdinaryContext::new(module))),
            functions: Functions::Complete(&module.functions),
            closures: Cow::Borrowed(&module.closures),
            life_instances: &module.life_instances,
            dyn_declarations: &module.dyn_declarations,
            dyn_interfaces: &module.dyn_interfaces,
            instances: BTreeMap::new(),
        }
    }
    pub(in crate::semantics) fn complete(&self) -> R<&'a TypedModule> {
        self.complete.ok_or_else(|| {
            unknown(
                "this whole-module obligation awaits completion of the actual emitter directory",
            )
        })
    }
    pub(in crate::semantics) fn ordinary_zero(
        &self,
        ty: &Type,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        self.complete()?;
        let mut context = self
            .ordinary
            .try_borrow_mut()
            .map_err(|_| unknown("ordinary domain context is already active"))?;
        context
            .as_mut()
            .ok_or_else(|| {
                unknown("ordinary domain awaits the complete immutable actual directory")
            })?
            .zero(ty, limits, left)
    }
    pub(in crate::semantics) fn function_count(&self) -> usize {
        match &self.functions {
            Functions::Complete(rows) => rows.len(),
            Functions::Building(rows) => rows.len(),
        }
    }
    pub(in crate::semantics) fn function(&self, id: FunctionId) -> Option<&'a TypedFunction> {
        match &self.functions {
            Functions::Complete(rows) => rows.iter().find(|row| row.id == id),
            Functions::Building(rows) => rows.iter().copied().find(|row| row.id == id),
        }
    }
    pub(in crate::semantics) fn original_instance<'b>(
        &'b self,
        program: &'b SemanticModule,
        id: SemanticInstanceId,
    ) -> Option<&'b SemanticInstance> {
        self.instances
            .get(&id)
            .or_else(|| program.instances.iter().find(|row| row.id == id))
    }
    pub(in crate::semantics) fn semantic_instances<'b>(
        &'b self,
        program: &'b SemanticModule,
    ) -> impl Iterator<Item = &'b SemanticInstance> {
        self.instances.values().chain(
            program
                .instances
                .iter()
                .filter(move |row| !self.instances.contains_key(&row.id)),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantics) fn building(
        functions: Vec<&'a TypedFunction>,
        closures: Vec<ClosureDefinition>,
        life_instances: &'a [LifeNominalInstance],
        dyn_declarations: &'a [DynDeclaration],
        dyn_interfaces: &'a [DynInterface],
        instances: BTreeMap<SemanticInstanceId, SemanticInstance>,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Self> {
        let count = functions
            .len()
            .saturating_add(closures.len())
            .saturating_add(instances.len());
        if functions.len() > limits.max_functions
            || closures.len() > limits.max_functions
            || instances.len() > limits.max_nodes
        {
            return Err(Diagnostic::limit(
                Phase::Types,
                "actual builder directory limit exceeded",
                None,
            ));
        }
        *left = left
            .checked_sub(count.saturating_mul(count.saturating_add(1).ilog2() as usize + 1))
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "actual builder directory work exceeded", None)
            })?;
        let mut ids = BTreeSet::new();
        for function in &functions {
            if !ids.insert(function.id) {
                return Err(bad("actual builder directory repeats a function identity"));
            }
        }
        Ok(Self {
            complete: None,
            ordinary: std::cell::RefCell::new(None),
            functions: Functions::Building(functions),
            closures: Cow::Owned(closures),
            life_instances,
            dyn_declarations,
            dyn_interfaces,
            instances,
        })
    }
}
impl crate::tasks::context_ports::Catalog for ActualDirectory<'_> {
    fn lookup_work(&self) -> usize {
        self.closures
            .len()
            .saturating_add(self.dyn_interfaces.len())
    }
    fn closure(&self, id: ClosureId) -> Option<&ClosureDefinition> {
        self.closures.iter().find(|row| row.id == id)
    }
    fn interface(&self, id: DynInterfaceId) -> Option<&DynInterface> {
        self.dyn_interfaces.iter().find(|row| row.id == id)
    }
}
