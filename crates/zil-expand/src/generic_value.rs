//! Ordinary inline values and synchronous cleanup are distinct obligations.
//! These definition-time checks never grant Store layout, Copy, or an empty
//! retained context. Concrete instances still enter all ordinary core checks.
use super::*;

pub(crate) fn bound(ty: &TypeRef) -> Option<Bound> {
    match plain(ty) {
        Some("Value") => Some(Bound::Value),
        Some("SyncDrop") => Some(Bound::SyncDrop),
        _ => None,
    }
}
pub(crate) fn is_bound(ty: &TypeRef) -> bool {
    bound(ty).is_some()
}
pub(crate) fn name(bound: &Bound) -> Option<&'static str> {
    match bound {
        Bound::Value => Some("Value"),
        Bound::SyncDrop => Some("SyncDrop"),
        _ => None,
    }
}

impl Index<'_> {
    pub(crate) fn value_bound(
        &self,
        ty: &TypeRef,
        capability: &Bound,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        self.value_shape(
            ty,
            capability,
            available,
            variables,
            budget,
            &mut BTreeSet::new(),
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn value_shape(
        &self,
        ty: &TypeRef,
        capability: &Bound,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
        active: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<()> {
        budget.node(ty.span, depth)?;
        budget.ty(ty)?;
        if matches!(&grouped(ty).kind,TypeKind::Named{path,..} if path.len()==2 && variables.contains(&path[0]))
        {
            let projections = self.projection_names(available, budget, ty.span)?;
            budget.substituted_type(ty, &projections)?;
            let mut projected = ty.clone();
            Substitution {
                substitutions: &projections,
            }
            .ty(&mut projected);
            if associated::parts(&projected).is_none() {
                return Err(error(
                    ty.span,
                    "Value projection requires an explicit unambiguous trait premise",
                ));
            }
            return self.value_shape(
                &projected,
                capability,
                available,
                variables,
                budget,
                active,
                depth + 1,
            );
        }
        let sync = matches!(capability, Bound::SyncDrop);
        let accepts = |b: &Bound| {
            b == capability || matches!(b, Bound::Copy | Bound::StoreElement)
        };
        let symbolic = plain(ty).filter(|name| variables.contains(*name));
        let projected = associated::parts(ty).is_some();
        if symbolic.is_some() || projected {
            let key;
            let subject = if let Some(name) = symbolic {
                name
            } else {
                key = subject_key(ty, budget)?;
                &key
            };
            if available
                .get(subject)
                .is_some_and(|bounds| bounds.iter().any(accepts))
            {
                return Ok(());
            }
            return Err(error(
                ty.span,
                if sync {
                    "synchronous cleanup requires a declared SyncDrop bound"
                } else {
                    "ordinary inline fields require a declared Value bound"
                },
            ));
        }
        match &grouped(ty).kind {
            // Dropping an address does not drop its owner. Its full source and
            // retained class remain in the original TypeRef and core contract.
            TypeKind::Reference { .. } => Ok(()),
            TypeKind::Function { .. } | TypeKind::ForeignFunction { .. } => {
                trait_instances::known_type(ty, variables, &self.nominals, budget, depth + 1)
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
                for field in fields {
                    self.value_shape(
                        field,
                        capability,
                        available,
                        variables,
                        budget,
                        active,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            TypeKind::Error { error, result } => {
                self.value_shape(
                    error,
                    capability,
                    available,
                    variables,
                    budget,
                    active,
                    depth + 1,
                )?;
                self.value_shape(
                    result,
                    capability,
                    available,
                    variables,
                    budget,
                    active,
                    depth + 1,
                )
            }
            TypeKind::Array {
                element,
                length: Some(_),
            } => self.value_shape(
                element,
                capability,
                available,
                variables,
                budget,
                active,
                depth + 1,
            ),
            TypeKind::Named { path, args } => {
                let name = path.join(".");
                if args.is_empty()
                    && matches!(
                        name.as_str(),
                        "void"
                            | "bool"
                            | "u8"
                            | "u16"
                            | "u32"
                            | "i8"
                            | "i16"
                            | "isize"
                            | "i32"
                            | "i64"
                            | "u64"
                            | "usize"
                            | "f32"
                            | "f64"
                            | "c_handle"
                            | "ForeignHandle"
                            | "c_wstr"
                            | "CWideString"
                    )
                {
                    return Ok(());
                }
                if matches!(name.as_str(), "Task" | "AsyncRegistration") {
                    if sync {
                        return Err(error(
                            ty.span,
                            "Task and AsyncRegistration require actual closed state before synchronous cleanup",
                        ));
                    }
                    // This is only a complete value obligation. State, port,
                    // closure, and cleanup obligations remain with their actual
                    // producer and independent core consumers.
                    if (name == "Task" && !(1..=2).contains(&args.len()))
                        || (name == "AsyncRegistration" && args.len() != 1)
                    {
                        return Err(error(
                            ty.span,
                            "Value requires the complete Task/registration application",
                        ));
                    }
                    for argument in args {
                        let GenericArg::Type(actual) = argument else {
                            return Err(error(
                                ty.span,
                                "Task Value needs actual type and context arguments",
                            ));
                        };
                        self.value_shape(
                            actual,
                            &Bound::Value,
                            available,
                            variables,
                            budget,
                            active,
                            depth + 1,
                        )?;
                    }
                    return Ok(());
                }
                if name == "Store" && sync {
                    return self.managed_value(
                        ty,
                        capability,
                        available,
                        variables,
                        budget,
                        active,
                        depth + 1,
                    );
                }
                if name == "Managed" {
                    let [GenericArg::Type(resource)] = args.as_slice() else {
                        return Err(error(ty.span, "Managed Value requires its actual resource"));
                    };
                    return self.managed_value(
                        resource,
                        capability,
                        available,
                        variables,
                        budget,
                        active,
                        depth + 1,
                    );
                }
                let key = type_key(ty, budget)?;
                budget.identity_work(
                    key.len()
                        .saturating_add(1)
                        .saturating_mul(active.len().saturating_add(2)),
                    ty.span,
                )?;
                if !active.insert(key.clone()) {
                    return Err(error(ty.span, "recursive Value has infinite inline layout"));
                }
                let result = (|| {
                    let item = self
                        .nominals
                        .get(&name)
                        .copied()
                        .or_else(|| self.instantiated_copy_shapes.get(&name))
                        .ok_or_else(|| {
                            error(
                                ty.span,
                                format!("Value requires a complete ordinary type: {name}"),
                            )
                        })?;
                    match &item.kind {
                        ItemKind::Aggregate(aggregate)
                            if matches!(
                                aggregate.kind,
                                AggregateKind::Package | AggregateKind::Struct
                            ) =>
                        {
                            if aggregate.generics.len() != args.len() {
                                return Err(error(
                                    ty.span,
                                    "Value requires every actual type argument",
                                ));
                            }
                            let mut substitutions = BTreeMap::new();
                            for (parameter, arg) in aggregate.generics.iter().zip(args) {
                                match (&parameter.kind, arg) {
                                    (GenericParamKind::Type(_), GenericArg::Type(actual)) => {
                                        budget.ty(actual)?;
                                        substitutions
                                            .insert(parameter.name.clone(), actual.clone());
                                    }
                                    (GenericParamKind::Lifetime, GenericArg::Lifetime(life)) => {
                                        budget.text(life, ty.span)?
                                    }
                                    _ => {
                                        return Err(unsupported(
                                            ty.span,
                                            "Value requires matching type/life arguments",
                                        ));
                                    }
                                }
                            }
                            for field in &aggregate.fields {
                                budget.substituted_type(&field.ty, &substitutions)?;
                                let mut actual = field.ty.clone();
                                Substitution {
                                    substitutions: &substitutions,
                                }
                                .ty(&mut actual);
                                if field.manages {
                                    self.managed_value(
                                        &actual,
                                        capability,
                                        available,
                                        variables,
                                        budget,
                                        active,
                                        depth + 1,
                                    )?;
                                } else {
                                    self.value_shape(
                                        &actual,
                                        capability,
                                        available,
                                        variables,
                                        budget,
                                        active,
                                        depth + 1,
                                    )?;
                                }
                            }
                            Ok(())
                        }
                        ItemKind::Enum(declaration) => {
                            if declaration.generics.len() != args.len() {
                                return Err(error(
                                    ty.span,
                                    "Value enum requires every actual argument",
                                ));
                            }
                            let mut substitutions = BTreeMap::new();
                            for (parameter, arg) in declaration.generics.iter().zip(args) {
                                match (&parameter.kind, arg) {
                                    (GenericParamKind::Type(_), GenericArg::Type(actual)) => {
                                        budget.ty(actual)?;
                                        substitutions
                                            .insert(parameter.name.clone(), actual.clone());
                                    }
                                    (GenericParamKind::Lifetime, GenericArg::Lifetime(life)) => {
                                        budget.text(life, ty.span)?
                                    }
                                    _ => {
                                        return Err(unsupported(
                                            ty.span,
                                            "Value enum requires matching type/life arguments",
                                        ));
                                    }
                                }
                            }
                            for variant in &declaration.variants {
                                let count = match &variant.fields {
                                    zil_syntax::ast::VariantFields::Unit => 0,
                                    zil_syntax::ast::VariantFields::Tuple(fields) => fields.len(),
                                    zil_syntax::ast::VariantFields::Record(fields) => fields.len(),
                                };
                                budget.identity_work(
                                    count.saturating_mul(2 * std::mem::size_of::<usize>()),
                                    variant.span,
                                )?;
                                let fields: Vec<_> = match &variant.fields {
                                    zil_syntax::ast::VariantFields::Unit => vec![],
                                    zil_syntax::ast::VariantFields::Tuple(fields) => fields
                                        .iter()
                                        .map(|field| (&field.ty, field.manages))
                                        .collect(),
                                    zil_syntax::ast::VariantFields::Record(fields) => fields
                                        .iter()
                                        .map(|field| (&field.ty, field.manages))
                                        .collect(),
                                };
                                for (ty, manages) in fields {
                                    budget.substituted_type(ty, &substitutions)?;
                                    let mut actual = ty.clone();
                                    Substitution {
                                        substitutions: &substitutions,
                                    }
                                    .ty(&mut actual);
                                    if manages {
                                        self.managed_value(
                                            &actual,
                                            capability,
                                            available,
                                            variables,
                                            budget,
                                            active,
                                            depth + 1,
                                        )?;
                                    } else {
                                        self.value_shape(
                                            &actual,
                                            capability,
                                            available,
                                            variables,
                                            budget,
                                            active,
                                            depth + 1,
                                        )?;
                                    }
                                }
                            }
                            Ok(())
                        }
                        _ => Err(error(
                            ty.span,
                            "Value requires an ordinary value; resource descriptions are not owners",
                        )),
                    }
                })();
                active.remove(&key);
                result
            }
            _ => Err(unsupported(
                ty.span,
                "Value requires a complete finite inline type",
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn managed_value(
        &self,
        ty: &TypeRef,
        capability: &Bound,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
        active: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<()> {
        budget.node(ty.span, depth)?;
        if matches!(&grouped(ty).kind,TypeKind::Named{path,..} if path.len()==1 && matches!(path[0].as_str(),"Task"|"AsyncRegistration"))
        {
            return self.value_shape(
                ty,
                capability,
                available,
                variables,
                budget,
                active,
                depth + 1,
            );
        }
        if let Some(element) = stores::element(ty) {
            self.store_element(
                element,
                available,
                variables,
                budget,
                callable::StoreCallbacks::None,
            )?;
            if matches!(capability, Bound::SyncDrop) {
                self.value_shape(
                    element,
                    capability,
                    available,
                    variables,
                    budget,
                    active,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        if stores::explicit_managed_value(
            &TypeRef {
                kind: TypeKind::Named {
                    path: vec!["Managed".into()],
                    args: vec![GenericArg::Type(ty.clone())],
                },
                span: ty.span,
            },
            &self.nominals,
            budget,
        )? {
            return Ok(());
        }
        Err(error(
            ty.span,
            "Value manages field requires its actual supported resource category",
        ))
    }
}
