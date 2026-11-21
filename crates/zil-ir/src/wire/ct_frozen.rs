//! Bounded context requests; the frozen source itself is an external view.
use super::*;
use crate::ct_artifact::CtStageIdentity;

impl Writer<'_> {
    pub(super) fn ct_frozen_context(
        &mut self,
        context: Option<&CtFrozenContext>,
    ) -> WireResult<()> {
        self.boolean(context.is_some())?;
        let Some(context) = context else {
            return Ok(());
        };
        self.fingerprint(context.stage.source_fingerprint)?;
        self.u64(context.stage.epoch)?;
        self.fingerprint(context.stage.committed_generation_fingerprint)?;
        self.fingerprint(context.stage.declaration_inventory_fingerprint)?;
        match context.query.owner {
            CtQueryOwner::Instance(id) => {
                self.u8(0)?;
                self.u32(id.module.0)?;
                self.u32(id.index)?;
            }
            CtQueryOwner::Definition(id) => {
                self.u8(1)?;
                self.u32(id.module.0)?;
                self.u32(id.index)?;
            }
            CtQueryOwner::Module(id) => {
                self.u8(2)?;
                self.u32(id.0)?;
            }
        }
        match context.query.original {
            SemanticOriginalNode::Expression(id) => {
                self.u8(0)?;
                self.u32(id.0)?;
            }
            SemanticOriginalNode::Statement(id) => {
                self.u8(1)?;
                self.u32(id.0)?;
            }
        }
        self.u32(context.query.expression.0)?;
        self.u32(context.query.occurrence)?;
        match &context.query.wrapper {
            SemanticFunctionTarget::Core { module, function } => {
                self.u8(0)?;
                self.u32(module.0)?;
                self.u32(function.0)
            }
            SemanticFunctionTarget::CoreClosure {
                module,
                creator,
                expression,
                occurrence,
            } => {
                self.u8(1)?;
                self.u32(module.0)?;
                self.u32(creator.0)?;
                self.u32(expression.0)?;
                self.u32(*occurrence)
            }
            SemanticFunctionTarget::Typed(function) => {
                self.u8(2)?;
                self.u32(function.0)
            }
        }
    }
}

impl Reader<'_> {
    pub(super) fn ct_frozen_context(&mut self) -> WireResult<Option<CtFrozenContext>> {
        if !self.boolean()? {
            return Ok(None);
        }
        let stage = CtStageIdentity {
            source_fingerprint: self.fingerprint()?,
            epoch: self.u64()?,
            committed_generation_fingerprint: self.fingerprint()?,
            declaration_inventory_fingerprint: self.fingerprint()?,
        };
        let owner = match self.u8()? {
            0 => CtQueryOwner::Instance(SemanticInstanceId {
                module: ModuleId(self.u32()?),
                index: self.u32()?,
            }),
            1 => CtQueryOwner::Definition(SemanticDefinitionId {
                module: ModuleId(self.u32()?),
                index: self.u32()?,
            }),
            2 => CtQueryOwner::Module(ModuleId(self.u32()?)),
            _ => return Err(malformed("E-WIRE-TAG", "unknown frozen CT query owner")),
        };
        let original = match self.u8()? {
            0 => SemanticOriginalNode::Expression(NodeId(self.u32()?)),
            1 => SemanticOriginalNode::Statement(NodeId(self.u32()?)),
            _ => return Err(malformed("E-WIRE-TAG", "unknown frozen CT original node")),
        };
        let expression = NodeId(self.u32()?);
        let occurrence = self.u32()?;
        let wrapper = match self.u8()? {
            0 => SemanticFunctionTarget::Core {
                module: ModuleId(self.u32()?),
                function: NodeId(self.u32()?),
            },
            1 => SemanticFunctionTarget::CoreClosure {
                module: ModuleId(self.u32()?),
                creator: NodeId(self.u32()?),
                expression: NodeId(self.u32()?),
                occurrence: self.u32()?,
            },
            2 => SemanticFunctionTarget::Typed(FunctionId(self.u32()?)),
            _ => return Err(malformed("E-WIRE-TAG", "unknown frozen CT wrapper target")),
        };
        Ok(Some(CtFrozenContext {
            stage,
            query: CtQueryOrigin {
                owner,
                original,
                expression,
                occurrence,
                wrapper,
            },
        }))
    }
}
