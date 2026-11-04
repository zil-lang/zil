use super::*;
use crate::place_replacements::fixture;
fn graph() -> FunctionGraph {
    FunctionGraph {
        successors: vec![vec![]],
        predecessors: vec![vec![]],
        reachable: vec![true],
        package_views: Default::default(),
        role_data_uses: vec![],
    }
}
#[test]
fn independent_replacement_protocol_holds_base_until_commit_and_checks_tickets() {
    let limits = Limits::default();
    let module = fixture::module();
    let f = &module.functions[0];
    let graph = graph();
    let mut b = Budget::new(&limits);
    let inventory = Inventory::reconstruct(&module, f, &mut b).unwrap();
    let schedule = inventory.schedule(f, &graph, &mut b).unwrap();
    assert_eq!(schedule.retained[0][2], vec![ValueId(0)]);
    assert_eq!(schedule.retained[0][3], vec![ValueId(0)]);
    for case in 0..5 {
        let mut module = fixture::module();
        let f = &mut module.functions[0];
        match case {
            0 => {
                f.blocks[0].instructions.remove(1);
            }
            1 => {
                let duplicate = f.blocks[0].instructions[3].clone();
                f.blocks[0].instructions.push(duplicate);
            }
            2 => {
                f.blocks[0].instructions.pop();
            }
            3 => {
                f.blocks[0].instructions[1].result = Some(ValueId(1));
            }
            _ => {
                let InstructionKind::ReservePlaceAddress { path, .. } =
                    &mut f.blocks[0].instructions[1].kind
                else {
                    unreachable!()
                };
                path[0] = PlaceProjection::Field(9);
            }
        }
        let mut b = Budget::new(&limits);
        let result = Inventory::reconstruct(&module, &module.functions[0], &mut b)
            .and_then(|i| i.schedule(&module.functions[0], &graph, &mut b));
        assert!(result.is_err(), "case{case}");
    }
}
#[test]
fn independent_replacement_protocol_work_boundary_is_exact() {
    let module = fixture::module();
    let graph = graph();
    let run = |work| {
        let limits = Limits {
            max_analysis_steps: work,
            ..Limits::default()
        };
        let mut b = Budget::new(&limits);
        Inventory::reconstruct(&module, &module.functions[0], &mut b)
            .and_then(|i| i.schedule(&module.functions[0], &graph, &mut b))
    };
    let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        if run(middle).is_ok() {
            high = middle
        } else {
            low = middle + 1
        }
    }
    assert!(run(low).is_ok());
    assert_eq!(
        run(low - 1).err().unwrap().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert!(run(low).is_ok());
    println!("340 Verify static replacement protocol N={low}");
}
