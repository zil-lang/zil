//! Borrowed actual emitter products. Building views contain real partial
//! functions and formed closure rows, never a fabricated TypedModule.
use super::*;
use std::borrow::Cow;

enum Functions<'a> {
    Complete(&'a [TypedFunction]),
    Building(Vec<&'a TypedFunction>),
}
pub(super) struct ActualDirectory<'a> {
    complete: Option<&'a TypedModule>,
    functions: Functions<'a>,
    pub(super) closures: Cow<'a, [ClosureDefinition]>,
    pub(super) life_instances: &'a [LifeNominalInstance],
    pub(super) dyn_declarations: &'a [DynDeclaration],
    pub(super) dyn_interfaces: &'a [DynInterface],
    pub(super) instances: BTreeMap<SemanticInstanceId, SemanticInstance>,
}
impl<'a> ActualDirectory<'a> {
    pub(super) fn from_module(module: &'a TypedModule) -> Self {
        Self {
            complete: Some(module),
            functions: Functions::Complete(&module.functions),
            closures: Cow::Borrowed(&module.closures),
            life_instances: &module.life_instances,
            dyn_declarations: &module.dyn_declarations,
            dyn_interfaces: &module.dyn_interfaces,
            instances: BTreeMap::new(),
        }
    }
    pub(super) fn complete(&self) -> R<&'a TypedModule> {
        self.complete.ok_or_else(|| {
            unknown(
                "this whole-module obligation awaits completion of the actual emitter directory",
            )
        })
    }
    pub(super) fn function_count(&self) -> usize {
        match &self.functions {
            Functions::Complete(rows) => rows.len(),
            Functions::Building(rows) => rows.len(),
        }
    }
    pub(super) fn function(&self, id: FunctionId) -> Option<&'a TypedFunction> {
        match &self.functions {
            Functions::Complete(rows) => rows.iter().find(|row| row.id == id),
            Functions::Building(rows) => rows.iter().copied().find(|row| row.id == id),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn building(
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
    fn closure(&self, id: ClosureId) -> Option<&ClosureDefinition> {
        self.closures.iter().find(|row| row.id == id)
    }
    fn interface(&self, id: DynInterfaceId) -> Option<&DynInterface> {
        self.dyn_interfaces.iter().find(|row| row.id == id)
    }
}
