//! Original sealed type expressions. These recipes preserve public operands;
//! they do not prove that a worker, payload, guard or environment is closed.
use super::*;

pub(super) struct CompilerNominal {
    pub module: ModuleId,
    /// The actual binding installed by the compiler prelude in this module.
    pub name: String,
    pub definition: SemanticDefinitionId,
    pub declaration: LifeDeclarationId,
}

impl Builder<'_, '_, '_, '_> {
    fn builtin_shadow(&mut self, names: &[String], span: Span) -> R<bool> {
        let Some(first) = names.first() else {
            return Ok(false);
        };
        if self.formal(first, 0, span)?.is_some() {
            return Ok(true);
        }
        let module = self.inventory.declarations[self.owner].module;
        for row in &self.inventory.declarations {
            self.budget.node(span, 0)?;
            if row.module != module || row.parent.is_some() {
                continue;
            }
            let declared = match &row.item.kind {
                s::ItemKind::Aggregate(value) => Some(value.name.as_str()),
                s::ItemKind::Enum(value) => Some(value.name.as_str()),
                s::ItemKind::Trait(value) => Some(value.name.as_str()),
                s::ItemKind::TypeAlias(value) => Some(value.name.as_str()),
                s::ItemKind::Import(value) => value
                    .alias
                    .as_deref()
                    .or_else(|| value.path.last().map(String::as_str)),
                _ => None,
            };
            if let Some(declared) = declared {
                self.budget.text(declared, span)?;
                self.budget.text(first, span)?;
                if declared == first {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn compiler_nominal(
        &mut self,
        names: &[String],
        span: Span,
    ) -> R<Option<SemanticDefinitionId>> {
        let module = self.inventory.declarations[self.owner].module;
        let mut selected = None;
        for row in self.compiler_nominals {
            self.budget.node(span, 0)?;
            self.budget.text(&row.name, span)?;
            for name in names {
                self.budget.text(name, span)?;
            }
            if row.module == module && row.name.split('.').eq(names.iter().map(String::as_str)) {
                if !matches!(row.declaration, id if id == zil_ir::builtin_life::OPTION || id == zil_ir::builtin_life::POLL)
                {
                    return Err(error(
                        span,
                        "compiler generic type lacks its actual registered family identity",
                    ));
                }
                if selected.replace(row.definition).is_some() {
                    return Err(error(
                        span,
                        "compiler type binding has competing actual declarations",
                    ));
                }
            }
        }
        Ok(selected)
    }

    fn sealed_desc(
        &mut self,
        inner: SemanticTypeRecipe,
        path: &[u32],
        span: Span,
        payload: bool,
    ) -> R<SemanticTypeRecipe> {
        Ok(if payload {
            inner
        } else {
            self.budget
                .lineage_bytes(std::mem::size_of::<SemanticTypeRecipe>(), span)?;
            SemanticTypeRecipe::Desc {
                lifetime: SemanticOuterLifeRecipe::Class(self.implicit(path, span)?),
                inner: Box::new(inner),
            }
        })
    }

    pub(super) fn builtin_type(
        &mut self,
        ty: &s::TypeRef,
        path: &mut Vec<u32>,
        depth: usize,
        payload: bool,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.budget.node(ty.span, depth)?;
        if let s::TypeKind::Optional(inner) = &ty.kind {
            let names = [self.text("Option", ty.span)?];
            let definition = self.compiler_nominal(&names, ty.span)?.ok_or_else(|| {
                error(
                    ty.span,
                    "optional type has no actual compiler prelude declaration",
                )
            })?;
            path.push(0);
            let result = self.ty(inner, path, depth + 1, false);
            path.pop();
            let mut arguments = Vec::new();
            self.push(&mut arguments, SemanticActualRecipe::Type(result?), ty.span)?;
            return Ok(Some(SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            }));
        }
        let s::TypeKind::Named { path: names, args } = &ty.kind else {
            return Ok(None);
        };
        let compiler = self.compiler_nominal(names, ty.span)?;
        let sealed = matches!(names.as_slice(), [name] if matches!(name.as_str(), "Task" | "MutexCreateError" | "DynCreateError"));
        if compiler.is_none() && !sealed {
            return Ok(None);
        }
        if self.builtin_shadow(names, ty.span)? {
            return Ok(None);
        }
        if let Some(definition) = compiler {
            let [s::GenericArg::Type(inner)] = args.as_slice() else {
                return Err(error(
                    ty.span,
                    "compiler Option/Poll requires its complete actual Type operand",
                ));
            };
            path.push(0);
            let result = self.ty(inner, path, depth + 1, false);
            path.pop();
            let mut arguments = Vec::new();
            self.push(&mut arguments, SemanticActualRecipe::Type(result?), ty.span)?;
            return Ok(Some(SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            }));
        }
        let [name] = names.as_slice() else {
            return Ok(None);
        };
        if args.is_empty() && matches!(name.as_str(), "MutexCreateError" | "DynCreateError") {
            // These exact nullary language types already have a canonical
            // physical declaration in type_ref. No generic subtree or hidden
            // environment is replaced by an empty shape here.
            self.budget.lineage_bytes(
                std::mem::size_of::<SemanticExactTypeRecipe>()
                    .saturating_add(std::mem::size_of::<(String, Vec<Type>)>())
                    .saturating_add(128),
                ty.span,
            )?;
            let actual = crate::type_ref(ty);
            let Type::Enum { variants, .. } = &actual else {
                return Err(error(
                    ty.span,
                    "sealed error carrier lost its actual enum declaration",
                ));
            };
            if !matches!(variants.as_slice(), [(allocation, fields)] if allocation == "Allocation" && fields.is_empty())
            {
                return Err(error(
                    ty.span,
                    "sealed error carrier has another actual declaration",
                ));
            }
            let exact = SemanticExactTypeRecipe {
                ty: actual,
                life: StorageLifeShape {
                    binder_count: 0,
                    slots: Vec::new(),
                    retained: Vec::new(),
                    nominal_arguments: Vec::new(),
                },
                classes: Vec::new(),
            };
            prepay_recipe(&exact, self.budget, ty.span, depth + 1)?;
            return Ok(Some(SemanticTypeRecipe::Exact(Box::new(exact))));
        }
        if name == "Task" {
            let (result, context) = match args.as_slice() {
                [s::GenericArg::Type(result)] => (result, None),
                [s::GenericArg::Type(result), s::GenericArg::Type(context)] => {
                    (result, Some(context))
                }
                _ => {
                    return Err(error(
                        ty.span,
                        "Task type requires its actual result and optional complete input Context",
                    ));
                }
            };
            path.push(0);
            let result = self.ty(result, path, depth + 1, false);
            path.pop();
            let result = result?;
            let retained = if let Some(context) = context {
                self.task_context(context, &result, path, depth + 1)?
            } else {
                // Task<R> explicitly declares an empty INPUT context. Keep R
                // whole; its unknown/free result domain is not proved empty
                // or grounded by this type expression.
                SemanticRetainedShapeRecipe {
                    domain: SemanticClassSequenceRecipe { parts: Vec::new() },
                    ports: Vec::new(),
                    retained: Vec::new(),
                    nominal: Vec::new(),
                }
            };
            self.budget.lineage_bytes(
                std::mem::size_of::<SemanticTypeRecipe>()
                    .saturating_add(std::mem::size_of::<SemanticRetainedShapeRecipe>()),
                ty.span,
            )?;
            let task = SemanticTypeRecipe::Task {
                result: Box::new(result),
                retained: Box::new(retained),
            };
            return self.sealed_desc(task, path, ty.span, payload).map(Some);
        }
        Ok(None)
    }

    fn task_context(
        &mut self,
        context: &s::TypeRef,
        result: &SemanticTypeRecipe,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticRetainedShapeRecipe> {
        self.budget.node(context.span, depth)?;
        let fields = match &context.kind {
            s::TypeKind::Tuple(fields) => fields.as_slice(),
            _ => std::slice::from_ref(context),
        };
        let mut domain = Vec::new();
        let mut ports = Vec::new();
        for (index, field) in fields.iter().enumerate() {
            path.extend([1, index as u32]);
            let value = self.ty(field, path, depth + 1, false);
            path.pop();
            path.pop();
            let value = value?;
            let SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(class),
                ..
            } = &value
            else {
                return Err(unsupported(
                    field.span,
                    "Task input Context requires its complete actual address ports",
                ));
            };
            self.budget.identity_work(domain.len(), field.span)?;
            for old in &domain {
                prepay_recipe(old, self.budget, field.span, depth + 1)?;
            }
            prepay_recipe(class, self.budget, field.span, depth + 1)?;
            let part = SemanticClassSequencePart::One(class.clone());
            if !domain.contains(&part) {
                self.push(&mut domain, part, field.span)?;
            }
            prepay_recipe(class, self.budget, field.span, depth + 1)?;
            let class = class.clone();
            let mode = if *mutable {
                StoredBorrowMode::ExclusiveAddress
            } else {
                StoredBorrowMode::SharedAddress
            };
            self.budget
                .lineage_bytes(std::mem::size_of::<SemanticTypeRecipe>(), field.span)?;
            self.push(
                &mut ports,
                SemanticPortPart::One {
                    leaf: Box::new(value),
                    mode,
                    class,
                },
                field.span,
            )?;
        }
        if ports.is_empty() {
            return Err(error(
                context.span,
                "Task explicit Context has no actual port",
            ));
        }
        // This is an equality obligation into the public input domain, not a
        // new domain copied from R. Zero result classes emit no rows; nonzero
        // result classes must independently match actual Context classes.
        prepay_recipe(result, self.budget, context.span, depth + 1)?;
        self.budget.lineage_bytes(
            std::mem::size_of::<SemanticClassSequencePart>()
                .saturating_add(std::mem::size_of::<SemanticTypeRecipe>())
                .saturating_add(std::mem::size_of::<SemanticRetainedPart>())
                .saturating_add(std::mem::size_of::<LifeTypeProjection>()),
            context.span,
        )?;
        let result_classes = SemanticClassSequenceRecipe {
            parts: vec![SemanticClassSequencePart::AllClasses {
                ty: Box::new(result.clone()),
                path: Vec::new(),
            }],
        };
        Ok(SemanticRetainedShapeRecipe {
            domain: SemanticClassSequenceRecipe { parts: domain },
            ports,
            retained: vec![SemanticRetainedPart::All {
                path: vec![LifeTypeProjection::Result],
                parents: result_classes,
            }],
            nominal: Vec::new(),
        })
    }
}
