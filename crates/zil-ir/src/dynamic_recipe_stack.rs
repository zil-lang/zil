use crate::*;

#[derive(Default)]
struct Count {
    nodes: usize,
    scratch: usize,
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
        self.scratch = self.scratch.max(bytes);
        Ok(())
    }
    fn ty(&mut self, _: &Type, depth: usize) -> Result<(), ()> {
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
