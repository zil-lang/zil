//! Structural clone accounting is independent of semantic acceptance.
use std::mem::size_of;
use zil_ir::*;

struct Meter {
    remaining: usize,
    allocated: usize,
    scratch_peak: usize,
    nodes: usize,
    rows: usize,
    text: usize,
}

impl Meter {
    fn new(limit: usize) -> Self {
        Self {
            remaining: limit,
            allocated: 0,
            scratch_peak: 0,
            nodes: 0,
            rows: 0,
            text: 0,
        }
    }
}

impl DynRecipeVisitor for Meter {
    type Error = ();
    fn node(&mut self, _: usize) -> Result<(), ()> {
        self.nodes += 1;
        Ok(())
    }
    fn rows(&mut self, length: usize, _: usize) -> Result<(), ()> {
        self.rows += length;
        Ok(())
    }
    fn allocation(&mut self, bytes: usize, _: usize) -> Result<(), ()> {
        self.remaining = self.remaining.checked_sub(bytes).ok_or(())?;
        self.allocated += bytes;
        Ok(())
    }
    fn scratch(&mut self, bytes: usize, _: usize) -> Result<(), ()> {
        self.scratch_peak = self.scratch_peak.max(bytes);
        Ok(())
    }
    fn ty(&mut self, ty: &Type, _: usize) -> Result<(), ()> {
        assert_eq!(*ty, Type::I32);
        Ok(())
    }
    fn life(&mut self, _: &StorageLifeShape, _: usize) -> Result<(), ()> {
        unreachable!()
    }
    fn identity(&mut self, _: &LifeDeclarationIdentity, _: usize) -> Result<(), ()> {
        unreachable!()
    }
    fn position(&mut self, _: &StoragePosition, _: usize) -> Result<(), ()> {
        unreachable!()
    }
    fn constant(&mut self, _: &Constant, _: usize) -> Result<(), ()> {
        unreachable!()
    }
    fn text(&mut self, text: &str, _: usize) -> Result<(), ()> {
        self.text += text.len();
        Ok(())
    }
}

#[test]
fn metadata_clone_budget_counts_heap_layout_and_traversal_separately() {
    let recipe = SemanticTypeRecipe::Tuple(vec![SemanticTypeRecipe::Slice(Box::new(
        SemanticTypeRecipe::Atom(Type::I32),
    ))]);
    let bytes = 2 * size_of::<SemanticTypeRecipe>();
    let mut exact = Meter::new(bytes);
    recipe.visit_dyn_metadata(&mut exact, 0).unwrap();
    assert_eq!(exact.allocated, bytes);
    assert_eq!(exact.remaining, 0);
    assert_eq!(exact.rows, 1);
    assert!(exact.nodes >= 3);
    assert!(exact.scratch_peak > 0);
    assert!(
        recipe
            .visit_dyn_metadata(&mut Meter::new(bytes - 1), 0)
            .is_err()
    );

    let name = SemanticOriginalNameBinding {
        path: vec!["module".into(), "value".into()],
        namespace: SemanticOriginalNamespace::Value,
        value: SemanticOriginalNameValue::Definition(SemanticDefinitionId::default()),
    };
    let bytes = 2 * size_of::<String>() + "modulevalue".len();
    let mut exact = Meter::new(bytes);
    name.visit_dyn_metadata(&mut exact, 0).unwrap();
    assert_eq!(exact.allocated, bytes);
    assert_eq!(exact.text, 11);
    assert!(
        name.visit_dyn_metadata(&mut Meter::new(bytes - 1), 0)
            .is_err()
    );

    // A scalar original-node reference has no cloned heap allocation.
    let mut scalar = Meter::new(0);
    SemanticOriginalNode::Expression(zil_base::NodeId(9))
        .visit_dyn_metadata(&mut scalar, 0)
        .unwrap();
    assert_eq!(scalar.allocated, 0);
    assert!(scalar.nodes > 0);
}
