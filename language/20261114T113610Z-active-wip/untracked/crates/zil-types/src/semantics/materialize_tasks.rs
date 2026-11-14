//! Task-specific joint domains. Child indices select rows directly; equal
//! caller images never select or merge original intrinsic classes.
use super::domains::{Domain, Selector};
use super::*;

struct Sequence {
    selectors: Vec<Selector>,
    images: Vec<SemanticClassExpr>,
}
impl Eval<'_, '_> {
    fn task_sequence(
        &mut self,
        scope: SemanticInstanceId,
        sequence: &SemanticClassSequenceRecipe,
        depth: usize,
    ) -> R<Sequence> {
        self.node(depth)?;
        self.fee(sequence.parts.len())?;
        let mut selectors = Vec::new();
        let mut images = Vec::new();
        for part in &sequence.parts {
            match part {
                SemanticClassSequencePart::One(life) => {
                    selectors.push(self.outer_selector(scope, life)?);
                    images.push(self.life(scope, life)?);
                }
                SemanticClassSequencePart::AllClasses { ty, path } => {
                    self.fee(path.len())?;
                    if !path.is_empty() {
                        return Err(unknown(
                            "Task class sequence awaits its exact original child-domain path",
                        ));
                    }
                    let child = self.type_domain(scope, ty, None, depth + 1)?;
                    let SemanticTypeValue::Resolved { life, classes, .. } = child.value else {
                        unreachable!()
                    };
                    if life.binder_count as usize != classes.len()
                        || child.selectors.len() != classes.len()
                    {
                        return Err(bad("Task class sequence omits its whole input domain"));
                    }
                    self.fee(classes.len())?;
                    selectors.extend(child.selectors);
                    images.extend(classes);
                }
            }
            if selectors.len() > self.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "Task class sequence node limit exceeded",
                    None,
                ));
            }
        }
        Ok(Sequence { selectors, images })
    }
    fn task_index(
        &mut self,
        domain: &Sequence,
        selector: &Selector,
        image: &SemanticClassExpr,
    ) -> R<u32> {
        self.find_selector(&domain.selectors, &domain.images, selector, image)?
            .ok_or_else(|| {
                unknown("Task result or child class has no proved original input-domain selector")
            })
    }
    fn task_sequence_map(&mut self, domain: &Sequence, child: &Sequence) -> R<Vec<u32>> {
        self.fee(child.selectors.len())?;
        if child.selectors.len() != child.images.len() {
            return Err(bad("Task child sequence changes selector/image arity"));
        }
        let mut result = Vec::new();
        for (selector, image) in child.selectors.iter().zip(&child.images) {
            result.push(self.task_index(domain, selector, image)?);
        }
        Ok(result)
    }
    pub(super) fn task_shape(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticRetainedShapeRecipe,
        depth: usize,
    ) -> R<(RetainedLifeShape, Vec<SemanticClassExpr>)> {
        self.node(depth)?;
        self.fee(recipe.domain.parts.len())?;
        let mut domain = Sequence {
            selectors: Vec::new(),
            images: Vec::new(),
        };
        for part in &recipe.domain.parts {
            match part {
                SemanticClassSequencePart::One(life) => {
                    let selector = self.outer_selector(scope, life)?;
                    let image = self.life(scope, life)?;
                    self.intern_selector(
                        &mut domain.selectors,
                        &mut domain.images,
                        &selector,
                        &image,
                    )?;
                }
                SemanticClassSequencePart::AllClasses { ty, path } => {
                    self.fee(path.len())?;
                    if !path.is_empty() {
                        return Err(unknown(
                            "Task input domain awaits its complete original child path",
                        ));
                    }
                    let child = self.type_domain(scope, ty, None, depth + 1)?;
                    self.join_domain(&mut domain.selectors, &mut domain.images, &child)?;
                }
            }
        }
        let binder_count = u32::try_from(domain.selectors.len())
            .map_err(|_| bad("Task complete joint domain overflows"))?;
        self.fee(recipe.ports.len())?;
        let mut ports = Vec::new();
        for part in &recipe.ports {
            match part {
                SemanticPortPart::One { leaf, mode, class } => {
                    let SemanticTypeValue::Resolved { ty: leaf, .. } =
                        self.ty(scope, leaf, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    let selector = self.outer_selector(scope, class)?;
                    let image = self.life(scope, class)?;
                    let binder = self.task_index(&domain, &selector, &image)?;
                    ports.push(RetainedBorrowPort {
                        leaf,
                        mode: *mode,
                        binder,
                    });
                }
                SemanticPortPart::FromType { ty, classes } => {
                    let Domain { value, .. } = self.type_domain(scope, ty, None, depth + 1)?;
                    let SemanticTypeValue::Resolved { ty, life, .. } = value else {
                        unreachable!()
                    };
                    let sequence = self.task_sequence(scope, classes, depth + 1)?;
                    if sequence.selectors.len() != life.binder_count as usize {
                        return Err(bad("Task input port map omits complete child indices"));
                    }
                    let map = self.task_sequence_map(&domain, &sequence)?;
                    let projected = crate::tasks::context_ports::from_type(
                        self.module,
                        &ty,
                        &life,
                        self.limits,
                        self.left,
                    )?;
                    self.fee(projected.len())?;
                    for projected in projected {
                        let binder = *map.get(projected.port.binder as usize).ok_or_else(|| {
                            bad("Task input port leaves its complete child domain")
                        })?;
                        // Port rows are physical occurrences. Even identical
                        // leaf/mode/binder triples retain separate ordinals.
                        ports.push(RetainedBorrowPort {
                            binder,
                            ..projected.port
                        });
                    }
                }
            }
            if ports.len() > self.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "Task port occurrence limit exceeded",
                    None,
                ));
            }
        }
        self.fee(recipe.retained.len())?;
        let mut retained = Vec::new();
        for part in &recipe.retained {
            match part {
                SemanticRetainedPart::One {
                    path,
                    child_class,
                    parent,
                } => {
                    let selector = self.outer_selector(scope, parent)?;
                    let image = self.life(scope, parent)?;
                    let binder = self.task_index(&domain, &selector, &image)?;
                    self.fee(path.len().saturating_add(1))?;
                    retained.push(StorageRetainedBinding {
                        path: path.clone(),
                        class: *child_class,
                        binder,
                    });
                }
                SemanticRetainedPart::All { path, parents } => {
                    let sequence = self.task_sequence(scope, parents, depth + 1)?;
                    let map = self.task_sequence_map(&domain, &sequence)?;
                    for (class, binder) in map.into_iter().enumerate() {
                        self.fee(path.len().saturating_add(1))?;
                        retained.push(StorageRetainedBinding {
                            path: path.clone(),
                            class: u32::try_from(class)
                                .map_err(|_| bad("Task retained child ordinal overflows"))?,
                            binder,
                        });
                    }
                }
            }
            if retained.len() > self.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "Task whole-child row limit exceeded",
                    None,
                ));
            }
        }
        self.fee(recipe.nominal.len())?;
        let mut nominal_arguments = Vec::new();
        for part in &recipe.nominal {
            match part {
                SemanticNominalPart::One {
                    path,
                    declaration,
                    argument,
                    class,
                } => {
                    let selector = self.outer_selector(scope, class)?;
                    let image = self.life(scope, class)?;
                    let binder = self.task_index(&domain, &selector, &image)?;
                    self.fee(path.len().saturating_add(1))?;
                    nominal_arguments.push(NominalLifeArgument {
                        path: path.clone(),
                        declaration: *declaration,
                        argument: *argument,
                        binder,
                    });
                }
                SemanticNominalPart::FromType { path, ty, classes } => {
                    let SemanticTypeValue::Resolved { life, .. } =
                        self.ty(scope, ty, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    let sequence = self.task_sequence(scope, classes, depth + 1)?;
                    if sequence.selectors.len() != life.binder_count as usize {
                        return Err(bad("Task nominal map omits complete child indices"));
                    }
                    let map = self.task_sequence_map(&domain, &sequence)?;
                    for row in life.nominal_arguments {
                        let binder = *map.get(row.binder as usize).ok_or_else(|| {
                            bad("Task nominal child leaves its full intrinsic domain")
                        })?;
                        self.fee(path.len().saturating_add(row.path.len()).saturating_add(1))?;
                        let mut full = path.clone();
                        full.extend(row.path);
                        nominal_arguments.push(NominalLifeArgument {
                            path: full,
                            declaration: row.declaration,
                            argument: row.argument,
                            binder,
                        });
                    }
                }
            }
            if nominal_arguments.len() > self.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "Task nominal row limit exceeded",
                    None,
                ));
            }
        }
        Ok((
            RetainedLifeShape {
                binder_count,
                ports,
                retained,
                nominal_arguments,
            },
            domain.images,
        ))
    }
}
