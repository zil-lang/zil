use zil_base::{FileId, Limits, ModuleId, SourceUnit};
use zil_ir::*;
fn expand(text: &str) -> CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "product237.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand_with_map_limits(&source, &syntax, &Limits::default())
        .unwrap()
        .core
}
const SOURCE: &str = r#"
resource Ticket{code:i32}
package Item<F>{manages ticket:Ticket;callback:F;value:i32}
package Packet<F>{manages marker:Ticket;callback:F}
package Bag<T:StoreElement>{manages items:Store<T>}
package Root<T:StoreElement>{child:Bag<T>}
fn wrap<T:StoreElement>(give items:Store<T>)->Root<T>{return Root::<T>{child:Bag::<T>{items:give items}}}
fn make()->impl Fn()->i32{let count=8;return move()=>count}
fn pack<F:Fn()->i32>(callback:F)->Packet<F>{return Packet::<F>{marker:Ticket{code:90},callback:callback}}
fn run<F:Fn()->i32>(packet:Packet<F>)->i32{
 let items:Store<Item<F>>=store_new();
 let root=wrap(give items);let child=root.child;
 return i32(store_len(child.items))
}
fn main()->i32{return run(pack(make()))}
"#;
#[test]
fn preserves_original_recipe_and_exact_ordinary_owner() {
    let core = expand(SOURCE);
    assert_eq!(core.aggregate_templates.len(), 2);
    let wrap = core.functions.iter().find(|f| f.name == "wrap").unwrap();
    let mut symbolic = 0;
    for a in &core.aggregate_applications {
        assert!(core.aggregates.iter().any(|d| d.id == a.aggregate));
        assert!(core.aggregate_templates.iter().any(|d| d.id == a.template));
        if matches!(
            a.arguments.as_slice(),
            [CoreAggregateArgument::Parameter { index: 0 }]
        ) {
            assert_eq!(
                a.owner,
                Some(CoreAggregateOwner {
                    module: core.module_id,
                    function: wrap.id
                })
            );
            symbolic += 1;
        }
    }
    assert_eq!(symbolic, 2);
    assert!(
        core.aggregate_templates
            .iter()
            .any(|t| matches!(t.fields[0].ty, CoreAggregateTypeRecipe::Apply { .. }))
    );
    assert!(
        core.aggregate_templates
            .iter()
            .any(|t| matches!(t.fields[0].ty, CoreAggregateTypeRecipe::Store { .. }))
    );
}

#[test]
fn concrete_existing_generic_fields_are_materialized_in_the_original_recipe() {
    let core = expand(
        "struct Cell<U>{value:U} resource R{n:i32} package Item{manages r:R} package Bag<T:StoreElement>{manages items:Store<T>;cookie:Cell<i32>} fn main(){let items:Store<Item>=store_new();let bag=Bag::<Item>{items:give items,cookie:Cell::<i32>{value:7}}}",
    );
    let recipe = &core.aggregate_templates[0].fields[1].ty;
    assert!(
        matches!(recipe,CoreAggregateTypeRecipe::Concrete(Type::Named(name)) if core.aggregates.iter().any(|a|a.name==*name)),
        "{recipe:#?}"
    );
}

#[test]
fn new_recipe_tables_have_exact_fresh_work_node_and_depth_boundaries() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "product237-budget.zil", SOURCE);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let default = Limits::default();
    let full = zil_expand::expand_with_map_limits(&source, &syntax, &default).unwrap();
    for dimension in ["work", "nodes", "depth"] {
        let probe = |n| {
            let mut limits = default.clone();
            match dimension {
                "work" => limits.max_analysis_steps = n,
                "nodes" => limits.max_nodes = n,
                _ => limits.max_call_depth = n,
            }
            zil_expand::expand_with_map_limits(&source, &syntax, &limits)
        };
        let mut high = match dimension {
            "work" => default.max_analysis_steps,
            "nodes" => default.max_nodes,
            _ => default.max_call_depth,
        };
        let mut low = 0;
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match probe(mid) {
                Ok(_) => high = mid,
                Err(ds) => {
                    assert!(
                        ds.iter()
                            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit),
                        "{dimension}: {ds:#?}"
                    );
                    low = mid;
                }
            }
        }
        assert_eq!(probe(high).unwrap().core, full.core);
        assert!(
            probe(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
        );
        assert_eq!(probe(high).unwrap().core, full.core);
        eprintln!("product237_expand_{dimension}_n={high}");
    }
}
