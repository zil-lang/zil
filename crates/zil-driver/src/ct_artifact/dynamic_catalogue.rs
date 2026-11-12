//! Lossless raw metadata reification from the frozen Core inventory.
//! This bridge supplies data; it cannot authorize an implementation or source.
use super::*;
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
pub(super) fn from_core(
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
    for module in &core.modules {
        let core = &module.core;
        macro_rules! copy_rows {
            ($field:ident,$convert:ident,$ty:ty) => {{
                builder.rows(core.$field.len(), std::mem::size_of::<$ty>(), 1)?;
                result
                    .$field
                    .try_reserve_exact(core.$field.len())
                    .map_err(|_| limit())?;
                for value in &core.$field {
                    result.$field.push($convert(value, &mut builder, 2)?);
                }
            }};
        }
        copy_rows!(
            life_declarations,
            reify_life_nominal_declaration,
            CtLifeNominalDeclaration
        );
        copy_rows!(
            life_instances,
            reify_life_nominal_instance,
            CtLifeNominalInstance
        );
        builder.rows(
            core.dyn_declarations.len(),
            std::mem::size_of::<CtDynDeclaration>(),
            1,
        )?;
        result
            .declarations
            .try_reserve_exact(core.dyn_declarations.len())
            .map_err(|_| limit())?;
        for row in &core.dyn_declarations {
            result
                .declarations
                .push(reify_dyn_declaration(row, &mut builder, 2)?);
        }
        builder.rows(
            core.dyn_interfaces.len(),
            std::mem::size_of::<CtDynInterface>(),
            1,
        )?;
        result
            .interfaces
            .try_reserve_exact(core.dyn_interfaces.len())
            .map_err(|_| limit())?;
        for row in &core.dyn_interfaces {
            result
                .interfaces
                .push(reify_dyn_interface(row, &mut builder, 2)?);
        }
        builder.rows(
            core.dyn_vtables.len(),
            std::mem::size_of::<CtCoreDynVtable>(),
            1,
        )?;
        result
            .vtables
            .try_reserve_exact(core.dyn_vtables.len())
            .map_err(|_| limit())?;
        for row in &core.dyn_vtables {
            result
                .vtables
                .push(reify_core_dyn_vtable(row, &mut builder, 2)?);
        }
        builder.rows(
            core.functions.len(),
            std::mem::size_of::<CtDynFunctionInstanceRecord>()
                + std::mem::size_of::<CtFunctionSourceBindings>(),
            1,
        )?;
        result
            .function_instances
            .try_reserve_exact(core.functions.len())
            .map_err(|_| limit())?;
        result
            .function_sources
            .try_reserve_exact(core.functions.len())
            .map_err(|_| limit())?;
        for function in &core.functions {
            if let Some(instance) = &function.dyn_instance {
                result.function_instances.push(CtDynFunctionInstanceRecord {
                    module: core.module_id,
                    source: function.id,
                    instance: reify_dyn_function_instance(instance, &mut builder, 2)?,
                });
            }
            let captures = function
                .callable_return_contract
                .as_ref()
                .and_then(|contract| contract.capture_sources.as_ref());
            if function.retained_return_sources.is_some() || captures.is_some() {
                let mut sources=|rows:Option<&Vec<zil_ir::StorageSourceBinding>>|->CompileResult<Option<Vec<CtStorageSourceBinding>>>{
                    let Some(rows)=rows else{return Ok(None)};
                    builder.rows(rows.len(),std::mem::size_of::<CtStorageSourceBinding>(),2)?;
                    rows.iter().map(|row|reify_storage_source_binding(row,&mut builder,3)).collect::<CompileResult<Vec<_>>>().map(Some)
                };
                result.function_sources.push(CtFunctionSourceBindings {
                    module: core.module_id,
                    source: function.id,
                    retained_return_sources: sources(function.retained_return_sources.as_ref())?,
                    returned_capture_sources: sources(captures)?,
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
        max_nodes: remaining.max_generated_nodes.min(usize::MAX as u64) as usize,
        bytes: 0,
    };
    catalogue.visit_ct_dynamic(&mut cost, 0)?;
    Ok(cost.bytes)
}
