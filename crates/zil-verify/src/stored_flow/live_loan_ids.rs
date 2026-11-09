//! One-call active-loan accumulator. Starting empty and admitting only paid
//! binary insertion makes ordering local state, not a claim about payload rows.
use super::*;

#[derive(Default)]
pub(super) struct SortedLoanIds {
    ids: Vec<usize>,
}
impl SortedLoanIds {
    pub(super) fn as_slice(&self) -> &[usize] {
        &self.ids
    }
    pub(super) fn add(&mut self, from: &[usize], b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        for id in from {
            let (mut low, mut high) = (0usize, self.ids.len());
            let mut found = false;
            while low < high {
                b.spend(1)?;
                let mid = low + (high - low) / 2;
                match self.ids[mid].cmp(id) {
                    std::cmp::Ordering::Less => low = mid + 1,
                    std::cmp::Ordering::Greater => high = mid,
                    std::cmp::Ordering::Equal => {
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                b.spend(self.ids.len().saturating_sub(low).saturating_add(1))?;
                if self.ids.len() == self.ids.capacity() {
                    b.spend(self.ids.len().saturating_add(1))?;
                }
                self.ids.insert(low, *id);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn live_loans_by_normalizing(
        solution: &Solution,
        state: &Frame,
        live: &WordRow,
        definition: Option<ValueId>,
        indexed: Option<&[usize]>,
        b: &mut Budget,
    ) -> Check<Vec<usize>> {
        b.spend(indexed.map_or(live.len(), |ids| {
            ids.len().saturating_add(live.len() - solution.values.len())
        }))?;
        let mut loans = Vec::new();
        let mut collect = |id: usize| -> Check<()> {
            if !live.is_live(id)? || definition.is_some_and(|value| value.0 as usize == id) {
                return Ok(());
            }
            let payload = if id < solution.values.len() {
                &solution.values[id]
            } else {
                &state.locals[id - solution.values.len()]
            };
            add_loans(&mut loans, &payload.address.loans, b)?;
            if b.task_borrowed_results {
                for (_, p) in &payload.task_ports {
                    add_loans(&mut loans, &p.loans, b)?;
                }
            }
            for (_, p) in &payload.leaves {
                add_loans(&mut loans, &p.loans, b)?;
            }
            Ok(())
        };
        if let Some(ids) = indexed {
            for &id in ids {
                collect(id)?;
            }
            for id in solution.values.len()..live.len() {
                collect(id)?;
            }
        } else {
            for id in 0..live.len() {
                collect(id)?;
            }
        }
        if b.task_borrowed_results {
            // The actual frame retains its inputs until its owning carrier moves
            // or is discharged; dead SSA uses of a head do not discharge a frame.
            for payload in &state.locals {
                for (_, permission) in &payload.task_ports {
                    add_loans(&mut loans, &permission.loans, b)?;
                }
            }
        }
        // Only an actual slot write can create a new external loan that is not
        // already represented by the original parameter relation.
        if b.stored_slot_updates {
            for holder in &state.external {
                for (_, p) in &holder.leaves {
                    add_loans(&mut loans, &p.loans, b)?;
                }
            }
        }
        ancestry(&loans, &solution.loans, b)
    }

    fn permission(ids: &[usize]) -> Permission {
        Permission {
            loans: ids.to_vec(),
            ..Permission::default()
        }
    }
    #[test]
    fn sorted_accumulator_keeps_arbitrary_contributions_and_exact_fresh_work() {
        let rounds = [
            (0..512).rev().collect::<Vec<_>>(),
            (0..512).map(|id| id * 2).collect(),
            vec![usize::MAX, 0, 17, usize::MAX],
            vec![],
            (0..1024).rev().collect(),
        ];
        let mut old = Vec::new();
        let mut new = SortedLoanIds::default();
        let mut old_b = Budget::new(&Limits::default());
        let mut new_b = Budget::new(&Limits::default());
        for from in &rounds {
            add_loans_by_scan(&mut old, from, &mut old_b).unwrap();
            new.add(from, &mut new_b).unwrap();
            assert_eq!(new.as_slice(), old);
        }
        assert!(new_b.remaining > old_b.remaining);
        let n = Limits::default().max_analysis_steps - new_b.remaining;
        for steps in [n, n - 1, n] {
            let mut acc = SortedLoanIds::default();
            let mut b = Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            });
            let result = rounds.iter().try_for_each(|from| acc.add(from, &mut b));
            assert!(acc.as_slice().windows(2).all(|p| p[0] < p[1]));
            if steps == n {
                result.unwrap();
                assert_eq!(acc.as_slice(), old);
            } else {
                assert_eq!(
                    result.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
    }
    #[test]
    fn active_accumulator_equals_original_for_all_contributors_and_ancestry_errors() {
        let mut b = Budget::new(&Limits::default());
        let mut values = vec![Payload::default(); 6];
        values[0].address = permission(&[3, 1, 3]);
        values[1]
            .leaves
            .push(
                (vec![PlaceProjection::Field(0)], permission(&[2, 0, 2])),
                &mut b,
            )
            .unwrap();
        values[2].task_ports.push((vec![], permission(&[4, 4, 1])));
        values[3].store_description = permission(&[5]);
        values[5].address = permission(&[3]);
        let mut state = Frame {
            locals: vec![Payload::default(); 2],
            ..Frame::default()
        };
        state.locals[0].address = permission(&[5]);
        state.locals[0]
            .task_ports
            .push((vec![], permission(&[6, 1])));
        state.locals[1].address = permission(&[7]);
        let mut external = Payload::default();
        external
            .leaves
            .push((vec![], permission(&[8, 4])), &mut b)
            .unwrap();
        state.external.push(external);
        let mut solution = Solution {
            values,
            outputs: vec![],
            entry: Frame::default(),
            loans: vec![Loan::default(); 12],
            sites: vec![],
        };
        solution.loans[1].parents = vec![0];
        solution.loans[3].parents = vec![2];
        solution.loans[6].parents = vec![9];
        solution.loans[9].parents = vec![10];
        solution.loans[10].parents = vec![9];
        let indices = (0..solution.values.len()).collect::<Vec<_>>();
        for invalid in [0, 1, 2] {
            solution.loans[2].parents.clear();
            solution.values[5].address = permission(&[3]);
            if invalid == 1 {
                solution.loans[2].parents = vec![usize::MAX];
            }
            if invalid == 2 {
                solution.values[5].address.loans = vec![usize::MAX];
            }
            for task in [false, true] {
                for external in [false, true] {
                    for definition in [None, Some(ValueId(0)), Some(ValueId(1))] {
                        for indexed in [None, Some(indices.as_slice())] {
                            let live = WordRow::from_bools(&[
                                true, true, true, true, false, true, false, true,
                            ]);
                            let budget = || {
                                let mut b = Budget::new(&Limits::default());
                                b.task_borrowed_results = task;
                                b.stored_slot_updates = external;
                                b
                            };
                            let old = live_loans_by_normalizing(
                                &solution,
                                &state,
                                &live,
                                definition,
                                indexed,
                                &mut budget(),
                            );
                            let new = live_loans(
                                &solution,
                                &state,
                                &live,
                                definition,
                                indexed,
                                &mut budget(),
                            );
                            assert_eq!(
                                new,
                                old,
                                "flags={task}/{external} definition={definition:?} indexed={} invalid={invalid}",
                                indexed.is_some()
                            );
                            if invalid == 0 {
                                let ids = new.unwrap();
                                assert!(
                                    ids.contains(&3),
                                    "another payload retains excluded SSA's loan"
                                );
                                assert_eq!(ids.contains(&6), task, "dead local retains task port");
                                assert_eq!(ids.contains(&8), external, "external slot flag");
                            }
                        }
                    }
                }
            }
        }
    }
}
