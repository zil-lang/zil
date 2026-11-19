//! Complete immutable actual-event coordinates. Every candidate is retained;
//! this directory decides no original role, class, source or permission.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum OperandKey {
    Parameter(u32),
    Capture(u32),
    Receiver,
    Value(ValueId),
    Local(LocalId),
    TypeArgument(u32, u32),
    Environment(u32),
    CoreExpression(zil_base::NodeId, u32),
    CoreLocal(zil_base::NodeId, u32),
}
impl OperandKey {
    pub(super) fn new(value: &SemanticOperand) -> Self {
        match value {
            SemanticOperand::Parameter(n) => Self::Parameter(*n),
            SemanticOperand::Capture(n) => Self::Capture(*n),
            SemanticOperand::Receiver => Self::Receiver,
            SemanticOperand::Value(id) => Self::Value(*id),
            SemanticOperand::Local(id) => Self::Local(*id),
            SemanticOperand::TypeArgument {
                scope: SemanticScopeRef::Ancestor(scope),
                ordinal,
            } => Self::TypeArgument(*scope, *ordinal),
            SemanticOperand::Environment(n) => Self::Environment(*n),
            SemanticOperand::CoreExpression {
                expression,
                occurrence,
            } => Self::CoreExpression(*expression, *occurrence),
        }
    }
}
pub(super) type Address = (SemanticInstanceId, BlockId, u32, u8);
pub(super) fn edge(edge: SemanticSiteEdge) -> u8 {
    match edge {
        SemanticSiteEdge::Before => 0,
        SemanticSiteEdge::After => 1,
    }
}
pub(super) fn operand(site: &SemanticSite) -> Option<OperandKey> {
    match &site.action {
        SemanticSiteAction::Construct { result, .. }
        | SemanticSiteAction::Closure { result, .. } => Some(OperandKey::new(result)),
        SemanticSiteAction::CallableCall { callee } => Some(OperandKey::new(callee)),
        SemanticSiteAction::TypeUse { subject, .. } => Some(OperandKey::new(subject)),
        SemanticSiteAction::DeclareLocal { local, .. } => Some(match local {
            SemanticLocalTarget::Typed(local) => OperandKey::Local(*local),
            SemanticLocalTarget::Core {
                statement,
                occurrence,
            } => OperandKey::CoreLocal(*statement, *occurrence),
        }),
        SemanticSiteAction::Call { .. }
        | SemanticSiteAction::CoreBodyCall
        | SemanticSiteAction::BodyCall { .. } => None,
    }
}
#[derive(Default)]
pub(super) struct Index {
    addresses: BTreeMap<Address, Vec<usize>>,
    operands: BTreeMap<(SemanticInstanceId, OperandKey), Vec<usize>>,
    expressions: BTreeMap<(SemanticInstanceId, zil_base::NodeId), Vec<usize>>,
}
pub(super) fn append<K: Ord, T>(
    rows: &mut BTreeMap<K, Vec<T>>,
    key: K,
    position: T,
    b: &mut Budget,
) -> CompileResult<()> {
    b.lookup(rows.len(), WORK)?;
    let row = rows.entry(key).or_default();
    b.spend(
        1 + if row.len() == row.capacity() {
            row.len()
        } else {
            0
        },
        WORK,
    )?;
    row.try_reserve(1)
        .map_err(|_| fail("actual event directory allocation failed"))?;
    row.push(position);
    Ok(())
}
impl Index {
    // The Actual Inventory already rejected duplicate world IDs and validated
    // each original definition's recipe-to-node relation. Borrow those rows
    // only for the same program. A frozen Source view builds its own directory.
    pub(super) fn from_actual(inventory: &Inventory<'_>, b: &mut Budget) -> CompileResult<Self> {
        let mut index = Self::default();
        let mut previous: Option<(SemanticInstanceId, std::sync::Arc<original_index::Index<'_>>)> = None;
        for (position, site) in inventory.program.sites.iter().enumerate() {
            b.spend(1, WORK)?;
            if previous.as_ref().is_none_or(|(id, _)| *id != site.instance) {
                let instance = inventory.instance(site.instance, b)?;
                previous = Some((instance.id, inventory.original_index(instance.definition, b)?));
            }
            let (_, directory) = previous.as_ref()
                .ok_or_else(|| fail("actual event has no original declaration directory"))?;
            if let Some(SemanticOriginalNode::Expression(node)) = directory.site_node(site.recipe, b)? {
                append(&mut index.expressions, (site.instance, node), position, b)?;
            }
            index.physical(site, position, b)?;
        }
        Ok(index)
    }

    pub(super) fn new(program: &SemanticModule, b: &mut Budget) -> CompileResult<Self> {
        let mut index = Self {
            addresses: BTreeMap::new(),
            operands: BTreeMap::new(),
            expressions: BTreeMap::new(),
        };
        let mut nodes = BTreeMap::new();
        for definition in &program.definitions {
            b.spend(1, WORK)?;
            if let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body {
                for link in site_nodes {
                    b.spend(1, WORK)?;
                    if let SemanticOriginalNode::Expression(node) = link.node {
                        append(&mut nodes, (definition.id,link.site),node,b)?;
                    }
                }
            }
        }
        let mut definitions = BTreeMap::new();
        for instance in &program.instances {
            b.lookup(definitions.len(),WORK)?;
            if definitions.insert(instance.id,instance.definition).is_some() {
                return Err(fail("actual operation directory repeats an instance identity"));
            }
        }
        for (position, site) in program.sites.iter().enumerate() {
            b.spend(1, WORK)?;
            b.lookup(definitions.len(),WORK)?;
            let owner=definitions.get(&site.instance)
                .ok_or_else(||fail("actual operation has no registered original instance"))?;
            b.lookup(nodes.len(),WORK)?;
            if let Some(originals)=nodes.get(&(*owner,site.recipe)) {
                for original in originals {
                    append(&mut index.expressions,(site.instance,*original),position,b)?;
                }
            }
            index.physical(site, position, b)?;
        }
        Ok(index)
    }
    fn physical(&mut self, site: &SemanticSite, position: usize, b: &mut Budget) -> CompileResult<()> {
        if let SemanticSiteAddress::Typed {
            block,
            instruction,
            edge: boundary,
        } = site.at
        {
            append(
                &mut self.addresses,
                (site.instance, block, instruction, edge(boundary)),
                position,
                b,
            )?;
        }
        if let Some(operand) = operand(site) {
            append(&mut self.operands, (site.instance, operand), position, b)?;
        }
        Ok(())
    }
    pub(super) fn address(&self, key: Address, b: &mut Budget) -> CompileResult<&[usize]> {
        b.lookup(self.addresses.len(), WORK)?;
        Ok(self.addresses.get(&key).map_or(&[], Vec::as_slice))
    }
    pub(super) fn operand(
        &self,
        instance: SemanticInstanceId,
        key: OperandKey,
        b: &mut Budget,
    ) -> CompileResult<&[usize]> {
        b.lookup(self.operands.len(), WORK)?;
        Ok(self
            .operands
            .get(&(instance, key))
            .map_or(&[], Vec::as_slice))
    }
    pub(super) fn expression(
        &self, instance:SemanticInstanceId, node:zil_base::NodeId,b:&mut Budget,
    ) -> CompileResult<&[usize]> {
        b.lookup(self.expressions.len(),WORK)?;
        Ok(self.expressions.get(&(instance,node)).map_or(&[],Vec::as_slice))
    }
}
