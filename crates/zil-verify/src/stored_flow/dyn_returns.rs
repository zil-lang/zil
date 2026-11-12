//! Function result uppers are compared with actual SSA/store facts. They never
//! seed a result permission or turn an absent source row into an empty one.
use super::*;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-SOURCE", None, message)
}
pub(super) fn complete(
    module: &TypedModule,
    ty: &Type,
    payload: &Payload,
    b: &mut Budget,
) -> Check<bool> {
    for (path, _) in dyn_ports::shape(module, ty, b)? {
        if dyn_ports::find(&payload.dyn_ports, &path, b)?
            .is_none_or(|at| !payload.dyn_ports.0[at].1.known)
        {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    payload: &Payload,
    summary: &mut Summary,
    loans: &[Loan],
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    for (path, _) in dyn_ports::shape(module, &f.return_type, b)? {
        let Some(at) = dyn_ports::find(&payload.dyn_ports, &path, b)? else {
            summary.complete = false;
            continue;
        };
        let original = &payload.dyn_ports.0[at].1;
        permission_work(&original.permission, b)?;
        source_work(&original.parent_sources, b)?;
        let mut row = original.clone();
        summary.complete &= row.known;
        // Loan indices are function-local. Only real source positions cross a
        // summary edge; instantiate rebuilds every actual caller parent set.
        add_sources(
            &mut row.parent_sources,
            &parents(&row.permission, loans, b)?,
            b,
        )?;
        row.permission.loans.clear();
        dyn_ports::insert(&mut summary.dyn_ports, &path, &row, b)?;
    }
    Ok(())
}
fn parameter<'a>(f: &'a TypedFunction, position: u32) -> Check<&'a Type> {
    f.parameters
        .get(position as usize)
        .and_then(|id| f.locals.get(id.0 as usize))
        .map(|l| &l.ty)
        .ok_or_else(|| bad("source upper parameter has no actual full type"))
}
fn path_places(path: &[ContractProjection], b: &mut Budget) -> Check<Path> {
    b.spend(path.len())?;
    path.iter()
        .map(|step| match step {
            ContractProjection::Place(p) => Ok(*p),
            _ => Err(bad(
                "physical source upper contains an opaque or capture projection",
            )),
        })
        .collect()
}
/// Return the actual full dependency leaf and its canonical symbolic source.
/// Address of holder storage and the pointer read from a holder are distinct.
fn input(
    module: &TypedModule,
    f: &TypedFunction,
    position: &StoragePosition,
    b: &mut Budget,
) -> Check<(Type, Source)> {
    let StorageBase::Parameter(index) = position.base else {
        return Err(bad(
            "ordinary result upper must select a real input parameter",
        ));
    };
    b.spend(position.path.len().saturating_add(1))?;
    let root = parameter(f, index)?;
    if let Some((ContractProjection::RetainedPort(port), carrier)) = position.path.split_last() {
        let endpoint = dyn_ports::carrier_type(module, root, carrier, b)?;
        let (Type::Dyn { interface } | Type::OwnedDyn { interface }) = endpoint else {
            return Err(bad("source upper opaque selector has a non-Dyn endpoint"));
        };
        let port_type = &crate::dynamic_ports::interface(module, *interface, b)?
            .retained_life
            .ports
            .get(*port as usize)
            .ok_or_else(|| bad("source upper Dyn port is absent"))?
            .leaf;
        dynamic_metadata::ty(port_type, b)?;
        return Ok((
            port_type.clone(),
            Source {
                base: HoldRoot::Parameter(index),
                retained_port: Some(PortOrigin::Dyn {
                    carrier: carrier.to_vec(),
                    port: *port,
                }),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_element: None,
            },
        ));
    }
    let path = path_places(&position.path, b)?;
    let storage = if let Type::Address { inner, .. } = root {
        inner.as_ref()
    } else {
        root
    };
    let endpoint = crate::dynamic_ports::physical_path(storage, &path, b)?;
    dynamic_metadata::ty(endpoint, b)?;
    let source = Source {
        base: HoldRoot::Parameter(index),
        retained_port: None,
        leaf: path,
        inline: Vec::new(),
        heap_element: None,
    };
    if matches!(endpoint, Type::Address { .. } | Type::Desc(_)) {
        Ok((endpoint.clone(), source))
    } else if position.path.is_empty() && matches!(root, Type::Address { .. }) {
        dynamic_metadata::ty(root, b)?;
        Ok((root.clone(), source))
    } else {
        Err(bad(
            "result upper does not select an actual borrowed input leaf",
        ))
    }
}
pub(super) fn validate(
    module: &TypedModule,
    f: &TypedFunction,
    rows: &dyn_ports::Rows,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    let expected = dyn_ports::shape(module, &f.return_type, b)?;
    let claim = f.retained_return_sources.as_ref();
    if expected.is_empty() {
        // Closure capture_sources has a separate independent proof. A missing
        // new family contract must not reject an older borrowed closure.
        if claim.is_some_and(|claim| !claim.is_empty()) {
            return Err(bad("source proposal names no actual result Dyn port"));
        }
        b.pending_dyn_returns.remove(&f.id);
        return Ok(());
    }
    let claim = claim.ok_or_else(|| bad("retained result source upper is Unknown"))?;
    b.spend(claim.len().saturating_add(expected.len()))?;
    let mut seen = Vec::new();
    for row in claim {
        b.spend(
            row.position
                .path
                .len()
                .saturating_mul(seen.len().saturating_add(1))
                .saturating_add(1),
        )?;
        if row.position.base != StorageBase::Result
            || !expected.iter().any(|(path, _)| *path == row.position.path)
            || seen.contains(&row.position.path)
        {
            return Err(bad(
                "result source upper has a duplicate or non-result-port destination",
            ));
        }
        seen.push(row.position.path.clone());
    }
    for (path, _) in expected {
        let Some(at) = dyn_ports::find(rows, &path, b)? else {
            return Err(bad("actual Return is missing a result dependency row"));
        };
        let actual = &rows.0[at].1;
        if !actual.known {
            return Err(bad("actual Return result port remains Unknown"));
        }
        let declared = claim.iter().find(|row| row.position.path == path);
        if !claim.is_empty() && declared.is_none() {
            return Err(bad("source upper omits a real result port"));
        }
        let Some((ContractProjection::RetainedPort(port), carrier)) = path.split_last() else {
            return Err(bad("actual result dependency has no typed terminal port"));
        };
        let (Type::Dyn { interface } | Type::OwnedDyn { interface }) =
            dyn_ports::carrier_type(module, &f.return_type, carrier, b)?
        else {
            return Err(bad("result port has a non-Dyn actual endpoint"));
        };
        let output = &crate::dynamic_ports::interface(module, *interface, b)?
            .retained_life
            .ports[*port as usize];
        let mut upper = Vec::new();
        if let Some(declared) = declared {
            for source in &declared.sources {
                let (leaf, symbolic) = input(module, f, source, b)?;
                dynamic_metadata::ty(&output.leaf, b)?;
                if leaf != output.leaf {
                    return Err(bad(
                        "source upper full borrowed leaf differs from result port",
                    ));
                }
                source_work(std::slice::from_ref(&symbolic), b)?;
                upper.push(symbolic);
            }
        }
        for source in &actual.permission.sources {
            source_work(std::slice::from_ref(source), b)?;
            if source.base == HoldRoot::Static
                && source.retained_port.is_none()
                && source.leaf.is_empty()
                && source.inline.is_empty()
                && source.heap_element.is_none()
            {
                continue;
            }
            if !matches!(source.base, HoldRoot::Parameter(_)) {
                return Err(bad("returned Dyn retains activation or capture storage"));
            }
            b.spend(upper.len().saturating_mul(width(source).saturating_add(1)))?;
            if !upper.contains(source) {
                return Err(bad(
                    "actual Return target exceeds its declared source upper",
                ));
            }
        }
    }
    b.pending_dyn_returns.remove(&f.id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};
    fn boundary() -> (TypedModule, TypedFunction, dyn_ports::Rows) {
        let (mut module, _) = crate::tests::baseline();
        let interface = DynInterfaceId {
            module: ModuleId(0),
            index: 0,
        };
        module.dyn_interfaces.push(DynInterface {
            context: Some(Type::Unit),
            owning_cleanup: None,
            id: interface,
            application: DynTraitApplication {
                declaration: LifeDeclarationId {
                    module: ModuleId(0),
                    declaration: NodeId(9),
                    compiler: false,
                },
                actuals: vec![],
            },
            inherited: vec![],
            associated_types: vec![],
            associated_constants: vec![],
            retained_life: RetainedLifeShape {
                binder_count: 1,
                ports: vec![RetainedBorrowPort {
                    leaf: Type::Address {
                        mutable: false,
                        inner: Box::new(Type::I32),
                    },
                    mode: StoredBorrowMode::SharedAddress,
                    binder: 0,
                }],
                retained: vec![],
                nominal_arguments: vec![],
            },
            slots: vec![],
        });
        let mut f = module.functions[0].clone();
        f.parameters = vec![LocalId(0), LocalId(1)];
        f.locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "first".into(),
                ty: Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
                mutable: false,
                span: f.span,
            },
            LocalDecl {
                id: LocalId(1),
                name: "same_type_second".into(),
                ty: Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
                mutable: false,
                span: f.span,
            },
        ];
        f.return_type = Type::OwnedDyn { interface };
        f.retained_return_sources = Some(vec![StorageSourceBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![ContractProjection::RetainedPort(0)],
            },
            sources: vec![StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![],
            }],
        }]);
        let source = Source {
            retained_port: None,
            base: HoldRoot::Parameter(0),
            leaf: vec![],
            inline: vec![],
            heap_element: None,
        };
        let rows = dyn_ports::Rows(vec![(
            vec![ContractProjection::RetainedPort(0)],
            dyn_ports::Port {
                parent_sources: Vec::new(),
                permission: Permission {
                    sources: vec![source],
                    loans: vec![],
                },
                known: true,
            },
        )]);
        (module, f, rows)
    }
    fn checked(module: &TypedModule, f: &TypedFunction, rows: &dyn_ports::Rows) -> bool {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        b.dyn_contexts = true;
        validate(module, f, rows, &mut b).is_ok()
    }
    #[test]
    fn dyn_result_upper_keeps_actual_same_type_input_and_rejects_local_escape() {
        let (module, f, rows) = boundary();
        assert!(checked(&module, &f, &rows));
        let mut forged = f.clone();
        forged.retained_return_sources.as_mut().unwrap()[0].sources[0].base =
            StorageBase::Parameter(1);
        assert!(!checked(&module, &forged, &rows));
        let mut forged = rows.clone();
        forged.0[0].1.permission.sources[0].base = HoldRoot::Local(LocalId(0));
        assert!(!checked(&module, &f, &forged));
    }
    #[test]
    fn dyn_result_unknown_missing_and_empty_are_distinct() {
        let (module, f, rows) = boundary();
        let mut claim = f.clone();
        claim.retained_return_sources = None;
        assert!(!checked(&module, &claim, &rows));
        claim.retained_return_sources = Some(vec![]);
        assert!(!checked(&module, &claim, &rows));
        let mut actual = rows.clone();
        actual.0[0].1.permission = Permission::default();
        assert!(checked(&module, &claim, &actual));
        actual.0[0].1.known = false;
        assert!(!checked(&module, &claim, &actual));
        assert!(!checked(&module, &claim, &dyn_ports::Rows::default()));
        claim.retained_return_sources = Some(vec![StorageSourceBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![ContractProjection::RetainedPort(0)],
            },
            sources: vec![],
        }]);
        assert!(!checked(&module, &claim, &rows));
        actual.0[0].1.known = true;
        assert!(checked(&module, &claim, &actual));
    }
    #[test]
    fn dyn_port_namespace_and_complete_carrier_are_never_task_or_head_roots() {
        let path = vec![
            ContractProjection::Place(PlaceProjection::Field(1)),
            ContractProjection::Capture(2),
        ];
        let dyn_source = Source {
            retained_port: Some(PortOrigin::Dyn {
                carrier: path.clone(),
                port: 0,
            }),
            base: HoldRoot::Parameter(0),
            leaf: vec![],
            inline: vec![],
            heap_element: None,
        };
        let task_source = Source {
            retained_port: Some(PortOrigin::Task(0)),
            ..dyn_source.clone()
        };
        let head = Source {
            retained_port: None,
            ..dyn_source.clone()
        };
        let mut b = Budget::new(&Limits::default());
        assert!(!overlap(&dyn_source, &task_source, &mut b).unwrap());
        assert!(!overlap(&dyn_source, &head, &mut b).unwrap());
        let mut other = dyn_source.clone();
        other.retained_port = Some(PortOrigin::Dyn {
            carrier: vec![ContractProjection::Place(PlaceProjection::Field(2))],
            port: 0,
        });
        assert!(!overlap(&dyn_source, &other, &mut b).unwrap());
        let mut requirements = vec![];
        apart(&dyn_source, &task_source, &mut requirements, &mut b).unwrap();
        assert_eq!(
            requirements.len(),
            1,
            "different abstract namespaces still require actual caller alias checking"
        );
    }
}

/// Resolve a Dyn data class through the real enclosing carrier metadata. This
/// never reuses the Task selector interpretation or a coincident numeric class.
pub(super) fn source_relation(
    module: &TypedModule,
    f: &TypedFunction,
    parameter: u32,
    carrier: &[ContractProjection],
    port: u32,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let root = parameter_type(f, parameter)?;
    let endpoint = dyn_ports::carrier_type(module, root, carrier, b)?;
    let (Type::Dyn { interface } | Type::OwnedDyn { interface }) = endpoint else {
        return Err(bad("Dyn class source has a non-Dyn endpoint"));
    };
    let class = crate::dynamic_ports::interface(module, *interface, b)?
        .retained_life
        .ports
        .get(port as usize)
        .ok_or_else(|| bad("Dyn class source port is absent"))?
        .binder;
    let (path, class) = class_key(module, root, carrier, class, 0, b)?;
    let life = f
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("Dyn data source has no actual signature class relation"))?;
    b.spend(
        life.classes
            .len()
            .saturating_mul(path.len().saturating_add(1)),
    )?;
    life.classes
        .iter()
        .find(|row| {
            row.position.base == StorageBase::Parameter(parameter)
                && row.position.path == path
                && row.position.class == class
        })
        .map(|row| row.binder)
        .ok_or_else(|| bad("Dyn data source omits its actual enclosing class relation"))
}
fn parameter_type(f: &TypedFunction, index: u32) -> Check<&Type> {
    parameter(f, index)
}
fn class_key(
    module: &TypedModule,
    root: &Type,
    carrier: &[ContractProjection],
    class: u32,
    depth: usize,
    b: &mut Budget,
) -> Check<(Vec<LifeTypeProjection>, u32)> {
    b.spend(carrier.len().saturating_add(1))?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("Dyn source class path exceeds selected depth"));
    }
    let (root, mut prefix) = if let Type::Address { inner, .. } = root {
        (inner.as_ref(), vec![LifeTypeProjection::Pointee])
    } else {
        (root, vec![])
    };
    if let Type::Closure(id) = root {
        let Some((ContractProjection::Capture(index), tail)) = carrier.split_first() else {
            return Err(bad(
                "closure Dyn source class omits the actual capture path",
            ));
        };
        let definition = crate::closures::definition(module, *id, None)?;
        let child = definition
            .captures
            .get(*index as usize)
            .ok_or_else(|| bad("Dyn class source capture is absent"))?;
        let (path, child_class) = class_key(module, child, tail, class, depth + 1, b)?;
        let life = definition
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("captured Dyn class map is Unknown"))?;
        b.spend(
            life.classes
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let binding = life
            .classes
            .iter()
            .find(|row| {
                row.position.base == StorageBase::Capture(*index)
                    && row.position.path == path
                    && row.position.class == child_class
            })
            .ok_or_else(|| bad("captured Dyn has no complete environment class map"))?;
        let StorageBinder::Fixed(class) = binding.binder else {
            return Err(bad(
                "captured Dyn persistent class was narrowed to an invocation",
            ));
        };
        return Ok((prefix, class));
    }
    let path = carrier
        .iter()
        .map(|step| match step {
            ContractProjection::Place(p) => Ok(LifeTypeProjection::Place(*p)),
            _ => Err(bad(
                "Dyn class carrier path has no complete typed metadata route",
            )),
        })
        .collect::<Check<Vec<_>>>()?;
    if let Type::BorrowedStorage { life, .. } = root {
        b.spend(
            life.retained
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let row = life
            .retained
            .iter()
            .find(|row| row.path == path && row.class == class)
            .ok_or_else(|| bad("Dyn carrier class omits its actual enclosing fixed map"))?;
        return Ok((prefix, row.binder));
    }
    prefix.extend(path);
    Ok((prefix, class))
}

pub(super) fn after(
    module: &TypedModule,
    f: &TypedFunction,
    state: &Frame,
    summary: &mut Summary,
    loans: &[Loan],
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    for (position, local) in f.parameters.iter().enumerate() {
        let ty = &f.locals[local.0 as usize].ty;
        if !matches!(ty, Type::Address { .. }) {
            continue;
        }
        let expected = dyn_ports::shape(module, ty, b)?;
        if expected.is_empty() {
            continue;
        }
        let actual = state
            .external
            .get(position)
            .ok_or_else(|| bad("helper actual external Dyn carrier is absent"))?;
        let mut rows = dyn_ports::Rows::default();
        for (path, _) in expected {
            let Some(at) = dyn_ports::find(&actual.dyn_ports, &path, b)? else {
                summary.complete = false;
                continue;
            };
            let original = &actual.dyn_ports.0[at].1;
            permission_work(&original.permission, b)?;
            source_work(&original.parent_sources, b)?;
            let mut row = original.clone();
            summary.complete &= row.known;
            add_sources(
                &mut row.parent_sources,
                &parents(&row.permission, loans, b)?,
                b,
            )?;
            row.permission.loans.clear();
            for source in &row.permission.sources {
                if !matches!(source.base, HoldRoot::Parameter(_) | HoldRoot::Static) {
                    return Err(bad(
                        "helper after Dyn port retains activation-local storage",
                    ));
                }
            }
            dyn_ports::insert(&mut rows, &path, &row, b)?;
        }
        b.spend(summary.dyn_after.len())?;
        if let Some((_, old)) = summary
            .dyn_after
            .iter_mut()
            .find(|(p, _)| *p == position as u32)
        {
            for (path, row) in rows.0 {
                dyn_ports::insert(old, &path, &row, b)?;
            }
        } else {
            summary.dyn_after.push((position as u32, rows));
        }
    }
    Ok(())
}

/// Preserve actual non-target parents across an ordinary call boundary. The
/// source graph of each ancestor was reconstructed by this kernel's SSA pass.
pub(super) fn parents(
    permission: &Permission,
    loans: &[Loan],
    b: &mut Budget,
) -> Check<Vec<Source>> {
    let mut result = Vec::new();
    for id in ancestry(&permission.loans, loans, b)? {
        let actual = loans
            .get(id)
            .ok_or_else(|| bad("returned parent lies outside the actual loan arena"))?;
        add_sources(&mut result, &actual.sources, b)?;
    }
    Ok(result)
}
