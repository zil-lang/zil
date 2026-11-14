//! Original declaration templates and their actual immutable syntax ledger.
//! These are data inputs, not completed instances, effects or source proofs.
use crate::{generics::Budget, source_context::ModuleDeclarationOrigins, source_declarations::*};
use zil_base::{CompileResult, Diagnostic, Limits, ModuleId, Phase, Span};
use zil_ir::*;
use zil_syntax::ast as s;

#[path = "source_semantic_body.rs"]
mod body;
#[path = "source_semantic_body_nodes.rs"]
mod body_nodes;
#[path = "source_semantic_builtins.rs"]
mod builtins;
#[path = "source_semantic_closures.rs"]
mod closures;
#[path = "source_semantic_constants.rs"]
mod constants;
#[path = "source_semantic_context.rs"]
mod context;
#[path = "source_semantic_dyn.rs"]
mod dyn_original;
#[path = "source_semantic_effects.rs"]
mod effects;
#[path = "source_semantic_emission.rs"]
pub(crate) mod emission;
#[path = "source_semantic_names.rs"]
mod names;
#[path = "source_semantic_nodes.rs"]
mod nodes;
#[path = "source_semantic_plan.rs"]
mod plan;
#[path = "source_semantic_prelude.rs"]
mod prelude;
#[path = "source_semantic_program.rs"]
pub(crate) mod program;
#[path = "source_semantic_projection.rs"]
mod projection;
#[path = "source_semantic_requests.rs"]
mod requests;
#[path = "source_semantic_types.rs"]
mod types;

pub struct OriginalSemanticInventory<'a> {
    /// Full original bodies/defaults remain available here, never empty CFGs.
    pub originals: SourceDeclarationInventory<'a>,
    /// Declaration templates only. Actual instances and emission sites must
    /// be added by the real specialization/emission consumer before publish.
    pub modules: Vec<(ModuleId, SemanticModule)>,
    closures: Vec<closures::ClosureTemplate>,
    compiler_nominals: Vec<builtins::CompilerNominal>,
    compiler_declarations: Vec<LifeNominalDeclaration>,
    body_nodes: Vec<DefinitionBodyNodes>,
}

fn error(span: Span, message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-SOURCE-SEMANTICS", Some(span), message)
}
fn unsupported(span: Span, message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Expand, message, Some(span))
}
type R<T> = Result<T, Diagnostic>;

/// Structural clone prepayment only; interpretation remains the consumer's
/// task. The visitor includes every original recipe constructor and scope.
pub(crate) fn prepay_recipe(
    value: &impl DynRecipeMetadata,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> R<()> {
    budget.retained_copy(span, |budget| recipe_work(value, budget, span, depth, true))
}
fn prepay_recipe_work(
    value: &impl DynRecipeMetadata,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> R<()> {
    budget.borrowed_work(span, |budget| {
        recipe_work(value, budget, span, depth, false)
    })
}
fn recipe_work(
    value: &impl DynRecipeMetadata,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
    copy: bool,
) -> R<()> {
    struct Meter<'b, 'l> {
        budget: &'b mut Budget<'l>,
        span: Span,
        copy: bool,
    }
    impl DynRecipeVisitor for Meter<'_, '_> {
        type Error = Diagnostic;
        fn node(&mut self, depth: usize) -> R<()> {
            self.budget.node(self.span, depth)
        }
        fn rows(&mut self, length: usize, depth: usize) -> R<()> {
            self.node(depth)?;
            self.budget.identity_work(length, self.span)?;
            // The generated heap stack stores one reference/depth/tag frame
            // per pending child. Four host words bound a frame; doubling
            // covers Vec growth before the previous allocation is released.
            self.budget.scratch_storage(
                length
                    .checked_mul(8 * std::mem::size_of::<usize>())
                    .ok_or_else(|| error(self.span, "semantic traversal scratch overflow"))?,
                self.span,
            )?;
            if self.copy {
                self.budget.lineage_bytes(
                    length
                        .checked_mul(std::mem::size_of::<SemanticTypeRecipe>())
                        .ok_or_else(|| error(self.span, "semantic clone inventory overflow"))?,
                    self.span,
                )?;
            }
            Ok(())
        }
        fn ty(&mut self, value: &Type, depth: usize) -> R<()> {
            crate::stored_types::pay_type(value, self.budget, self.span, depth)
        }
        fn life(&mut self, value: &StorageLifeShape, depth: usize) -> R<()> {
            self.node(depth)?;
            self.budget
                .identity_work(value.binder_count as usize, self.span)?;
            self.rows(
                value.slots.len() + value.retained.len() + value.nominal_arguments.len(),
                depth + 1,
            )?;
            for row in &value.slots {
                self.rows(row.path.len(), depth + 1)?;
            }
            for row in &value.retained {
                self.rows(row.path.len(), depth + 1)?;
            }
            for row in &value.nominal_arguments {
                self.rows(row.path.len(), depth + 1)?;
            }
            Ok(())
        }
        fn identity(&mut self, value: &LifeDeclarationIdentity, depth: usize) -> R<()> {
            self.node(depth)?;
            self.text(&value.package, depth + 1)?;
            self.rows(
                value.module_path.len()
                    + value.declaration_path.len()
                    + value.generation_path.len(),
                depth + 1,
            )?;
            for part in value.module_path.iter().chain(&value.declaration_path) {
                self.text(part, depth + 1)?;
            }
            Ok(())
        }
        fn position(&mut self, value: &StoragePosition, depth: usize) -> R<()> {
            self.rows(value.path.len(), depth)
        }
        fn constant(&mut self, value: &Constant, depth: usize) -> R<()> {
            self.node(depth)?;
            if let Constant::WideString(units) = value {
                self.rows(units.len(), depth + 1)?;
            }
            Ok(())
        }
        fn text(&mut self, value: &str, depth: usize) -> R<()> {
            self.node(depth)?;
            self.budget.text(value, self.span)?;
            if self.copy {
                self.budget.lineage_bytes(value.len(), self.span)?;
            }
            Ok(())
        }
    }
    value.visit_dyn_metadata(&mut Meter { budget, span, copy }, depth)
}

struct DefinitionBodyNodes {
    definition: SemanticDefinitionId,
    value_names: Vec<zil_base::NodeId>,
    nodes: Vec<body::OriginalBodyNode>,
}
struct InvocationFrame {
    inputs: Vec<Vec<u32>>,
    result: bool,
    input_addresses: usize,
    result_life: Option<SemanticLifeRecipe>,
}
struct Builder<'s, 'i, 'b, 'l> {
    inventory: &'i SourceDeclarationInventory<'s>,
    compiler_nominals: &'i [builtins::CompilerNominal],
    imports: &'i [(ModuleId, &'i [CoreImport])],
    budget: &'b mut Budget<'l>,
    owner: usize,
    lexical_offset: u32,
    implicit: Vec<SemanticImplicitLifetime>,
    bound_lives: Vec<Vec<String>>,
    invocations: Vec<InvocationFrame>,
    closures: Vec<closures::ClosureTemplate>,
    next_body_nodes: Vec<(ModuleId, u32)>,
    opaque_result: Option<SemanticTypeRecipe>,
    body_nodes: Vec<DefinitionBodyNodes>,
}
impl Builder<'_, '_, '_, '_> {
    fn id(&self, row: usize) -> SemanticDefinitionId {
        let source = &self.inventory.declarations[row];
        SemanticDefinitionId {
            module: source.module,
            index: source.declaration.0,
        }
    }
    fn push<T>(&mut self, values: &mut Vec<T>, value: T, span: Span) -> R<()> {
        self.budget.lineage_push(values, value, span)
    }
    fn text(&mut self, value: &str, span: Span) -> R<String> {
        self.budget.text(value, span)?;
        self.budget.lineage_bytes(value.len(), span)?;
        Ok(value.to_owned())
    }
    fn formal(&mut self, name: &str, kind: u8, span: Span) -> R<Option<SemanticFormalRef>> {
        let mut row = Some(self.owner);
        let mut distance = self.lexical_offset;
        while let Some(index) = row {
            let source = &self.inventory.declarations[index];
            for (ordinal, formal) in source.parameters().iter().enumerate() {
                self.budget.node(span, 0)?;
                self.budget.text(&formal.name, span)?;
                self.budget.text(name, span)?;
                if formal.name == name {
                    let matches = matches!(
                        (&formal.kind, kind),
                        (s::GenericParamKind::Type(_), 0)
                            | (s::GenericParamKind::Const(_), 1)
                            | (s::GenericParamKind::Lifetime, 2)
                    );
                    if !matches {
                        return Err(error(span, "semantic formal has the wrong original kind"));
                    }
                    return Ok(Some(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(distance),
                        ordinal: u32::try_from(ordinal)
                            .map_err(|_| error(span, "formal ordinal overflow"))?,
                    }));
                }
            }
            row = source.parent;
            distance = distance
                .checked_add(1)
                .ok_or_else(|| error(span, "lexical scope overflow"))?;
        }
        Ok(None)
    }
    fn named(&mut self, path: &[String], span: Span) -> R<usize> {
        let local = self.inventory.declarations[self.owner].module;
        let (module, name) = match path {
            [name] => (local, name),
            [alias, name] => {
                let mut target = None;
                for (module, edges) in self.imports {
                    self.budget.node(span, 0)?;
                    if *module != local {
                        continue;
                    }
                    for edge in *edges {
                        self.budget.text(alias, span)?;
                        self.budget.text(&edge.alias, span)?;
                        if edge.alias == *alias {
                            if target.replace(edge.module).is_some() {
                                return Err(error(span, "ambiguous original import alias"));
                            }
                        }
                    }
                }
                (
                    target.ok_or_else(|| {
                        error(span, "semantic type has no actual imported module")
                    })?,
                    name,
                )
            }
            _ => {
                return Err(unsupported(
                    span,
                    "semantic path needs actual original declaration resolution",
                ));
            }
        };
        let mut found = None;
        for (index, row) in self.inventory.declarations.iter().enumerate() {
            self.budget.node(span, 0)?;
            if row.module != module || row.parent.is_some() {
                continue;
            }
            let spelling = match &row.item.kind {
                s::ItemKind::Aggregate(a) => &a.name,
                s::ItemKind::Enum(e) => &e.name,
                s::ItemKind::Trait(t) => &t.name,
                s::ItemKind::TypeAlias(a) => &a.name,
                _ => continue,
            };
            self.budget.text(spelling, span)?;
            self.budget.text(name, span)?;
            if spelling == name {
                if found.replace(index).is_some() {
                    return Err(error(span, "duplicate original type declaration"));
                }
            }
        }
        let index =
            found.ok_or_else(|| error(span, "semantic type has no original declaration"))?;
        if module != local && !self.inventory.declarations[index].public {
            return Err(error(
                span,
                "semantic declaration is private to another module",
            ));
        }
        Ok(index)
    }
    fn implicit(&mut self, path: &[u32], span: Span) -> R<SemanticLifeRecipe> {
        self.budget
            .identity_work(self.implicit.len().saturating_mul(path.len() + 1), span)?;
        if let Some(row) = self.implicit.iter().find(|row| row.occurrence_path == path) {
            return Ok(SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: row.ordinal,
            });
        }
        self.budget.lineage_bytes(path.len() * 4, span)?;
        let ordinal = u32::try_from(self.implicit.len())
            .map_err(|_| error(span, "implicit occurrence overflow"))?;
        let row = SemanticImplicitLifetime {
            ordinal,
            occurrence_path: path.to_vec(),
        };
        let mut values = std::mem::take(&mut self.implicit);
        self.push(&mut values, row, span)?;
        self.implicit = values;
        Ok(SemanticLifeRecipe::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        })
    }
    fn identity(&mut self, row: usize) -> R<LifeDeclarationIdentity> {
        let original = &self.inventory.declarations[row];
        let source = &original.origin.identity;
        let mut output = LifeDeclarationIdentity {
            package: self.text(&source.package, original.item.span)?,
            module_path: Vec::new(),
            declaration_path: Vec::new(),
            generation_path: Vec::new(),
        };
        for part in &source.module_path {
            let part = self.text(part, original.item.span)?;
            self.push(&mut output.module_path, part, original.item.span)?;
        }
        for part in &source.declaration_path {
            let part = self.text(part, original.item.span)?;
            self.push(&mut output.declaration_path, part, original.item.span)?;
        }
        for number in &source.generation_path {
            self.push(&mut output.generation_path, *number, original.item.span)?;
        }
        if original.parent.is_some() {
            let member = self.text("member", original.item.span)?;
            self.push(&mut output.declaration_path, member, original.item.span)?;
            for ordinal in original.path.iter().skip(1) {
                let number = ordinal.to_string();
                let number = self.text(&number, original.item.span)?;
                self.push(&mut output.declaration_path, number, original.item.span)?;
            }
            if let Some(function) = original.function() {
                let name = self.text(&function.name, original.item.span)?;
                self.push(&mut output.declaration_path, name, original.item.span)?;
            }
        }
        Ok(output)
    }
}

/// Normalize requested original declaration templates from the complete source
/// inventory. The immutable AST ledger remains separate from these programs;
/// this function creates no actual instance, Ready receipt or fake CFG.
pub fn collect_original_semantics<'a>(
    modules: &[&'a s::Module],
    origins: &'a [ModuleDeclarationOrigins],
    imports: &[Vec<CoreImport>],
    remaining: &mut Limits,
) -> CompileResult<OriginalSemanticInventory<'a>> {
    let originals = collect_original_declarations_with_budget(modules, origins, remaining)?;
    let (compiler_nominals, compiler_definitions, compiler_declarations) =
        prelude::collect(modules, remaining)?;
    if imports.len() != modules.len() {
        return Err(vec![error(
            Span::default(),
            "original semantic import inventory is incomplete",
        )]);
    }
    let limits = remaining.clone();
    let mut budget = Budget::new(&limits);
    let result = (|| -> R<(Vec<(ModuleId, SemanticModule)>,Vec<closures::ClosureTemplate>,Vec<DefinitionBodyNodes>)> {
        budget.identity_work(modules.len(), Span::default())?;
        budget.lineage_bytes(
            modules
                .len()
                .checked_mul(std::mem::size_of::<(ModuleId, &[CoreImport])>())
                .ok_or_else(|| error(Span::default(), "semantic module mapping overflow"))?,
            Span::default(),
        )?;
        let mappings = modules
            .iter()
            .zip(imports)
            .map(|(module, imports)| (module.id, imports.as_slice()))
            .collect::<Vec<_>>();
        let mut output = Vec::new();
        budget.lineage_bytes(originals.next_by_module.len().checked_mul(std::mem::size_of::<(ModuleId, u32)>())
            .ok_or_else(|| error(Span::default(), "original body namespace allocation overflow"))?, Span::default())?;
        let mut builder = Builder {
            inventory: &originals,
            compiler_nominals: &compiler_nominals,
            imports: &mappings,
            budget: &mut budget,
            owner: 0,
            lexical_offset: 0,
            implicit: Vec::new(),
            bound_lives: Vec::new(),
            invocations: Vec::new(),
            closures: Vec::new(),
            next_body_nodes: originals.next_by_module.clone(),
            opaque_result: None,
            body_nodes:Vec::new(),
        };
        for module in modules {
            let mut definitions = Vec::new();
            for (index, row) in originals.declarations.iter().enumerate() {
                if row.module != module.id {
                    continue;
                }
                builder.owner = index;
                builder.implicit.clear();
                if let Some(definition) = builder.definition(index)? {
                    builder.push(&mut definitions, definition, row.item.span)?;
                }
            }
            for closure in &builder.closures {
                if closure.definition.id.module == module.id {
                    prepay_recipe(&closure.definition,builder.budget,module.span,0)?;
                    builder.budget.lineage_push(&mut definitions, closure.definition.clone(), module.span)?;
                }
            }
            builder.push(
                &mut output,
                (
                    module.id,
                    SemanticModule {
                        definitions,
                        instances: Vec::new(),
                        sites: Vec::new(),
                        dyn_scopes: Vec::new(),
                    },
                ),
                module.span,
            )?;
        }
        for definition in compiler_definitions {
            let module=output.first_mut()
                .ok_or_else(||error(Span::default(),"compiler semantic program directory is absent"))?;
            builder.budget.lineage_push(&mut module.1.definitions, definition, Span::default())?;
        }
        let closures = std::mem::take(&mut builder.closures);
        let body_nodes = std::mem::take(&mut builder.body_nodes);
        let namespaces = std::mem::take(&mut builder.next_body_nodes);
        drop(builder);
        budget.drop_lineage_vec(mappings, Span::default())?;
        budget.drop_lineage_vec(namespaces, Span::default())?;
        Ok((output,closures,body_nodes))
    })();
    *remaining = budget.remaining_limits();
    result
        .map(
            |(modules, closures, body_nodes)| OriginalSemanticInventory {
                originals,
                modules,
                closures,
                compiler_nominals,
                compiler_declarations,
                body_nodes,
            },
        )
        .map_err(|e| vec![e])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_context::{self, DeclarationPlacement, SourceIdentityContext};
    use std::collections::BTreeMap;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn original_templates_preserve_unused_formals_self_fields_and_actual_bodies() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(7),
            "original-semantics.zil",
            "struct Marker<life a,life b,#N:usize>{code:i32}trait Read<life a>{fn read(self:&Self)->&'a [u8];}impl<life a> Read<'a> for Marker<'a,'a,4usize>{fn read(self:&Self)->&'a [u8]{return \"abc\"}}fn keep<life a,life b>(value:Marker<'a,'b,4usize>)->Marker<'a,'b,4usize>{return value}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut remaining = limits.clone();
        let inventory =
            collect_original_semantics(&[&parsed], &origins, &[Vec::new()], &mut remaining)
                .unwrap();
        let module = &inventory.modules[0].1;
        assert!(module.instances.is_empty() && module.sites.is_empty());
        let marker = module
            .definitions
            .iter()
            .find(|row| matches!(row.body, SemanticDefinitionBody::Nominal { .. }))
            .unwrap();
        assert_eq!(marker.parameters.len(), 3);
        assert!(matches!(marker.parameters[0], SemanticFormal::Lifetime));
        assert!(matches!(
            &marker.parameters[2],
            SemanticFormal::Const {
                carrier: SemanticTypeRecipe::Atom(Type::Usize)
            }
        ));
        let method = module
            .definitions
            .iter()
            .find(|row| row.kind == SemanticDefinitionKind::Method)
            .unwrap();
        assert!(method.parent.is_some());
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &method.body else {
            unreachable!()
        };
        assert!(
            matches!(&signature.result,SemanticTypeRecipe::Address{lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(1),ordinal:0})),inner,..} if matches!(inner.as_ref(),SemanticTypeRecipe::Slice(_)))
        );
        assert!(
            inventory
                .originals
                .declarations
                .iter()
                .any(|row| row.body().is_some())
        );
        assert!(
            inventory
                .originals
                .declarations
                .iter()
                .any(|row| row.function().is_some() && row.body().is_none())
        );
        assert!(remaining.max_analysis_steps < limits.max_analysis_steps);
    }
    #[test]
    fn original_callable_constraints_keep_outer_formals_and_nested_quantifiers_separate() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(2),
            "where.zil",
            "fn forward<life a,F>(f:&F)->&'a i32 where F:Fn()->&'a i32{return f()}fn scoped<life a,F:for<life a>Fn(&'a i32)->&'a i32>(f:&F){}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut remaining = limits.clone();
        let inventory =
            collect_original_semantics(&[&parsed], &origins, &[Vec::new()], &mut remaining)
                .unwrap();
        let definitions = &inventory.modules[0].1.definitions;
        let SemanticRequirementRecipe::Callable { ty, signature, .. } =
            &definitions[0].requirements[0]
        else {
            panic!("where F:Fn must retain its actual callable constraint")
        };
        assert!(matches!(
            ty,
            SemanticTypeRecipe::Formal(SemanticFormalRef { ordinal: 1, .. })
        ));
        assert!(matches!(
            signature.result,
            SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(
                    SemanticFormalRef { ordinal: 0, .. }
                )),
                ..
            }
        ));
        let SemanticFormal::Type { requirements } = &definitions[1].parameters[1] else {
            panic!()
        };
        let SemanticRequirementRecipe::ForAll {
            lifetimes: 1,
            requirements,
        } = &requirements[0]
        else {
            panic!()
        };
        let SemanticRequirementRecipe::Callable { signature, .. } = &requirements[0] else {
            panic!()
        };
        assert!(matches!(
            signature.result,
            SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Bound {
                    depth: 0,
                    index: 0
                }),
                ..
            }
        ));
        assert!(matches!(
            signature.parameters[0].ty,
            SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Bound {
                    depth: 0,
                    index: 0
                }),
                ..
            }
        ));
        assert!(matches!(
            definitions[1].parameters[0],
            SemanticFormal::Lifetime
        ));
    }
    #[test]
    fn callable_anonymous_invocations_do_not_become_definition_formals_or_bound_frames() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(2),
            "elided-bound.zil",
            "fn inferred<F:Fn(&i32)->&i32>(f:&F){}fn named<life a,F:Fn(&'a i32)->&i32>(f:&F){}fn code(value:for<life a>for<life b>fn(&'a i32,&'b i32)->&'a i32){}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut remaining = limits.clone();
        let inventory =
            collect_original_semantics(&[&parsed], &origins, &[Vec::new()], &mut remaining)
                .unwrap();
        let definitions = &inventory.modules[0].1.definitions;
        let SemanticFormal::Type { requirements } = &definitions[0].parameters[0] else {
            panic!()
        };
        let SemanticRequirementRecipe::Callable { signature, .. } = &requirements[0] else {
            panic!()
        };
        assert_eq!(signature.invocation_lifetimes, 1);
        assert_eq!(signature.bound_lifetimes, 0);
        assert!(matches!(
            signature.result,
            SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Invocation(0)),
                ..
            }
        ));
        let SemanticFormal::Type { requirements } = &definitions[1].parameters[1] else {
            panic!()
        };
        let SemanticRequirementRecipe::Callable { signature, .. } = &requirements[0] else {
            panic!()
        };
        assert_eq!(signature.invocation_lifetimes, 0);
        assert!(matches!(
            signature.result,
            SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(
                    SemanticFormalRef { ordinal: 0, .. }
                )),
                ..
            }
        ));
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definitions[2].body
        else {
            panic!()
        };
        let SemanticTypeRecipe::Code { signature, .. } = &signature.parameters[0].ty else {
            panic!()
        };
        assert_eq!(signature.bound_lifetimes, 2);
        assert_eq!(signature.invocation_lifetimes, 0);
        assert!(matches!(
            signature.result,
            SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Bound {
                    depth: 0,
                    index: 0
                }),
                ..
            }
        ));
        assert!(matches!(
            signature.parameters[1].ty,
            SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Bound {
                    depth: 0,
                    index: 1
                }),
                ..
            }
        ));
    }
    #[test]
    fn const_actual_keeps_the_real_dependent_carrier_in_full_mixed_formal_order() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(2),
            "carrier.zil",
            "struct Sample<T,#N:T>{code:T}struct Forward<#N:T,T>{code:T}fn value(x:Sample<u8,255>,y:Forward<65535,u16>)->u8{return x.code}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut remaining = limits.clone();
        let inventory =
            collect_original_semantics(&[&parsed], &origins, &[Vec::new()], &mut remaining)
                .unwrap();
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
            &inventory.modules[0].1.definitions[2].body
        else {
            panic!()
        };
        let SemanticTypeRecipe::Nominal { arguments, .. } = &signature.parameters[0].ty else {
            panic!()
        };
        assert!(
            matches!(&arguments[1], SemanticActualRecipe::Const { carrier: SemanticTypeRecipe::Atom(Type::U8), value: SemanticConstRecipe::IntegerLiteral(text) } if text == "255")
        );
        let SemanticTypeRecipe::Nominal { arguments, .. } = &signature.parameters[1].ty else {
            panic!()
        };
        assert!(
            matches!(&arguments[0], SemanticActualRecipe::Const { carrier: SemanticTypeRecipe::Atom(Type::U16), value: SemanticConstRecipe::IntegerLiteral(text) } if text == "65535")
        );
        assert!(matches!(
            &arguments[1],
            SemanticActualRecipe::Type(SemanticTypeRecipe::Atom(Type::U16))
        ));
    }
}
