//! Private fixed points of the complete tag transfer. Strong roots prevent
//! address reuse and make every later COW edit miss this cache automatically.
use super::*;

struct Entry {
    ty: usize,
    root: Arc<Tree>,
}
pub(super) struct Cache {
    locals: usize,
    width: usize,
    rows: Option<Vec<Option<Entry>>>,
}
impl Cache {
    pub(super) fn new(locals: &[LocalDecl]) -> Self {
        Self {
            locals: locals.as_ptr() as usize,
            width: locals.len(),
            rows: None,
        }
    }
    pub(super) fn forget(
        &mut self,
        locals: &[LocalDecl],
        local: LocalId,
        value: &mut Arc<Tree>,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        budget.spend(2, "tag fixed point actual local and type identity")?;
        let declaration = locals
            .get(local.0 as usize)
            .ok_or_else(|| error("tag fixed point local absent"))?;
        let ty = &declaration.ty;
        if locals.as_ptr() as usize != self.locals || locals.len() != self.width {
            return value.forget_addressed_copy_tags(ty, budget);
        }
        budget.spend(1, "tag fixed point optional local inventory")?;
        if let Some(rows) = &self.rows {
            budget.spend(2, "tag fixed point actual local slot and immutable root")?;
            if rows[local.0 as usize].as_ref().is_some_and(|entry| {
                entry.ty == ty as *const Type as usize && Arc::ptr_eq(&entry.root, value)
            }) {
                return Ok(());
            }
        }
        // All classification, failures, recursive alternatives, moved markers
        // and births are handled by the original transfer before publication.
        value.forget_addressed_copy_tags(ty, budget)?;
        if self.rows.is_none() {
            let words = std::mem::size_of::<Option<Entry>>().div_ceil(std::mem::size_of::<usize>());
            budget.spend(
                self.width
                    .saturating_mul(words.saturating_add(2))
                    .saturating_add(2),
                "tag fixed point local allocation initialization and release",
            )?;
        }
        budget.spend(
            3,
            "tag fixed point slot and strong root publication and release",
        )?;
        self.rows
            .get_or_insert_with(|| (0..self.width).map(|_| None).collect())[local.0 as usize] =
            Some(Entry {
                ty: ty as *const Type as usize,
                root: Arc::clone(value),
            });
        Ok(())
    }
}
