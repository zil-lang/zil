//! One ordinary raw plan fragment. No Analyze/Verify receipt is restored.
use super::*;

impl RawFunctionPlanEncoding {
    pub fn node_cost(&self) -> usize {
        self.encoding.nodes
    }
    pub fn matches_limits(&self, limits: &Limits) -> bool {
        self.encoding.limits == *limits
    }
}

pub fn encode_raw_function_plan(
    function: Arc<FunctionPlan>,
    limits: &Limits,
) -> CompileResult<Arc<RawFunctionPlanEncoding>> {
    public((|| {
        let mut writer = Writer::new(limits);
        writer.function_plan(&function)?;
        Ok(Arc::new(RawFunctionPlanEncoding {
            function,
            encoding: RawFunctionEncoding {
                bytes: writer.output.into(),
                limits: limits.clone(),
                nodes: limits.max_nodes - writer.nodes,
                fingerprint: OnceLock::new(),
                decoded: None,
            },
        }))
    })())
}

pub fn decode_raw_function_plan_shared_owner(
    bytes: Arc<[u8]>,
    limits: &Limits,
    remaining_nodes: usize,
) -> CompileResult<(Arc<FunctionPlan>, Arc<RawFunctionPlanEncoding>)> {
    let mut remaining = limits.clone();
    remaining.max_nodes = remaining_nodes.min(limits.max_nodes);
    public((|| {
        let mut reader = Reader::new(&bytes, &remaining)?;
        let function = Arc::new(reader.raw_function_plan()?);
        reader.finish()?;
        let encoding = Arc::new(RawFunctionPlanEncoding {
            function: function.clone(),
            encoding: RawFunctionEncoding {
                bytes: bytes.clone(),
                limits: limits.clone(),
                nodes: remaining.max_nodes - reader.nodes,
                fingerprint: OnceLock::new(),
                decoded: None,
            },
        });
        Ok((function, encoding))
    })())
}

impl Reader<'_> {
    pub(super) fn raw_function_plan(&mut self) -> WireResult<FunctionPlan> {
        let function = FunctionId(self.u32()?);
        let count = self.count(self.limits.max_blocks, 12)?;
        let mut blocks = Vec::with_capacity(count);
        for _ in 0..count {
            blocks.push(BlockPlan {
                block: BlockId(self.u32()?),
                initialized_in: self.locals()?,
                initialized_out: self.locals()?,
            });
        }
        let count = self.count(self.limits.max_values, 8)?;
        let mut holds = Vec::with_capacity(count);
        for _ in 0..count {
            let value = ValueId(self.u32()?);
            let count = self.count(self.limits.max_nodes, 1)?;
            let mut roots = Vec::with_capacity(count);
            for _ in 0..count {
                roots.push(match self.u8()? {
                    0 => HoldRoot::Local(LocalId(self.u32()?)),
                    1 => HoldRoot::Capture(self.u32()?),
                    2 => HoldRoot::Parameter(self.u32()?),
                    3 => HoldRoot::Static,
                    _ => return Err(malformed("E-WIRE-TAG", "unknown Hold root tag")),
                });
            }
            holds.push(ValueHold { value, roots });
        }
        Ok(FunctionPlan {
            function,
            blocks: blocks.into(),
            holds: holds.into(),
        })
    }
}
