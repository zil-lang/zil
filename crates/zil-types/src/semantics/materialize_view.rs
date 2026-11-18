//! Borrowed actual emitter products. Building views contain real partial
//! functions and formed closure rows, never a fabricated TypedModule.
use super::*;
use std::borrow::Cow;

enum Functions<'a> {
    Complete(&'a [TypedFunction]),
    Building(Vec<crate::FunctionRef<'a>>),
}
type ParentKey = (SemanticInstanceId, SemanticSiteId, SemanticInstanceId);
type ParentSolution = (SemanticInstanceId, std::rc::Rc<Vec<SemanticActualRecipe>>);
struct SourceDirectory<'a> {
    program: &'a SemanticModule,
    context: std::rc::Rc<super::super::original_context::OriginalContext>,
    definitions: BTreeMap<SemanticDefinitionId, usize>,
    positions: BTreeMap<SemanticInstanceId, usize>,
    last_instance: std::cell::Cell<Option<(SemanticInstanceId, Option<usize>)>>,
    functions: BTreeMap<FunctionId, usize>,
    nominals: BTreeMap<LifeDeclarationId, Vec<usize>>,
    parents: std::cell::RefCell<BTreeMap<ParentKey, ParentSolution>>,
}
pub(in crate::semantics) struct ActualDirectory<'a> {
    complete: Option<&'a TypedModule>,
    source: Option<SourceDirectory<'a>>,
    ordinary: std::cell::RefCell<Option<stored::OrdinaryContext<'a>>>,
    functions: Functions<'a>,
    pub(in crate::semantics) closures: Cow<'a, [ClosureDefinition]>,
    // Private, complete child proposals for a single deferred nominal pass.
    // The raw physical occurrence accompanies the row until canonical rewrite.
    pub(super) nominal_proposals: std::cell::RefCell<Vec<(Type, LifeNominalInstance)>>,
    pub(super) deferred_nominals:
        std::cell::RefCell<Option<deferred::NominalIndex<deferred::NominalPosition>>>,
    pub(in crate::semantics) life_instances: &'a [LifeNominalInstance],
    pub(in crate::semantics) dyn_declarations: &'a [DynDeclaration],
    pub(in crate::semantics) dyn_interfaces: &'a [DynInterface],
    pub(in crate::semantics) instances: BTreeMap<SemanticInstanceId, SemanticInstance>,
    pub(super) invocation_inputs:
        std::cell::RefCell<BTreeMap<invocations::InputKey, invocations::Inputs>>,
    pub(super) invocation_active: std::cell::RefCell<BTreeSet<SemanticInstanceId>>,
}
impl<'a> ActualDirectory<'a> {
    pub(super) fn stable_invocation_inputs(&self) -> bool {
        self.complete.is_some() && self.instances.is_empty()
    }
    pub(in crate::semantics) fn from_module(module: &'a TypedModule) -> Self {
        Self::from_module_at(module, "complete directory")
    }
    pub(in crate::semantics) fn from_module_at(module: &'a TypedModule, tag: &'static str) -> Self {
        Self {
            complete: Some(module),
            source: None,
            invocation_inputs: Default::default(),
            invocation_active: Default::default(),
            ordinary: std::cell::RefCell::new(Some(stored::OrdinaryContext::tagged(module, tag))),
            functions: Functions::Complete(&module.functions),
            closures: Cow::Borrowed(&module.closures),
            life_instances: &module.life_instances,
            nominal_proposals: Default::default(),
            deferred_nominals: Default::default(),
            dyn_declarations: &module.dyn_declarations,
            dyn_interfaces: &module.dyn_interfaces,
            instances: BTreeMap::new(),
        }
    }
    pub(in crate::semantics) fn for_world(
        module: &'a TypedModule,
        program: &'a SemanticModule,
        context: &std::rc::Rc<super::super::original_context::OriginalContext>,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Self> {
        Self::indexed(module, program, context, "world", limits, left)
    }
    pub(in crate::semantics) fn indexed(
        module: &'a TypedModule,
        program: &'a SemanticModule,
        context: &std::rc::Rc<super::super::original_context::OriginalContext>,
        tag: &'static str,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Self> {
        let count = program
            .instances
            .len()
            .saturating_add(program.definitions.len())
            .saturating_add(module.functions.len())
            .saturating_add(module.life_instances.len());
        if count > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "immutable materialization directory exceeds node limit",
                None,
            ));
        }
        Self::source_fee(
            left,
            count
                .saturating_mul(count.saturating_add(1).ilog2() as usize + 2)
                .saturating_add(1),
        )?;
        let mut definitions = BTreeMap::new();
        for (offset, row) in program.definitions.iter().enumerate() {
            if definitions.insert(row.id, offset).is_some() {
                return Err(bad("immutable materialization directory repeats a definition"));
            }
        }
        let mut positions = BTreeMap::new();
        for (offset, row) in program.instances.iter().enumerate() {
            if positions.insert(row.id, offset).is_some() {
                return Err(bad(
                    "immutable materialization directory repeats an instance",
                ));
            }
        }
        let mut functions = BTreeMap::new();
        for (offset, function) in module.functions.iter().enumerate() {
            if functions.insert(function.id, offset).is_some() {
                return Err(bad(
                    "immutable materialization directory repeats a function",
                ));
            }
        }
        let mut nominals: BTreeMap<LifeDeclarationId, Vec<usize>> = BTreeMap::new();
        for (offset, row) in module.life_instances.iter().enumerate() {
            nominals.entry(row.declaration).or_default().push(offset);
        }
        let mut directory = Self::from_module_at(module, tag);
        directory.source = Some(SourceDirectory {
            program,
            context: context.clone(),
            definitions,
            positions,
            last_instance: Default::default(),
            functions,
            nominals,
            parents: Default::default(),
        });
        Ok(directory)
    }
    fn source_fee(left: &mut usize, work: usize) -> R<()> {
        *left = left.checked_sub(work).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "immutable materialization directory work exceeded",
                None,
            )
        })?;
        Ok(())
    }
    pub(super) fn indexed_instance<'p>(
        &self,
        program: &'p SemanticModule,
        id: SemanticInstanceId,
        left: &mut usize,
    ) -> R<Option<&'p SemanticInstance>> {
        let Some(source) = &self.source else {
            return Ok(None);
        };
        if !std::ptr::eq(source.program, program) {
            return Ok(None);
        }
        Self::source_fee(left, 1)?;
        let position = match source.last_instance.get() {
            Some((previous, position)) if previous == id => position,
            _ => {
                Self::source_fee(
                    left,
                    source.positions.len().saturating_add(1).ilog2() as usize + 2,
                )?;
                let position = source.positions.get(&id).copied();
                source.last_instance.set(Some((id, position)));
                position
            }
        };
        Ok(position.map(|offset| &program.instances[offset]))
    }
    pub(super) fn find_source_definition<'p>(
        &self,
        program: &'p SemanticModule,
        id: SemanticDefinitionId,
        left: &mut usize,
    ) -> R<Option<&'p SemanticDefinition>> {
        if let Some(source) = self.source.as_ref().filter(|source| std::ptr::eq(source.program, program)) {
            Self::source_fee(
                left,
                source.definitions.len().saturating_add(1).ilog2() as usize + 1,
            )?;
            return Ok(source.definitions.get(&id).map(|offset| &program.definitions[*offset]));
        }
        Self::source_fee(left, program.definitions.len())?;
        Ok(program.definitions.iter().find(|row| row.id == id))
    }
    // Candidate offsets only: the consumer must still compare complete mixed
    // actuals, physical hint and class domain, and reject all ambiguity.
    pub(super) fn nominal_positions(
        &self,
        declaration: LifeDeclarationId,
        left: &mut usize,
    ) -> R<Option<&[usize]>> {
        let Some(source) = &self.source else {
            Self::source_fee(left, self.life_instances.len())?;
            return Ok(None);
        };
        Self::source_fee(
            left,
            source.nominals.len().saturating_add(1).ilog2() as usize + 1,
        )?;
        Ok(Some(
            source.nominals.get(&declaration).map_or(&[], Vec::as_slice),
        ))
    }
    pub(super) fn function_lookup_work(&self) -> usize {
        self.source.as_ref().map_or_else(
            || self.function_count(),
            |source| source.functions.len().saturating_add(1).ilog2() as usize + 1,
        )
    }
    fn parent_context<'b>(
        &'b self,
        program: &SemanticModule,
        context: Option<&std::rc::Rc<super::super::original_context::OriginalContext>>,
    ) -> R<Option<&'b SourceDirectory<'a>>> {
        let Some(source) = &self.source else {
            return Ok(None);
        };
        if !std::ptr::eq(source.program, program) {
            return Ok(None);
        }
        if !context.is_some_and(|context| std::rc::Rc::ptr_eq(context, &source.context)) {
            return Err(bad(
                "pure parent materialization crossed its immutable original Gamma",
            ));
        }
        Ok(Some(source))
    }
    pub(super) fn parent_solution(
        &self,
        program: &SemanticModule,
        context: Option<&std::rc::Rc<super::super::original_context::OriginalContext>>,
        key: ParentKey,
        left: &mut usize,
    ) -> R<Option<ParentSolution>> {
        let Some(source) = self.parent_context(program, context)? else {
            return Ok(None);
        };
        let parents = source.parents.borrow();
        Self::source_fee(left, parents.len().saturating_add(1).ilog2() as usize + 2)?;
        Ok(parents.get(&key).cloned())
    }
    pub(super) fn remember_parent(
        &self,
        program: &SemanticModule,
        context: Option<&std::rc::Rc<super::super::original_context::OriginalContext>>,
        key: ParentKey,
        value: &ParentSolution,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        let Some(source) = self.parent_context(program, context)? else {
            return Ok(());
        };
        let mut parents = source.parents.borrow_mut();
        if parents.len() >= limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "pure parent materialization entry limit exceeded",
                None,
            ));
        }
        Self::source_fee(
            left,
            parents.len().saturating_add(1).ilog2() as usize
                + std::mem::size_of::<(ParentKey, ParentSolution)>(),
        )?;
        parents.insert(key, value.clone());
        Ok(())
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
        stats: &std::rc::Rc<std::cell::RefCell<stored::OrdinaryStats>>,
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
            .zero_tracked(ty, limits, left, stats)
    }
    pub(in crate::semantics) fn function_count(&self) -> usize {
        match &self.functions {
            Functions::Complete(rows) => rows.len(),
            Functions::Building(rows) => rows.len(),
        }
    }
    pub(in crate::semantics) fn function(&self, id: FunctionId) -> Option<crate::FunctionRef<'a>> {
        if let (Some(source), Functions::Complete(rows)) = (&self.source, &self.functions) {
            return source
                .functions
                .get(&id)
                .and_then(|offset| rows.get(*offset))
                .map(crate::FunctionRef::from);
        }
        match &self.functions {
            Functions::Complete(rows) => rows
                .iter()
                .find(|row| row.id == id)
                .map(crate::FunctionRef::from),
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
        functions: Vec<crate::FunctionRef<'a>>,
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
            source: None,
            invocation_inputs: Default::default(),
            invocation_active: Default::default(),
            ordinary: std::cell::RefCell::new(None),
            functions: Functions::Building(functions),
            closures: Cow::Owned(closures),
            life_instances,
            nominal_proposals: Default::default(),
            deferred_nominals: Default::default(),
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
