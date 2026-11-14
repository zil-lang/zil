//! Request-local immutable original input and its Types-only symbolic proof.
//! This is neither a public certificate nor a cache: a new Types invocation
//! must build and check its own context. Runtime emission is checked separately.
use crate::*;
use std::ops::Range;
use std::rc::Rc;
use zil_base::ModuleId;
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-CONTEXT", Span::default(), message)
}
fn pay(left: &mut usize, work: usize) -> R<()> {
    *left = left
        .checked_sub(work)
        .ok_or_else(|| Diagnostic::limit(Phase::Types, "original context work exceeded", None))?;
    Ok(())
}
fn search(n: usize) -> usize {
    n.saturating_add(1).ilog2() as usize + 1
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Declaration {
    pub(super) row: DynDeclaration,
    pub(super) scope: SemanticDefinitionId,
}
struct Part {
    module: ModuleId,
    present: bool,
    definitions: Range<usize>,
    instances: Range<usize>,
    sites: Range<usize>,
    dyn_scopes: Range<usize>,
    declarations: Range<usize>,
}
pub(super) struct Input {
    pub(super) semantic: SemanticModule,
    pub(super) declarations: Rc<Vec<Declaration>>,
    parts: Vec<Part>,
}
impl Input {
    pub(super) fn collect(cores: &[&CoreModule], limits: &Limits, left: &mut usize) -> R<Self> {
        pay(left, cores.len().saturating_add(1))?;
        let mut semantic = SemanticModule {
            definitions: Vec::new(),
            instances: Vec::new(),
            sites: Vec::new(),
            dyn_scopes: Vec::new(),
        };
        let mut parts = Vec::with_capacity(cores.len());
        let mut declarations = Vec::new();
        for core in cores {
            let start = (
                semantic.definitions.len(),
                semantic.instances.len(),
                semantic.sites.len(),
                semantic.dyn_scopes.len(),
                declarations.len(),
            );
            if let Some(program) = &core.semantic {
                crate::dyn_metadata::semantic_snapshot(program, limits, left, 1)?;
                pay(
                    left,
                    program
                        .definitions
                        .len()
                        .saturating_add(program.instances.len())
                        .saturating_add(program.sites.len())
                        .saturating_add(program.dyn_scopes.len())
                        .saturating_mul(2)
                        .saturating_add(1),
                )?;
                let copy = program.clone();
                semantic.definitions.extend(copy.definitions);
                semantic.instances.extend(copy.instances);
                semantic.sites.extend(copy.sites);
                semantic.dyn_scopes.extend(copy.dyn_scopes);
            }
            pay(left, core.dyn_declarations.len().saturating_add(1))?;
            for row in &core.dyn_declarations {
                crate::dyn_metadata::actual_declaration_snapshot(row, limits, left, 1)?;
                if row.id.module != core.module_id {
                    return Err(bad(
                        "original declaration is outside its actual source Core module",
                    ));
                }
                let original = core.semantic.as_ref().ok_or_else(|| {
                    bad("original associated declaration has no source semantic scope")
                })?;
                pay(left, original.dyn_scopes.len().saturating_add(1))?;
                let mut scopes = original.dyn_scopes.iter().filter(|binding| {
                    matches!(
                        binding.owner, SemanticDynOwner::Declaration(id) if id == row.id
                    )
                });
                let scope = scopes
                    .next()
                    .ok_or_else(|| {
                        bad(
                    "original associated declaration has no exact declaration Scope binding",
                )
                    })?
                    .scope;
                if scopes.next().is_some() {
                    return Err(bad(
                        "original associated declaration has duplicate Scope bindings",
                    ));
                }
                declarations.push(Declaration {
                    row: row.clone(),
                    scope,
                });
            }
            parts.push(Part {
                module: core.module_id,
                present: core.semantic.is_some(),
                definitions: start.0..semantic.definitions.len(),
                instances: start.1..semantic.instances.len(),
                sites: start.2..semantic.sites.len(),
                dyn_scopes: start.3..semantic.dyn_scopes.len(),
                declarations: start.4..declarations.len(),
            });
        }
        Ok(Self {
            semantic,
            declarations: Rc::new(declarations),
            parts,
        })
    }
    /// E can refer to immutable S declarations; S instances/sites remain in S.
    /// Every duplicate is compared completely before it is accepted as shared.
    pub(super) fn include_frozen(
        &mut self,
        source: &super::frozen_context::FrozenSource,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        let mut index = BTreeMap::new();
        pay(left, self.semantic.definitions.len())?;
        for (ordinal, definition) in self.semantic.definitions.iter().enumerate() {
            pay(left, search(index.len()))?;
            if index.insert(definition.id, ordinal).is_some() {
                return Err(bad("original context contains duplicate E definitions"));
            }
        }
        pay(left, source.semantic.definitions.len())?;
        for original in &source.semantic.definitions {
            pay(left, search(index.len()))?;
            // Recursive snapshot accounting also bounds a full structural
            // comparison. A fingerprint never substitutes for this equality.
            crate::dyn_metadata::semantic_operand_snapshot(original, limits, left, 1)?;
            if let Some(ordinal) = index.get(&original.id) {
                let actual = &self.semantic.definitions[*ordinal];
                crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
                if actual != original {
                    return Err(bad("E changes an immutable S original definition"));
                }
            } else {
                pay(left, search(index.len()).saturating_add(2))?;
                index.insert(original.id, self.semantic.definitions.len());
                self.semantic.definitions.push(original.clone());
            }
        }
        let mut declarations = Vec::new();
        pay(
            left,
            self.declarations
                .len()
                .saturating_add(source.declarations.len())
                .saturating_add(1),
        )?;
        let mut identities = BTreeMap::new();
        let mut executable_ids = BTreeSet::new();
        for declaration in self.declarations.iter() {
            pay(left, search(executable_ids.len()))?;
            if !executable_ids.insert(declaration.row.id) {
                return Err(bad("E repeats an original associated declaration identity"));
            }
        }
        for declaration in self.declarations.iter().chain(source.declarations.iter()) {
            crate::dyn_metadata::actual_declaration_snapshot(&declaration.row, limits, left, 1)?;
            pay(left, search(identities.len()))?;
            if let Some(index) = identities.get(&declaration.row.id) {
                let previous: &Declaration = &declarations[*index];
                crate::dyn_metadata::actual_declaration_snapshot(&previous.row, limits, left, 1)?;
                if previous != declaration {
                    return Err(bad(
                        "E changes an immutable S original associated declaration or Scope binding",
                    ));
                }
            } else {
                identities.insert(declaration.row.id, declarations.len());
                declarations.push(declaration.clone());
            }
        }
        self.declarations = Rc::new(declarations);
        Ok(())
    }
}
pub(crate) struct OriginalContext {
    pub(super) semantic: SemanticModule,
    pub(super) declarations: Rc<Vec<Declaration>>,
    pub(super) original: super::original::Proof,
    parts: Vec<Part>,
    definitions: BTreeMap<SemanticDefinitionId, usize>,
    instances: BTreeMap<SemanticInstanceId, usize>,
    frozen: Option<Rc<super::frozen_context::FrozenSource>>,
}
impl OriginalContext {
    pub(super) fn checked(
        input: Input,
        proof: super::original::Proof,
        frozen: Option<Rc<super::frozen_context::FrozenSource>>,
        left: &mut usize,
    ) -> R<Rc<Self>> {
        if !Rc::ptr_eq(&input.declarations, &proof.declarations) {
            return Err(bad(
                "checked original proof differs from its immutable declaration input",
            ));
        }
        let mut definitions = BTreeMap::new();
        let mut instances = BTreeMap::new();
        pay(
            left,
            input
                .semantic
                .definitions
                .len()
                .saturating_add(input.semantic.instances.len())
                .saturating_add(1),
        )?;
        for (index, definition) in input.semantic.definitions.iter().enumerate() {
            pay(left, search(definitions.len()))?;
            if definitions.insert(definition.id, index).is_some() {
                return Err(bad("checked original context has duplicate definitions"));
            }
        }
        for (index, instance) in input.semantic.instances.iter().enumerate() {
            pay(left, search(instances.len()))?;
            if instances.insert(instance.id, index).is_some() {
                return Err(bad("checked original context has duplicate instances"));
            }
        }
        Ok(Rc::new(Self {
            semantic: input.semantic,
            declarations: input.declarations,
            original: proof,
            parts: input.parts,
            definitions,
            instances,
            frozen,
        }))
    }
    pub(super) fn definition(
        &self,
        id: SemanticDefinitionId,
        left: &mut usize,
    ) -> R<&SemanticDefinition> {
        pay(left, search(self.definitions.len()))?;
        self.definitions
            .get(&id)
            .and_then(|index| self.semantic.definitions.get(*index))
            .ok_or_else(|| bad("original context definition is absent"))
    }
    pub(super) fn instance(
        &self,
        id: SemanticInstanceId,
        left: &mut usize,
    ) -> R<&SemanticInstance> {
        pay(left, search(self.instances.len()))?;
        self.instances
            .get(&id)
            .and_then(|index| self.semantic.instances.get(*index))
            .ok_or_else(|| bad("original context instance is absent"))
    }
    pub(super) fn verify_inputs(
        &self,
        cores: &[&CoreModule],
        frozen: Option<&Rc<super::frozen_context::FrozenSource>>,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        if !match (&self.frozen, frozen) {
            (None, None) => true,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        } {
            return Err(bad("original context crossed its immutable frozen request"));
        }
        pay(left, cores.len().saturating_add(self.parts.len()))?;
        if cores.len() != self.parts.len() {
            return Err(bad("original context module inventory changed"));
        }
        for (core, part) in cores.iter().zip(&self.parts) {
            if core.module_id != part.module || core.semantic.is_some() != part.present {
                return Err(bad("original context module identity or Some/None changed"));
            }
            pay(left, core.dyn_declarations.len().saturating_add(1))?;
            for declaration in &core.dyn_declarations {
                crate::dyn_metadata::actual_declaration_snapshot(declaration, limits, left, 1)?;
            }
            let declarations = &self.declarations[part.declarations.clone()];
            if core.dyn_declarations.len() != declarations.len()
                || core
                    .dyn_declarations
                    .iter()
                    .zip(declarations)
                    .any(|(actual, original)| actual != &original.row)
            {
                return Err(bad(
                    "original associated declaration input changed after symbolic checking",
                ));
            }
            let Some(actual) = &core.semantic else {
                continue;
            };
            crate::dyn_metadata::semantic_snapshot(actual, limits, left, 1)?;
            if actual.definitions.as_slice() != &self.semantic.definitions[part.definitions.clone()]
                || actual.instances.as_slice() != &self.semantic.instances[part.instances.clone()]
                || actual.sites.as_slice() != &self.semantic.sites[part.sites.clone()]
                || actual.dyn_scopes.as_slice()
                    != &self.semantic.dyn_scopes[part.dyn_scopes.clone()]
            {
                return Err(bad(
                    "original context input changed after symbolic checking",
                ));
            }
        }
        Ok(())
    }
    /// The one mutable publication copy. Its original definitions are still
    /// checked against this immutable context after all physical rewriting.
    pub(super) fn publication_copy(&self, limits: &Limits, left: &mut usize) -> R<SemanticModule> {
        crate::dyn_metadata::semantic_snapshot(&self.semantic, limits, left, 1)?;
        Ok(self.semantic.clone())
    }
    pub(super) fn verify_definitions(
        &self,
        actual: &SemanticModule,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        pay(left, actual.definitions.len())?;
        if actual.definitions.len() != self.semantic.definitions.len() {
            return Err(bad("publication changed the original definition inventory"));
        }
        for (definition, expected) in actual.definitions.iter().zip(&self.semantic.definitions) {
            crate::dyn_metadata::semantic_operand_snapshot(definition, limits, left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(expected, limits, left, 1)?;
            if definition != expected {
                return Err(bad(
                    "publication changed an immutable original body, scope or premise",
                ));
            }
        }
        Ok(())
    }
}
/// A read-only compatibility view for existing private proof queries. There is
/// no borrow_mut, replacement or cross-request installation operation.
#[derive(Default)]
pub(super) struct Slot {
    context: Option<Rc<OriginalContext>>,
}
pub(super) struct View<'a> {
    proof: Option<&'a super::original::Proof>,
}
impl<'a> View<'a> {
    pub(super) fn as_ref(&self) -> Option<&'a super::original::Proof> {
        self.proof
    }
}
impl Slot {
    pub(super) fn borrow(&self) -> View<'_> {
        View {
            proof: self.context.as_ref().map(|context| &context.original),
        }
    }
    pub(super) fn context(&self) -> Option<&Rc<OriginalContext>> {
        self.context.as_ref()
    }
    pub(super) fn install(&mut self, context: Rc<OriginalContext>) -> R<()> {
        if self.context.is_some() {
            return Err(bad(
                "original context was installed twice in one Types request",
            ));
        }
        self.context = Some(context);
        Ok(())
    }
}
