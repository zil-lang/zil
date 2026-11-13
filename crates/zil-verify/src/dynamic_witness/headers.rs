//! Check original signature occurrences against an actual function header.
//! Matching images is insufficient: every actual input header binder retains
//! the intrinsic origin(s) established by the independently rebuilt instance.
use super::*;

#[derive(Default)]
struct Directory {
    bindings: Vec<(StoragePosition, terms::LifeTerm)>,
    classes: Vec<(SignatureLifeClass, terms::LifeTerm)>,
}
impl Directory {
    fn class_map(
        value: &terms::Value,
        path: &[LifeTypeProjection],
        count: u32,
        b: &mut Budget,
    ) -> Check<Vec<terms::LifeTerm>> {
        if count as usize > b.limits.max_nodes {
            return Err(limit("header child class domain exceeded"));
        }
        b.spend((count as usize).saturating_add(value.life.retained.len()))?;
        let mut result = Vec::with_capacity(count as usize);
        if path.is_empty() {
            if value.terms.len() != count as usize {
                return Err(bad("header root class directory is incomplete"));
            }
            result.extend_from_slice(&value.terms);
        } else {
            for class in 0..count {
                b.spend(value.life.retained.len())?;
                let mut rows = value
                    .life
                    .retained
                    .iter()
                    .filter(|row| row.path == path && row.class == class);
                let row = rows
                    .next()
                    .ok_or_else(|| bad("header omits a complete typed child class occurrence"))?;
                if rows.next().is_some() {
                    return Err(bad("header repeats a typed child class occurrence"));
                }
                result.push(value.term(row.binder, b)?);
            }
        }
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        module: &TypedModule,
        value: &terms::Value,
        ty: &Type,
        base: StorageBase,
        physical: &mut Vec<ContractProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        b.spend(
            physical
                .len()
                .saturating_add(semantic.len())
                .saturating_add(1),
        )?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("original signature occurrence depth exceeded"));
        }
        let count = match ty {
            Type::BorrowedStorage { life, .. } => Some(life.binder_count),
            Type::Closure(_) | Type::Task { .. } | Type::Dyn { .. } | Type::OwnedDyn { .. } => {
                Some(values::intrinsic(module, ty, b)?.count)
            }
            _ => None,
        };
        if let Some(count) = count {
            let terms = Self::class_map(value, semantic, count, b)?;
            if let Type::Task { retained_life, .. } = ty {
                b.spend(retained_life.ports.len())?;
                for (port, row) in retained_life.ports.iter().enumerate() {
                    let term = *terms
                        .get(row.binder as usize)
                        .ok_or_else(|| bad("Task port is outside its full actual class domain"))?;
                    let mut path = physical.clone();
                    path.push(ContractProjection::RetainedPort(port as u32));
                    self.bindings.push((StoragePosition { base, path }, term));
                }
            }
            for (class, term) in terms.into_iter().enumerate() {
                b.spend(semantic.len().saturating_add(1))?;
                self.classes.push((
                    SignatureLifeClass {
                        base,
                        path: semantic.clone(),
                        class: class as u32,
                    },
                    term,
                ));
            }
            return Ok(());
        }
        match ty {
            Type::Address { inner, .. } | Type::Desc(inner) => {
                let path = physical
                    .iter()
                    .map(|step| match step {
                        ContractProjection::Place(p) => Ok(*p),
                        _ => Err(bad("raw header borrow has no physical inline source path")),
                    })
                    .collect::<Check<Vec<_>>>()?;
                b.spend(value.life.slots.len())?;
                let mut rows = value.life.slots.iter().filter(|row| row.path == path);
                let row = rows
                    .next()
                    .ok_or_else(|| bad("original signature outer borrow occurrence is absent"))?;
                if rows.next().is_some() {
                    return Err(bad(
                        "original signature outer borrow occurrence is ambiguous",
                    ));
                }
                self.bindings.push((
                    StoragePosition {
                        base,
                        path: physical.clone(),
                    },
                    value.term(row.binder, b)?,
                ));
                semantic.push(LifeTypeProjection::Pointee);
                self.node(module, value, inner, base, physical, semantic, b, depth + 1)?;
                semantic.pop();
            }
            Type::Managed(inner) => {
                semantic.push(LifeTypeProjection::Pointee);
                self.node(module, value, inner, base, physical, semantic, b, depth + 1)?;
                semantic.pop();
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                for (index, field) in fields.iter().enumerate() {
                    self.field(
                        module,
                        value,
                        field,
                        base,
                        PlaceProjection::Field(index as u32),
                        physical,
                        semantic,
                        b,
                        depth,
                    )?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                for (index, (_, field)) in fields.iter().enumerate() {
                    self.field(
                        module,
                        value,
                        field,
                        base,
                        PlaceProjection::Field(index as u32),
                        physical,
                        semantic,
                        b,
                        depth,
                    )?;
                }
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    b.spend(fields.len())?;
                    for (index, field) in fields.iter().enumerate() {
                        self.field(
                            module,
                            value,
                            field,
                            base,
                            PlaceProjection::Variant(VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            }),
                            physical,
                            semantic,
                            b,
                            depth,
                        )?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                for (arm, field) in [(true, error.as_ref()), (false, result.as_ref())] {
                    self.field(
                        module,
                        value,
                        field,
                        base,
                        PlaceProjection::ErrorArm(arm),
                        physical,
                        semantic,
                        b,
                        depth,
                    )?;
                }
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element } => {
                semantic.push(LifeTypeProjection::Element);
                self.node(
                    module,
                    value,
                    element,
                    base,
                    physical,
                    semantic,
                    b,
                    depth + 1,
                )?;
                semantic.pop();
            }
            _ => values::closed(module, ty, b)?,
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn field(
        &mut self,
        module: &TypedModule,
        value: &terms::Value,
        ty: &Type,
        base: StorageBase,
        step: PlaceProjection,
        physical: &mut Vec<ContractProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        physical.push(ContractProjection::Place(step));
        semantic.push(LifeTypeProjection::Place(step));
        self.node(module, value, ty, base, physical, semantic, b, depth + 1)?;
        physical.pop();
        semantic.pop();
        Ok(())
    }
}
/// Canonical expected positions, still pending actual SSA/access/effect and
/// Static proofs. This is not an actual Typed runtime header certificate.
pub(super) fn expected(
    module: &TypedModule,
    parameters: &[terms::Value],
    result: &terms::Value,
    b: &mut Budget,
) -> Check<requests::Header> {
    let directory = directory(module, parameters, result, b)?;
    let mut origins = Vec::with_capacity(
        directory
            .bindings
            .len()
            .saturating_add(directory.classes.len()),
    );
    b.spend(
        directory
            .bindings
            .len()
            .saturating_add(directory.classes.len()),
    )?;
    for (position, term) in &directory.bindings {
        b.spend(position.path.len())?;
        origins.push(requests::Occurrence::Borrow {
            position: position.clone(),
            binder: term.image,
            origin: symbol(*term),
        });
    }
    for (position, term) in &directory.classes {
        b.spend(position.path.len())?;
        origins.push(requests::Occurrence::Class {
            position: position.clone(),
            binder: term.image,
            origin: symbol(*term),
        });
    }
    Ok(requests::Header {
        bindings: directory
            .bindings
            .into_iter()
            .map(|(position, term)| StorageLifeBinding {
                position,
                binder: term.image,
            })
            .collect(),
        static_bindings: vec![],
        classes: directory
            .classes
            .into_iter()
            .map(|(position, term)| SignatureLifeClassBinding {
                position,
                binder: term.image,
            })
            .collect(),
        origins,
    })
}
fn symbol(term: terms::LifeTerm) -> requests::Intrinsic {
    let scope = match term.origin.owner {
        terms::Owner::Function(function) => requests::IntrinsicScope::Function(function),
        terms::Owner::Witness(table, node) => requests::IntrinsicScope::Witness { table, node },
    };
    let root = match term.origin.root {
        terms::Root::Receiver => requests::IntrinsicRoot::Receiver,
        terms::Root::DeclarationType(i) => requests::IntrinsicRoot::DeclarationType(i),
        terms::Root::MethodType(i) => requests::IntrinsicRoot::MethodType(i),
        terms::Root::ApplicationType(i) => requests::IntrinsicRoot::ApplicationType(i),
        terms::Root::Lexical(binder) => requests::IntrinsicRoot::Lexical(binder),
    };
    requests::Intrinsic {
        scope,
        root,
        class: term.origin.class,
    }
}
fn directory(
    module: &TypedModule,
    parameters: &[terms::Value],
    result: &terms::Value,
    b: &mut Budget,
) -> Check<Directory> {
    let mut directory = Directory::default();
    b.spend(parameters.len().saturating_add(1))?;
    for (index, value) in parameters.iter().chain(std::iter::once(result)).enumerate() {
        dynamic_metadata::recipe(&value.raw, b)?;
        let base = if index == parameters.len() {
            StorageBase::Result
        } else {
            StorageBase::Parameter(index as u32)
        };
        directory.node(
            module,
            value,
            &value.raw.ty,
            base,
            &mut vec![],
            &mut vec![],
            b,
            0,
        )?;
    }
    Ok(directory)
}
/// Every actual variable header occurrence must have the expected intrinsic
/// origin. Inputs establish that relation; matching Context images never does.
/// Constant outputs are retained as pending Static obligations for the runtime
/// kernel, and never manufacture a variable map row.
pub(super) fn function(
    module: &TypedModule,
    function: &TypedFunction,
    parameters: &[terms::Value],
    result: &terms::Value,
    b: &mut Budget,
) -> Check<requests::Header> {
    checked_function(module, function, parameters, result, true, b)
}
/// Compare occurrences in a different request domain. The returned map is
/// actual header -> request image, induced by complete positions, never by
/// inversion of Context images. Intrinsic origins remain per occurrence.
pub(super) fn rebased_function(
    module: &TypedModule,
    function: &TypedFunction,
    parameters: &[terms::Value],
    result: &terms::Value,
    b: &mut Budget,
) -> Check<(requests::Header, Vec<DynLifeEmbedding>)> {
    let header = checked_function(module, function, parameters, result, false, b)?;
    let Some(actual) = &function.storage_life else {
        return Ok((header, vec![]));
    };
    let mut embedding = Vec::new();
    for row in &actual.bindings {
        b.spend(
            header
                .bindings
                .len()
                .saturating_add(embedding.len())
                .saturating_add(1),
        )?;
        let to = header
            .bindings
            .iter()
            .find(|r| r.position == row.position)
            .ok_or_else(|| bad("rebased actual borrow lacks its original occurrence"))?
            .binder;
        image(&mut embedding, row.binder, to)?;
    }
    for row in &actual.classes {
        b.spend(
            header
                .classes
                .len()
                .saturating_add(embedding.len())
                .saturating_add(1),
        )?;
        let to = header
            .classes
            .iter()
            .find(|r| r.position == row.position)
            .ok_or_else(|| bad("rebased actual class lacks its original occurrence"))?
            .binder;
        image(&mut embedding, row.binder, to)?;
    }
    Ok((header, embedding))
}
fn image(rows: &mut Vec<DynLifeEmbedding>, from: StorageBinder, to: StorageBinder) -> Check<()> {
    if let Some(row) = rows.iter().find(|r| r.from == from) {
        if row.to != to {
            return Err(bad(
                "one actual header image has inconsistent request occurrences",
            ));
        }
    } else {
        rows.push(DynLifeEmbedding { from, to });
    }
    Ok(())
}
fn checked_function(
    module: &TypedModule,
    function: &TypedFunction,
    parameters: &[terms::Value],
    result: &terms::Value,
    own_domain: bool,
    b: &mut Budget,
) -> Check<requests::Header> {
    if function.parameters.len() != parameters.len() || function.foreign.is_some() {
        return Err(bad(
            "original method does not select one complete executable actual header",
        ));
    }
    for (local, expected) in function.parameters.iter().zip(parameters) {
        let actual = function
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| bad("actual method parameter local is absent"))?;
        dynamic_metadata::ty(&actual.ty, b)?;
        dynamic_metadata::recipe(&expected.raw, b)?;
        if actual.ty != expected.raw.ty {
            return Err(bad(
                "actual ordered method parameter differs from its full original type",
            ));
        }
    }
    dynamic_metadata::ty(&function.return_type, b)?;
    if function.return_type != result.raw.ty {
        return Err(bad(
            "actual method result differs from its full original type",
        ));
    }
    let directory = directory(module, parameters, result, b)?;
    let Some(actual) = function.storage_life.as_ref() else {
        if directory.bindings.is_empty() && directory.classes.is_empty() {
            return Ok(requests::Header::default());
        }
        return Err(bad("actual method header occurrence relation is Unknown"));
    };
    signature_life::charge(actual, b)?;
    let mut mapping: Vec<(StorageBinder, terms::LifeTerm)> = vec![];
    for (position, term) in &directory.classes {
        b.spend(actual.classes.len().saturating_add(mapping.len()))?;
        let mut rows = actual
            .classes
            .iter()
            .filter(|row| row.position == *position);
        let row = rows
            .next()
            .ok_or_else(|| bad("actual header loses an original full class occurrence"))?;
        if rows.next().is_some() {
            return Err(bad(
                "actual header repeats an original full class occurrence",
            ));
        }
        if position.base != StorageBase::Result {
            bind(&mut mapping, row.binder, *term, b)?;
        }
    }
    for (position, term) in &directory.bindings {
        b.spend(
            actual
                .bindings
                .len()
                .saturating_add(actual.static_bindings.len()),
        )?;
        if actual.static_bindings.contains(position) {
            if position.base != StorageBase::Result {
                return Err(bad(
                    "original ordinary input cannot be replaced by a constant Static occurrence",
                ));
            }
            continue;
        }
        let mut rows = actual
            .bindings
            .iter()
            .filter(|row| row.position == *position);
        let row = rows
            .next()
            .ok_or_else(|| bad("actual header loses an original borrow occurrence"))?;
        if rows.next().is_some() {
            return Err(bad("actual header repeats an original borrow occurrence"));
        }
        if position.base != StorageBase::Result {
            bind(&mut mapping, row.binder, *term, b)?;
        }
    }
    if let Some(instance) = &function.dyn_instance {
        scopes::function(module, function, instance, b)?;
        for row in &instance.header_embedding {
            b.spend(mapping.len())?;
            let mut occurrences = mapping
                .iter()
                .filter(|(from, _)| *from == row.from)
                .peekable();
            if occurrences.peek().is_none() {
                return Err(bad(
                    "actual header embedding has no complete original input occurrence",
                ));
            }
            for (_, term) in occurrences {
                if own_domain && row.to != term.image {
                    return Err(bad(
                        "actual header embedding changes an independently matched occurrence image",
                    ));
                }
            }
        }
    }
    for (position, term) in &directory.classes {
        if position.base != StorageBase::Result {
            continue;
        }
        let row = actual
            .classes
            .iter()
            .find(|row| row.position == *position)
            .ok_or_else(|| bad("actual result class occurrence is absent"))?;
        output(&mapping, row.binder, *term, b)?;
    }
    for (position, term) in &directory.bindings {
        if position.base != StorageBase::Result || actual.static_bindings.contains(position) {
            continue;
        }
        let row = actual
            .bindings
            .iter()
            .find(|row| row.position == *position)
            .ok_or_else(|| bad("actual result borrow occurrence is absent"))?;
        output(&mapping, row.binder, *term, b)?;
    }
    if actual
        .classes
        .iter()
        .map(|row| &row.position)
        .ne(directory.classes.iter().map(|(position, _)| position))
        || actual.bindings.iter().map(|row| &row.position).ne(directory
            .bindings
            .iter()
            .filter(|(position, _)| !actual.static_bindings.contains(position))
            .map(|(position, _)| position))
        || actual.static_bindings.iter().ne(directory
            .bindings
            .iter()
            .filter(|(position, _)| actual.static_bindings.contains(position))
            .map(|(position, _)| position))
        || actual.classes.len() != directory.classes.len()
        || actual
            .bindings
            .len()
            .saturating_add(actual.static_bindings.len())
            != directory.bindings.len()
        || actual
            .static_bindings
            .iter()
            .any(|p| !directory.bindings.iter().any(|(position, _)| position == p))
    {
        return Err(bad(
            "actual header has extra, missing or displaced original occurrences",
        ));
    }
    // The actual header (including its Static positions) is preserved in the
    // ContractRequest separately. The original variable upper stays variable;
    // a constant result may satisfy it only after the actual CFG proves Static.
    expected(module, parameters, result, b)
}
fn bind(
    mapping: &mut Vec<(StorageBinder, terms::LifeTerm)>,
    binder: StorageBinder,
    term: terms::LifeTerm,
    b: &mut Budget,
) -> Check<()> {
    b.spend(mapping.len().saturating_add(1))?;
    // The binder is an image channel, not an origin. Distinct actual P2
    // occurrences may share H0 while retaining their two intrinsic terms.
    for (_, known) in mapping.iter().filter(|(from, _)| *from == binder) {
        if known.image != term.image {
            return Err(bad(
                "one actual header binder has inconsistent occurrence images",
            ));
        }
    }
    if !mapping.contains(&(binder, term)) {
        mapping.push((binder, term));
    }
    Ok(())
}
fn output(
    mapping: &[(StorageBinder, terms::LifeTerm)],
    binder: StorageBinder,
    term: terms::LifeTerm,
    b: &mut Budget,
) -> Check<()> {
    b.spend(mapping.len())?;
    if !mapping.contains(&(binder, term)) {
        return Err(bad(
            "actual result selects a different intrinsic input origin despite equal lexical images",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (TypedModule, terms::Value, terms::Value) {
        let (mut module, _) = crate::tests::baseline();
        let span = module.functions[0].span;
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let declaration = LifeDeclarationId {
            module: module.module_id,
            declaration: zil_base::NodeId(80),
            compiler: false,
        };
        let life = StorageLifeShape {
            binder_count: 2,
            slots: (0..2)
                .map(|class| StorageLifeSlot {
                    path: vec![PlaceProjection::Field(class)],
                    mode: StoredBorrowMode::SharedAddress,
                    binder: class,
                })
                .collect(),
            retained: vec![],
            nominal_arguments: (0..2)
                .map(|class| NominalLifeArgument {
                    path: vec![],
                    declaration,
                    argument: class,
                    binder: class,
                })
                .collect(),
        };
        let pair = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "header.Pair".into(),
                fields: vec![
                    ("left".into(), address.clone()),
                    ("right".into(), address.clone()),
                ],
            }),
            life: life.clone(),
        };
        module.life_declarations.push(LifeNominalDeclaration {
            id: declaration,
            identity: LifeDeclarationIdentity {
                package: "header".into(),
                module_path: vec![],
                declaration_path: vec!["Pair".into()],
                generation_path: vec![],
            },
            kind: LifeNominalKind::Record,
            parameters: vec![LifeParameterKind::Lifetime; 2],
        });
        module.life_instances.push(LifeNominalInstance {
            declaration,
            ty: pair.clone(),
            arguments: vec![
                LifeNominalActual::Lifetime(0),
                LifeNominalActual::Lifetime(1),
            ],
        });
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let owner = terms::Owner::Function(FunctionId(0));
        let receiver = terms::Value::anchored(
            &DynTypeValue {
                ty: pair,
                life,
                classes: vec![StorageBinder::Fixed(0); 2],
            },
            owner,
            terms::Root::Receiver,
            &mut b,
        )
        .unwrap();
        let head = terms::LifeTerm {
            origin: terms::Origin {
                owner,
                root: terms::Root::Lexical(StorageBinder::Invocation(0)),
                class: 0,
            },
            image: StorageBinder::Invocation(0),
        };
        let parameter =
            constructors::address(&receiver, head, StoredBorrowMode::SharedAddress, &mut b)
                .unwrap();
        let scalar = terms::Value {
            raw: DynTypeValue {
                ty: Type::I32,
                life: StorageLifeShape {
                    binder_count: 0,
                    slots: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
                classes: vec![],
            },
            terms: vec![],
        };
        let result = constructors::address(
            &scalar,
            receiver.terms[0],
            StoredBorrowMode::SharedAddress,
            &mut b,
        )
        .unwrap();
        let f = &mut module.functions[0];
        f.parameters = vec![LocalId(0)];
        f.locals = vec![LocalDecl {
            id: LocalId(0),
            name: "pair".into(),
            ty: parameter.raw.ty.clone(),
            mutable: false,
            span,
        }];
        f.return_type = address.clone();
        f.values = vec![
            parameter.raw.ty.clone(),
            Type::Address {
                mutable: false,
                inner: Box::new(address.clone()),
            },
            address,
        ];
        f.blocks[0].instructions = vec![
            Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::Load(LocalId(0)),
                span,
            },
            Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::AddressField {
                    address: ValueId(0),
                    index: 0,
                },
                span,
            },
            Instruction {
                result: Some(ValueId(2)),
                kind: InstructionKind::ReadAddress {
                    address: ValueId(1),
                },
                span,
            },
        ];
        f.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
        f.storage_life = Some(StorageLifeContract {
            fixed_binder_count: 2,
            invocation_binder_count: 1,
            bindings: vec![
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(0),
                },
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Result,
                        path: vec![],
                    },
                    binder: StorageBinder::Fixed(0),
                },
            ],
            static_bindings: vec![],
            classes: (0..2)
                .map(|class| SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base: StorageBase::Parameter(0),
                        path: vec![LifeTypeProjection::Pointee],
                        class,
                    },
                    binder: StorageBinder::Fixed(class),
                })
                .collect(),
        });
        (module, parameter, result)
    }
    fn raw_and_wire(
        module: &TypedModule,
        parameter: &terms::Value,
        result: &terms::Value,
    ) -> Check<requests::Header> {
        let limits = Limits::default();
        let raw = function(
            module,
            &module.functions[0],
            std::slice::from_ref(parameter),
            result,
            &mut Budget::new(&limits),
        );
        let plan = CandidatePlan::empty_for(module);
        let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = function(
            &decoded,
            &decoded.functions[0],
            std::slice::from_ref(parameter),
            result,
            &mut Budget::new(&limits),
        );
        assert_eq!(
            raw.as_ref().map_err(|e| (&e.code, &e.message)),
            wire.as_ref().map_err(|e| (&e.code, &e.message))
        );
        raw
    }
    #[test]
    fn equal_lexical_images_do_not_replace_exact_header_input_origins() {
        let (module, parameter, result) = fixture();
        assert!(raw_and_wire(&module, &parameter, &result).is_ok());
        let mut wrong = module.clone();
        wrong.functions[0].storage_life.as_mut().unwrap().bindings[1].binder =
            StorageBinder::Fixed(1);
        assert!(raw_and_wire(&wrong, &parameter, &result).is_err());
        let mut collapsed = module.clone();
        collapsed.functions[0]
            .storage_life
            .as_mut()
            .unwrap()
            .classes[1]
            .binder = StorageBinder::Fixed(0);
        collapsed.functions[0]
            .storage_life
            .as_mut()
            .unwrap()
            .fixed_binder_count = 1;
        assert!(raw_and_wire(&collapsed, &parameter, &result).is_ok());
        // This does not change either nominal occurrence in the complete P2.
        assert_eq!(parameter.life.nominal_arguments.len(), 2);
        let mut moved = module.clone();
        moved.functions[0]
            .storage_life
            .as_mut()
            .unwrap()
            .classes
            .swap(0, 1);
        assert!(raw_and_wire(&moved, &parameter, &result).is_err());
    }
    #[test]
    fn constant_result_keeps_its_pending_occurrence_and_variable_upper_separate() {
        let (mut module, parameter, mut result) = fixture();
        let slice = Type::Slice {
            element: Box::new(Type::U8),
        };
        result.raw.ty = Type::Address {
            mutable: false,
            inner: Box::new(slice),
        };
        let f = &mut module.functions[0];
        f.return_type = result.raw.ty.clone();
        let span = f.span;
        f.values = vec![result.raw.ty.clone()];
        f.blocks[0].instructions = vec![Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::StaticUtf8 {
                bytes: b"constant".to_vec(),
            },
            span,
        }];
        f.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
        let life = f.storage_life.as_mut().unwrap();
        let output = life.bindings.pop().unwrap();
        life.static_bindings.push(output.position.clone());
        let expected = raw_and_wire(&module, &parameter, &result).unwrap();
        assert_eq!(expected.bindings.last().unwrap().position, output.position);
        assert!(expected.static_bindings.is_empty());
        assert_eq!(
            module.functions[0]
                .storage_life
                .as_ref()
                .unwrap()
                .static_bindings,
            [output.position]
        );
        // The same transport marker cannot narrow a universal input to Static.
        let mut narrow = module;
        let life = narrow.functions[0].storage_life.as_mut().unwrap();
        let input = life.bindings.remove(0);
        life.static_bindings.insert(0, input.position);
        assert!(raw_and_wire(&narrow, &parameter, &result).is_err());
    }
}
