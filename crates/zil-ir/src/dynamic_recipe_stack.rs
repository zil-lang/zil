use crate::*;

#[derive(Default)]
struct Count {
    nodes: usize,
    scratch: usize,
    scratch_limit: Option<usize>,
    types: Vec<Type>,
}
impl DynRecipeVisitor for Count {
    type Error = ();
    fn node(&mut self, depth: usize) -> Result<(), ()> {
        assert!(depth <= 32);
        self.nodes += 1;
        Ok(())
    }
    fn rows(&mut self, _: usize, depth: usize) -> Result<(), ()> {
        self.node(depth)
    }
    fn scratch(&mut self, bytes: usize, _: usize) -> Result<(), ()> {
        if self.scratch_limit.is_some_and(|limit| bytes > limit) {
            return Err(());
        }
        self.scratch = self.scratch.max(bytes);
        Ok(())
    }
    fn ty(&mut self, ty: &Type, depth: usize) -> Result<(), ()> {
        self.types.push(ty.clone());
        self.node(depth)
    }
    fn life(&mut self, _: &StorageLifeShape, depth: usize) -> Result<(), ()> {
        self.node(depth)
    }
    fn identity(&mut self, _: &LifeDeclarationIdentity, depth: usize) -> Result<(), ()> {
        self.node(depth)
    }
    fn position(&mut self, _: &StoragePosition, depth: usize) -> Result<(), ()> {
        self.node(depth)
    }
    fn constant(&mut self, _: &Constant, depth: usize) -> Result<(), ()> {
        self.node(depth)
    }
    fn text(&mut self, _: &str, depth: usize) -> Result<(), ()> {
        self.node(depth)
    }
}

#[test]
fn raw_metadata_walker_uses_a_bounded_frame_on_a_small_host_stack() {
    let counts = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut count = Count::default();
            SemanticTypeRecipe::Atom(Type::I32)
                .visit_dyn_metadata(&mut count, 0)
                .unwrap();
            count
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(counts.nodes, 2);
    assert_eq!(counts.scratch, 96);
}

#[test]
fn wide_original_tables_keep_every_row_in_order_at_the_single_row_scratch_peak() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let element = |i: u32| if i % 3 == 0 { Type::U64 } else { Type::I32 };
            let row = |i: u32| SemanticSite {
                instance: SemanticInstanceId::default(),
                recipe: SemanticSiteId(i),
                at: SemanticSiteAddress::Typed {
                    block: BlockId(0),
                    instruction: i,
                    edge: SemanticSiteEdge::Before,
                },
                action: SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Value(ValueId(i)),
                    expected: SemanticTypeValue::Resolved {
                        ty: element(i),
                        life: StorageLifeShape {
                            binder_count: 0,
                            slots: vec![],
                            retained: vec![],
                            nominal_arguments: vec![],
                        },
                        classes: vec![],
                    },
                },
            };
            let mut program = SemanticModule {
                definitions: vec![],
                instances: vec![],
                sites: vec![row(0)],
                dyn_scopes: vec![],
            };
            let mut one = Count::default();
            program.visit_dyn_metadata(&mut one, 0).unwrap();
            let width = 2048;
            program.sites = (0..width).map(row).collect();
            let mut many = Count {
                scratch_limit: Some(one.scratch),
                ..Count::default()
            };
            program.visit_dyn_metadata(&mut many, 0).unwrap();
            assert_eq!(many.types, (0..width).map(element).collect::<Vec<_>>());
            assert_eq!(many.scratch, one.scratch);
            let mut short = Count {
                scratch_limit: Some(one.scratch - 1),
                ..Count::default()
            };
            assert!(program.visit_dyn_metadata(&mut short, 0).is_err());
        })
        .unwrap()
        .join()
        .unwrap();
}
