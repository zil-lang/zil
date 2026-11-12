//! Expressions select candidate uppers; only actual Return facts prove them.
use super::*;

pub(super) fn mode(leaf: &Type) -> Check<StoredBorrowMode> {
    match leaf {
        Type::Address { mutable: false, .. } => Ok(StoredBorrowMode::SharedAddress),
        Type::Address { mutable: true, .. } => Ok(StoredBorrowMode::ExclusiveAddress),
        _ => Err(bad("source leaf has no independently checked access mode")),
    }
}
fn binding(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let HoldRoot::Parameter(parameter) = source.base else {
        return Err(bad("input expression has no actual parameter class"));
    };
    match &source.retained_port {
        Some(PortOrigin::Dyn { carrier, port }) => {
            source_relation(module, f, parameter, carrier, *port, b)
        }
        None => relation(f, StorageBase::Parameter(parameter), &source.leaf, b),
        _ => Err(bad(
            "source expression uses a different opaque carrier namespace",
        )),
    }
}
pub(super) fn resolve(
    module: &TypedModule,
    f: &TypedFunction,
    atom: &StorageSource,
    b: &mut Budget,
) -> Check<(Type, Option<StorageBinder>, Source)> {
    dynamic_metadata::source(atom, b)?;
    match atom {
        StorageSource::Exact(position) => {
            let (leaf, source) = input(module, f, position, b)?;
            let binder = binding(module, f, &source, b)?;
            Ok((leaf, Some(binder), source))
        }
        StorageSource::PlaceAddress { place, mode } => {
            let StorageBase::Parameter(index) = place.base else {
                return Err(bad("physical place source is not an actual input head"));
            };
            let Type::Address { mutable, inner } = parameter(f, index)? else {
                return Err(bad(
                    "physical place source has no actual borrowed address permission",
                ));
            };
            let selected_mutable = match mode {
                StoredBorrowMode::SharedAddress => false,
                StoredBorrowMode::ExclusiveAddress if *mutable => true,
                _ => {
                    return Err(bad(
                        "physical place source cannot authorize the requested access mode",
                    ));
                }
            };
            let path = path_places(&place.path, b)?;
            let endpoint = crate::dynamic_ports::physical_path(inner, &path, b)?;
            dynamic_metadata::ty(endpoint, b)?;
            let binder = relation(f, StorageBase::Parameter(index), &[], b)?;
            Ok((
                Type::Address {
                    mutable: selected_mutable,
                    inner: Box::new(endpoint.clone()),
                },
                Some(binder),
                Source {
                    base: HoldRoot::Parameter(index),
                    retained_port: None,
                    leaf: Vec::new(),
                    inline: path,
                    heap_element: None,
                    heap_parents: Vec::new(),
                },
            ))
        }
        StorageSource::Static { leaf, mode: access } => {
            if *access != StoredBorrowMode::SharedAddress || mode(leaf)? != *access {
                return Err(bad(
                    "static source does not authorize mutable or unproved storage",
                ));
            }
            // This expression alone creates no actual static evidence.
            Ok((
                leaf.clone(),
                None,
                Source {
                    base: HoldRoot::Static,
                    retained_port: None,
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_element: None,
                    heap_parents: Vec::new(),
                },
            ))
        }
        StorageSource::OwnedView { .. } | StorageSource::ReborrowedLeaf { .. } => Err(unsupported(
            "source view requires independently reconstructed owned region and child scope",
            None,
        )),
    }
}
