//! Independent bounded structural admission of raw CT dynamic inventory.
//! Neither this catalogue nor a valid DAG grants runtime/provider authority.
use super::*;
use zil_ir::ct_artifact::{CtDynamicMetadata, CtDynamicVisitor};
struct Inventory<'a, 'b> {
    raw: &'a CtArtifactProgram,
    limits: &'a Limits,
    meter: &'b mut CtVerificationMeter,
    nodes: usize,
    bytes: usize,
    heights: Vec<usize>,
    current: Option<usize>,
    height: usize,
}
impl Inventory<'_, '_> {
    fn allocation(&mut self, bytes: usize) -> Check<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| limit(None, "CT dynamic metadata byte overflow"))?;
        if self.bytes > self.limits.max_source_bytes
            || self.bytes as u64 > self.raw.budget.max_temporary_bytes
        {
            return Err(limit(None, "CT dynamic metadata temporary bytes exhausted"));
        }
        Ok(())
    }
}
impl CtDynamicVisitor for Inventory<'_, '_> {
    type Error = Vec<Diagnostic>;
    fn node(&mut self, depth: usize) -> Check<()> {
        if depth
            >= self
                .limits
                .max_call_depth
                .min(self.raw.budget.max_call_depth as usize)
                .min(128)
        {
            return Err(limit(None, "CT dynamic metadata depth exhausted"));
        }
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or_else(|| limit(None, "CT dynamic node count overflow"))?;
        if self.nodes > self.limits.max_nodes
            || self.nodes as u64 > self.raw.budget.max_generated_nodes
        {
            return Err(limit(None, "CT dynamic metadata node quota exhausted"));
        }
        // One structural visit and the later full frozen-inventory comparison.
        self.meter.charge(2, None)
    }
    fn rows(&mut self, count: usize, bytes: usize, depth: usize) -> Check<()> {
        self.node(depth)?;
        if count > self.limits.max_nodes {
            return Err(limit(None, "CT dynamic row quota exhausted"));
        }
        self.meter.charge(count.saturating_mul(2), None)?;
        self.allocation(
            count
                .checked_mul(bytes)
                .ok_or_else(|| limit(None, "CT dynamic row bytes overflow"))?,
        )
    }
    fn type_node(&mut self, index: usize, node: &CtDynamicTypeNode, depth: usize) -> Check<()> {
        if index != self.heights.len() {
            return Err(reject(None, "CT metadata Type node order is inconsistent"));
        }
        self.current = Some(index);
        self.height = 1;
        node.visit_ct_dynamic(self, depth)?;
        if self.height
            >= self
                .limits
                .max_call_depth
                .min(self.raw.budget.max_call_depth as usize)
        {
            return Err(limit(None, "CT dynamic Type DAG depth exhausted"));
        }
        self.heights.push(self.height);
        self.current = None;
        Ok(())
    }
    fn type_reference(&mut self, id: CtDynamicTypeId, depth: usize) -> Check<()> {
        self.node(depth)?;
        let index = id.0 as usize;
        if index >= self.raw.dynamic_catalogue.types.len() {
            return Err(reject(
                None,
                "CT dynamic Type reference is outside the catalogue",
            ));
        }
        if let Some(current) = self.current {
            if index >= current {
                return Err(reject(
                    None,
                    "CT dynamic Type DAG contains a cycle or forward edge",
                ));
            }
            self.height = self.height.max(self.heights[index].saturating_add(1));
        }
        Ok(())
    }
    fn constant(&mut self, value: &Constant, depth: usize) -> Check<()> {
        self.node(depth)?;
        if let Constant::WideString(text) = value {
            self.rows(text.len(), 2, depth + 1)?;
        }
        Ok(())
    }
    fn text(&mut self, text: &str, depth: usize) -> Check<()> {
        self.node(depth)?;
        self.meter.charge(text.len().saturating_mul(2), None)?;
        self.allocation(text.len())
    }
}
pub(super) fn check(
    raw: &CtArtifactProgram,
    limits: &Limits,
    meter: &mut CtVerificationMeter,
) -> Check<()> {
    let count = raw.dynamic_catalogue.types.len();
    // Reserve the private postorder heights before constructing it.
    meter.charge(count, None)?;
    let bytes = count
        .checked_mul(std::mem::size_of::<usize>())
        .ok_or_else(|| limit(None, "CT dynamic DAG scratch overflow"))?;
    if count > limits.max_nodes || bytes as u64 > raw.budget.max_temporary_bytes {
        return Err(limit(None, "CT dynamic DAG scratch quota exhausted"));
    }
    let mut inventory = Inventory {
        raw,
        limits,
        meter,
        nodes: 0,
        bytes,
        heights: Vec::with_capacity(count),
        current: None,
        height: 1,
    };
    raw.dynamic_catalogue.visit_ct_dynamic(&mut inventory, 0)
}
