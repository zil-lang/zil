//! Function-local retained borrow sites. A new holder of a borrowed argument
//! must not reuse the argument's own loan as its only retention authority.
use super::*;
type ContractPath = Vec<ContractProjection>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Destination {
    Result(ContractPath),
    After(StoragePosition),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Component {
    Summary(Source),
    Slot(StoragePosition),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    instruction: usize,
    destination: Destination,
    component: Component,
    mode: StoredBorrowMode,
}
#[derive(Default)]
pub(super) struct Sites(BTreeMap<Key, usize>);

fn key_work(destination: &Destination, component: &Component) -> usize {
    let to = match destination {
        Destination::Result(path) => path.len(),
        Destination::After(position) => position.path.len().saturating_add(1),
    };
    let from = match component {
        Component::Summary(source) => width(source),
        Component::Slot(position) => position.path.len().saturating_add(1),
    };
    to.saturating_add(from).saturating_add(4)
}

pub(super) fn borrowed_input(f: &TypedFunction, source: &Source) -> Check<bool> {
    let HoldRoot::Parameter(index) = source.base else {
        return Ok(false);
    };
    let ty = f
        .parameters
        .get(index as usize)
        .and_then(|local| f.locals.get(local.0 as usize))
        .ok_or_else(|| err("retained lease source has no actual parameter"))?;
    Ok(matches!(ty.ty, Type::Address { .. } | Type::Desc(_)))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn retain(
    sites: &mut Sites,
    loans: &mut Vec<Loan>,
    instruction: &Instruction,
    destination: Destination,
    component: Component,
    mode: StoredBorrowMode,
    original: &Permission,
    b: &mut Budget,
) -> Check<Permission> {
    permission_work(original, b)?;
    if original.sources.is_empty() {
        return Ok(original.clone());
    }
    let work = key_work(&destination, &component);
    b.spend(amount(sites.0.len().saturating_add(1), work)?)?;
    let key = Key {
        instruction: instruction as *const Instruction as usize,
        destination,
        component,
        mode,
    };
    let id = if let Some(id) = sites.0.get(&key) {
        *id
    } else {
        if sites.0.len() >= b.limits.max_nodes {
            return Err(limit("retained dyn lease sites exceed selected node bound"));
        }
        // Complete key, arena growth and both insertions are paid before
        // publication. Sites live only as long as this actual function solve.
        b.spend(work.saturating_add(loans.len()).saturating_add(4))?;
        let id = loans.len();
        loans.push(Loan::default());
        sites.0.insert(key, id);
        id
    };
    let row = loans
        .get_mut(id)
        .ok_or_else(|| err("retained dyn lease exceeds actual arena"))?;
    add_sources(&mut row.sources, &original.sources, b)?;
    add_loans(&mut row.parents, &original.loans, b)?;
    row.parents.retain(|parent| *parent != id);
    row.exclusive = matches!(
        mode,
        StoredBorrowMode::ExclusiveAddress | StoredBorrowMode::WritableDescription
    );
    b.stored_exclusive_loans |= row.exclusive;
    b.spend(1)?;
    Ok(Permission {
        sources: original.sources.clone(),
        loans: vec![id],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Instruction, Permission, Vec<Loan>) {
        let (module, _) = crate::tests::baseline();
        let source = Source {
            heap_parents: vec![],
            base: HoldRoot::Local(LocalId(0)),
            retained_port: None,
            leaf: vec![],
            inline: vec![],
            heap_element: None,
        };
        (
            Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::BorrowLocalMut { local: LocalId(0) },
                span: module.functions[0].span,
            },
            Permission {
                sources: vec![source.clone()],
                loans: vec![0],
            },
            vec![Loan {
                sources: vec![source],
                parents: vec![],
                exclusive: true,
            }],
        )
    }
    #[test]
    fn retained_mutable_argument_has_a_distinct_child_until_the_holder_retires() {
        for mode in [
            StoredBorrowMode::SharedAddress,
            StoredBorrowMode::ExclusiveAddress,
        ] {
            let (instruction, original, mut loans) = fixture();
            let mut sites = Sites::default();
            let mut b = Budget::new(&Limits::default());
            let row = retain(
                &mut sites,
                &mut loans,
                &instruction,
                Destination::Result(vec![ContractProjection::RetainedPort(0)]),
                Component::Summary(original.sources[0].clone()),
                mode,
                &original,
                &mut b,
            )
            .unwrap();
            assert_ne!(row.loans, original.loans);
            let live = ancestry(&row.loans, &loans, &mut b).unwrap();
            assert!(access(&original, true, &live, &loans, &mut vec![], &mut b).is_err());
            assert!(
                access(
                    &row,
                    mode == StoredBorrowMode::ExclusiveAddress,
                    &live,
                    &loans,
                    &mut vec![],
                    &mut b
                )
                .is_ok()
            );
            assert!(access(&original, true, &[], &loans, &mut vec![], &mut b).is_ok());
            let again = retain(
                &mut sites,
                &mut loans,
                &instruction,
                Destination::Result(vec![ContractProjection::RetainedPort(0)]),
                Component::Summary(original.sources[0].clone()),
                mode,
                &original,
                &mut b,
            )
            .unwrap();
            assert_eq!(again.loans, row.loans);
            assert_eq!(loans.len(), 2);
        }
    }
    #[test]
    fn metadata_carrier_and_port_namespace_select_different_retained_sites() {
        let (instruction, original, mut loans) = fixture();
        let mut sites = Sites::default();
        let mut b = Budget::new(&Limits::default());
        let mut ids = Vec::new();
        for field in [0, 1] {
            for capture in [0, 1] {
                let row = retain(
                    &mut sites,
                    &mut loans,
                    &instruction,
                    Destination::Result(vec![
                        ContractProjection::Place(PlaceProjection::Field(field)),
                        ContractProjection::Capture(capture),
                        ContractProjection::RetainedPort(0),
                    ]),
                    Component::Summary(original.sources[0].clone()),
                    StoredBorrowMode::SharedAddress,
                    &original,
                    &mut b,
                )
                .unwrap();
                assert!(!ids.contains(&row.loans[0]));
                ids.push(row.loans[0]);
            }
        }
    }
    #[test]
    fn retained_site_growth_and_reuse_have_fresh_exact_budget_boundaries() {
        let (instruction, original, initial) = fixture();
        let run = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            let mut b = Budget::new(&limits);
            let mut sites = Sites::default();
            let mut loans = initial.clone();
            for _ in 0..2 {
                retain(
                    &mut sites,
                    &mut loans,
                    &instruction,
                    Destination::Result(vec![ContractProjection::RetainedPort(0)]),
                    Component::Summary(original.sources[0].clone()),
                    StoredBorrowMode::SharedAddress,
                    &original,
                    &mut b,
                )?;
            }
            Ok::<_, Diagnostic>(())
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        assert!(run(high).is_ok());
        while low < high {
            let mid = low + (high - low) / 2;
            if run(mid).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert!(run(low).is_ok());
        assert_eq!(
            run(low - 1).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert!(run(low).is_ok());
    }
}
