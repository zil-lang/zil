//! Lossless raw metadata reification from the frozen Core inventory.
//! This bridge supplies data; it cannot authorize an implementation or source.
use zil_base::{CompileResult, Diagnostic, Limits, Phase};
use zil_ct::CtExecutionBudget;
use zil_ir::{Constant, CoreProgram, ct_artifact::*};
fn limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "CT dynamic metadata budget exhausted",
        None,
    )]
}
struct Builder<'a> {
    budget: &'a mut CtExecutionBudget,
    limits: &'a Limits,
    types: Vec<CtDynamicTypeNode>,
}
impl CtDynamicReifier for Builder<'_> {
    type Error = Vec<Diagnostic>;
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(limit());
        }
        self.budget.reserve_analysis_work(1)
    }
    fn rows(&mut self, count: usize, bytes: usize, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if count > self.limits.max_nodes {
            return Err(limit());
        }
        self.budget.reserve_analysis_work(count as u64)?;
        self.budget
            .reserve_temporary_bytes(count.checked_mul(bytes).ok_or_else(limit)? as u64)
    }
    fn text(&mut self, text: &str, depth: usize) -> CompileResult<String> {
        self.rows(text.len(), 1, depth)?;
        Ok(text.to_owned())
    }
    fn constant(&mut self, value: &Constant, depth: usize) -> CompileResult<Constant> {
        self.node(depth)?;
        match value {
            Constant::WideString(v) => self.rows(v.len(), 2, depth + 1)?,
            _ => {}
        }
        Ok(value.clone())
    }
    fn store_type(
        &mut self,
        node: CtDynamicTypeNode,
        depth: usize,
    ) -> CompileResult<CtDynamicTypeId> {
        self.node(depth)?;
        if self.types.len() >= self.limits.max_nodes {
            return Err(limit());
        }
        self.budget.reserve_generated_nodes(1)?;
        if self.types.len() == self.types.capacity() {
            let target = self
                .types
                .capacity()
                .saturating_mul(2)
                .max(4)
                .min(self.limits.max_nodes);
            let growth = target
                .checked_sub(self.types.capacity())
                .ok_or_else(limit)?;
            self.budget.reserve_analysis_work(self.types.len() as u64)?;
            self.budget.reserve_temporary_bytes(
                growth
                    .checked_mul(std::mem::size_of::<CtDynamicTypeNode>())
                    .ok_or_else(limit)? as u64,
            )?;
            self.types
                .try_reserve_exact(target - self.types.len())
                .map_err(|_| limit())?;
        }
        let id = CtDynamicTypeId(u32::try_from(self.types.len()).map_err(|_| limit())?);
        self.types.push(node);
        Ok(id)
    }
}
/// Reify an immutable Core snapshot as untrusted CT inventory data. The caller
/// must attach the result to its actual source snapshot and retain independent
/// authority checks. This function supplies no visibility or runtime permission.
pub fn from_core(
    core: &CoreProgram,
    budget: &mut CtExecutionBudget,
    limits: &Limits,
) -> CompileResult<CtDynamicCatalogue> {
    let mut builder = Builder {
        budget,
        limits,
        types: Vec::new(),
    };
    let mut result = CtDynamicCatalogue::default();
    builder.rows(core.modules.len(), 0, 0)?;
    let mut counts = [0usize; 7];
    for module in &core.modules {
        builder.node(1)?;
        let module = &module.core;
        for (count, length) in counts.iter_mut().zip([
            module.life_declarations.len(),
            module.life_instances.len(),
            module.dyn_declarations.len(),
            module.dyn_interfaces.len(),
            module.dyn_vtables.len(),
            0,
            0,
        ]) {
            *count = count.checked_add(length).ok_or_else(limit)?;
        }
        builder.rows(module.functions.len(), 0, 1)?;
        for function in &module.functions {
            counts[5] = counts[5]
                .checked_add(usize::from(function.dyn_instance.is_some()))
                .ok_or_else(limit)?;
            counts[6] = counts[6]
                .checked_add(usize::from(function.retained_return_sources.is_some()))
                .ok_or_else(limit)?;
        }
    }
    macro_rules! reserve_rows {
        ($field:ident, $index:expr, $ty:ty) => {{
            builder.rows(counts[$index], std::mem::size_of::<$ty>(), 1)?;
            result
                .$field
                .try_reserve_exact(counts[$index])
                .map_err(|_| limit())?;
        }};
    }
    reserve_rows!(life_declarations, 0, CtLifeNominalDeclaration);
    reserve_rows!(life_instances, 1, CtLifeNominalInstance);
    reserve_rows!(declarations, 2, CtDynDeclaration);
    reserve_rows!(interfaces, 3, CtDynInterface);
    reserve_rows!(vtables, 4, CtCoreDynVtable);
    reserve_rows!(function_instances, 5, CtDynFunctionInstanceRecord);
    reserve_rows!(function_sources, 6, CtFunctionSourceBindings);
    for module in &core.modules {
        builder.node(1)?;
        let core = &module.core;
        for row in &core.life_declarations {
            result
                .life_declarations
                .push(reify_life_nominal_declaration(row, &mut builder, 2)?);
        }
        for row in &core.life_instances {
            result
                .life_instances
                .push(reify_life_nominal_instance(row, &mut builder, 2)?);
        }
        for row in &core.dyn_declarations {
            result
                .declarations
                .push(reify_dyn_declaration(row, &mut builder, 2)?);
        }
        for row in &core.dyn_interfaces {
            result
                .interfaces
                .push(reify_dyn_interface(row, &mut builder, 2)?);
        }
        for row in &core.dyn_vtables {
            result
                .vtables
                .push(reify_core_dyn_vtable(row, &mut builder, 2)?);
        }
        builder.rows(core.functions.len(), 0, 1)?;
        for function in &core.functions {
            if let Some(instance) = &function.dyn_instance {
                result.function_instances.push(CtDynFunctionInstanceRecord {
                    module: core.module_id,
                    source: function.id,
                    instance: reify_dyn_function_instance(instance, &mut builder, 2)?,
                });
            }
            if let Some(rows) = &function.retained_return_sources {
                builder.rows(rows.len(), std::mem::size_of::<CtStorageSourceBinding>(), 2)?;
                let rows = rows
                    .iter()
                    .map(|row| reify_storage_source_binding(row, &mut builder, 3))
                    .collect::<CompileResult<Vec<_>>>()?;
                result.function_sources.push(CtFunctionSourceBindings {
                    module: core.module_id,
                    source: function.id,
                    retained_return_sources: Some(rows),
                });
            }
        }
    }
    result.types = builder.types;
    Ok(result)
}

struct CopyCost<'a> {
    budget: &'a mut CtExecutionBudget,
    max_depth: usize,
    max_nodes: usize,
    bytes: u64,
}
impl CtDynamicVisitor for CopyCost<'_> {
    type Error = Vec<Diagnostic>;
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        if depth >= self.max_depth.min(128) {
            return Err(limit());
        }
        self.budget.reserve_analysis_work(1)
    }
    fn rows(&mut self, count: usize, element_bytes: usize, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if count > self.max_nodes {
            return Err(limit());
        }
        self.budget.reserve_analysis_work(count as u64)?;
        self.bytes = self
            .bytes
            .checked_add(count.checked_mul(element_bytes).ok_or_else(limit)? as u64)
            .ok_or_else(limit)?;
        Ok(())
    }
    fn type_node(
        &mut self,
        _index: usize,
        node: &CtDynamicTypeNode,
        depth: usize,
    ) -> CompileResult<()> {
        node.visit_ct_dynamic(self, depth)
    }
    fn type_reference(&mut self, _id: CtDynamicTypeId, depth: usize) -> CompileResult<()> {
        self.node(depth)
    }
    fn constant(&mut self, value: &Constant, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if let Constant::WideString(text) = value {
            self.rows(text.len(), 2, depth + 1)?;
        }
        Ok(())
    }
    fn text(&mut self, value: &str, depth: usize) -> CompileResult<()> {
        self.rows(value.len(), 1, depth)
    }
}
pub(super) fn copy_cost(
    catalogue: &CtDynamicCatalogue,
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    let remaining = budget.remaining();
    let mut cost = CopyCost {
        budget,
        max_depth: remaining.max_call_depth as usize,
        max_nodes: remaining.max_analysis_work.min(usize::MAX as u64) as usize,
        bytes: 0,
    };
    catalogue.visit_ct_dynamic(&mut cost, 0)?;
    Ok(cost.bytes)
}
