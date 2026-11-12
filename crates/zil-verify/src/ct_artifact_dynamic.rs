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
    type_count: usize,
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
            > self
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
        if index >= self.type_count {
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
    fn type_handle(&mut self, value: &CtTypeRef, depth: usize) -> Check<()> {
        if let Some(catalogue) = &value.scoped_catalogue {
            // Each nested handle owns a separate finite descriptor namespace.
            // These are structural bounds only, not frozen-stage authorization.
            self.rows(catalogue.types.len(), std::mem::size_of::<usize>(), depth)?;
            let heights =
                std::mem::replace(&mut self.heights, Vec::with_capacity(catalogue.types.len()));
            let state = (self.current.take(), self.height, self.type_count);
            self.height = 1;
            self.type_count = catalogue.types.len();
            let result = value.visit_ct_dynamic(self, depth);
            self.heights = heights;
            (self.current, self.height, self.type_count) = state;
            result
        } else {
            self.rows(value.nodes.len(), 0, depth)?;
            if value.nodes.iter().any(|node| {
                matches!(
                    node,
                    CtTypeNode::ScopedDyn { .. } | CtTypeNode::ScopedTask { .. }
                )
            }) {
                return Err(reject(
                    None,
                    "scoped Type node has no bound descriptor catalogue",
                ));
            }
            value.visit_ct_dynamic(self, depth)
        }
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
    let Some(catalogue) = &raw.dynamic_catalogue else {
        return Ok(());
    };
    let count = catalogue.types.len();
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
        type_count: count,
    };
    catalogue.visit_ct_dynamic(&mut inventory, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, ModuleId, NodeId};
    fn raw() -> CtArtifactProgram {
        let limits = Limits::default();
        CtArtifactProgram {
            schema_version: CT_ARTIFACT_SCHEMA_VERSION,
            target_pointer_width: 64,
            source_fingerprint: Fingerprint::ZERO,
            ambient_life_scopes: vec![],
            dynamic_catalogue: None,
            functions: vec![],
            entry: CtFunctionId(0),
            constants: vec![],
            record_schemas: vec![],
            enum_schemas: vec![],
            templates: vec![],
            input_tokens: CtInputTable {
                invocation_fingerprint: Fingerprint::ZERO,
                tokens: vec![],
            },
            snapshot: CtSnapshotRequest {
                stage: None,
                fingerprint: Fingerprint::ZERO,
                ready: CtReadyStage::Tokens,
                target_fingerprint: None,
                requested_queries: vec![],
            },
            budget: CtDeclaredBudget {
                max_steps: limits.max_ct_steps,
                max_call_depth: limits.max_call_depth as u32,
                max_live_cells: limits.max_values as u64,
                max_temporary_bytes: limits.max_source_bytes as u64,
                max_generated_nodes: limits.max_nodes as u64,
                max_instances: limits.max_functions as u64,
                max_analysis_work: limits.max_analysis_steps as u64,
            },
        }
    }
    fn fixture() -> CtArtifactProgram {
        let mut raw = raw();
        raw.dynamic_catalogue = Some(CtDynamicCatalogue::default());
        raw.dynamic_catalogue.as_mut().unwrap().types = vec![
            CtDynamicTypeNode::I32,
            CtDynamicTypeNode::Address {
                mutable: false,
                inner: Box::new(CtDynamicTypeId(0)),
            },
        ];
        raw.dynamic_catalogue
            .as_mut()
            .unwrap()
            .function_sources
            .push(CtFunctionSourceBindings {
                module: ModuleId(0),
                source: NodeId(7),
                retained_return_sources: Some(vec![CtStorageSourceBinding {
                    position: zil_ir::StoragePosition {
                        base: zil_ir::StorageBase::Result,
                        path: vec![],
                    },
                    sources: vec![CtStorageSource::Static {
                        leaf: CtDynamicTypeId(1),
                        mode: zil_ir::StoredBorrowMode::SharedAddress,
                    }],
                }]),
            });
        raw
    }
    #[test]
    fn ct_dynamic_raw_and_wire_type_edges_fail_closed_independently() {
        let limits = Limits::default();
        let good = fixture();
        // Structural metadata admission only: there is no runtime source permission
        // or executable/provider authority in these deliberately raw fixtures.
        for attack in 0..4 {
            let mut raw = good.clone();
            match attack {
                0 => {}
                1 => {
                    raw.dynamic_catalogue.as_mut().unwrap().types[1] = CtDynamicTypeNode::Address {
                        mutable: false,
                        inner: Box::new(CtDynamicTypeId(1)),
                    }
                }
                2 => {
                    raw.dynamic_catalogue.as_mut().unwrap().types[0] = CtDynamicTypeNode::Address {
                        mutable: false,
                        inner: Box::new(CtDynamicTypeId(1)),
                    }
                }
                _ => {
                    let CtStorageSource::Static { leaf, .. } =
                        &mut raw.dynamic_catalogue.as_mut().unwrap().function_sources[0]
                            .retained_return_sources
                            .as_mut()
                            .unwrap()[0]
                            .sources[0]
                    else {
                        unreachable!()
                    };
                    *leaf = CtDynamicTypeId(2);
                }
            }
            let bytes = zil_ir::ct_wire::encode_program(&raw, &limits).unwrap();
            let decoded = zil_ir::ct_wire::decode_program(&bytes, &limits).unwrap();
            for candidate in [&raw, &decoded] {
                let mut meter = CtVerificationMeter::new(u64::MAX);
                assert_eq!(check(candidate, &limits, &mut meter).is_ok(), attack == 0);
            }
        }
    }
    #[test]
    fn ct_dynamic_metadata_exact_work_and_fresh_retry_do_not_cache_permissions() {
        let limits = Limits::default();
        let raw = fixture();
        let mut measure = CtVerificationMeter::new(u64::MAX);
        check(&raw, &limits, &mut measure).unwrap();
        let n = measure.spent();
        for allowance in [n - 1, n] {
            let mut meter = CtVerificationMeter::new(allowance);
            let result = check(&raw, &limits, &mut meter);
            assert_eq!(result.is_ok(), allowance == n);
            if allowance < n {
                assert!(check(&raw, &limits, &mut meter).is_err());
            }
        }
        let mut fresh = CtVerificationMeter::new(n);
        check(&raw, &limits, &mut fresh).unwrap();
        assert_eq!(fresh.spent(), n);
    }
}
