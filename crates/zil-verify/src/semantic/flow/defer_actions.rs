//! Bind the registered action to its actual creator and consumed receiver.
//! The independent executable defer checker still proves scope activation,
//! adjacency, LIFO order and fulfillment along every actual exit.
use super::*;
use zil_base::NodeId;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Action {
    pub(super) node: NodeId,
    pub(super) child: SemanticInstanceId,
    pub(super) closure: ClosureId,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Registered {
    pub(super) scope: ScopeId,
    pub(super) action: Action,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Creation {
    action: Action,
    point: (BlockId, usize),
}

#[derive(Default)]
pub(super) struct Actions {
    created: BTreeMap<ValueId, Creation>,
    registered: BTreeMap<ValueId, (BlockId, usize)>,
}

fn lookup(len: usize) -> usize {
    len.saturating_add(1).ilog2() as usize + 1
}

impl Actions {
    pub(super) fn created(
        &mut self,
        value: ValueId,
        action: Action,
        block: BlockId,
        position: usize,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(lookup(self.created.len()))?;
        let actual = Creation {
            action,
            point: (block, position),
        };
        if self
            .created
            .insert(value, actual)
            .is_some_and(|previous| previous != actual)
        {
            return Err(bad(
                "actual defer construction changes its original action across CFG visits",
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn register(
        &mut self,
        context: &CheckedContext,
        block: &BasicBlock,
        position: usize,
        scope: ScopeId,
        local: LocalId,
        state: &mut State,
        values: &[Option<CheckedType>],
        environment: &EvalEnv,
        b: &mut Budget,
    ) -> Check<()> {
        let instruction = &block.instructions[position];
        let value = crate::defers::registration_value(block, position, local, instruction.span)?;
        b.spend(lookup(self.created.len()))?;
        let Some(&Creation {
            action,
            point: (creator_block, creator_position),
        }) = self.created.get(&value)
        else {
            return Err(bad(
                "defer registers another value instead of its actual original cleanup closure",
            ));
        };
        let before_store = position
            .checked_sub(2)
            .ok_or_else(|| bad("defer has no actual construction before its initial Store"))?;
        let creator = if matches!(block.instructions[before_store].kind, InstructionKind::StorageLive(actual) if actual == local)
        {
            before_store.checked_sub(1)
        } else {
            Some(before_store)
        };
        if creator_block != block.id || Some(creator_position) != creator {
            return Err(bad(
                "defer registration is separated from its actual original closure construction",
            )
            .with_note(format!(
                "action {:?} at {:?}:{creator_position}, register at {:?}:{position}",
                action.node, creator_block, block.id
            )));
        }
        let occupant = state
            .locals
            .get(local.0 as usize)
            .and_then(Option::as_deref)
            .ok_or_else(|| bad("defer registration has no initialized actual slot"))?;
        if occupant.ty != Type::Closure(action.closure)
            || !context.same_type(environment, occupant, value_at(values, value)?, b)?
        {
            return Err(bad(
                "defer registration changes the complete original action environment",
            ));
        }
        b.spend(lookup(self.registered.len()).saturating_add(lookup(state.deferred.len())))?;
        let point = (block.id, position);
        if self
            .registered
            .insert(value, point)
            .is_some_and(|old| old != point)
            || state
                .deferred
                .insert(local, Registered { scope, action })
                .is_some()
        {
            return Err(bad(
                "actual original defer closure registers more than once in one activation",
            ));
        }
        Ok(())
    }

    pub(super) fn finish(&self, b: &mut Budget) -> Check<()> {
        if self.created.len() != self.registered.len() {
            b.spend(
                self.created
                    .len()
                    .saturating_mul(lookup(self.registered.len())),
            )?;
            let unregistered = self
                .created
                .iter()
                .find(|(value, _)| !self.registered.contains_key(value));
            return Err(
                bad("actual original cleanup closure has no checked defer registration").with_note(
                    format!(
                        "unregistered source and SSA: {:?}",
                        unregistered.map(|(value, creation)| (creation.action.node, *value))
                    ),
                ),
            );
        }
        Ok(())
    }
}

pub(super) fn taken(
    block: &BasicBlock,
    position: usize,
    state: &mut State,
    b: &mut Budget,
) -> Check<Option<(ValueId, Action)>> {
    if state.deferred.is_empty() {
        return Ok(None);
    }
    let Some((local, value)) = crate::defers::preceding_take(block, position) else {
        return Ok(None);
    };
    b.spend(lookup(state.deferred.len()))?;
    let Some(row) = state.deferred.remove(&local) else {
        return Ok(None);
    };
    if !crate::defers::invokes_action(&block.instructions[position], value) {
        return Err(bad(
            "defer must invoke the actual immediately preceding registered TakeLocal result",
        )
        .with_note(format!(
            "action {:?} local {local:?}, required receiver {value:?} at {:?}:{position}",
            row.action.node, block.id
        )));
    }
    Ok(Some((value, row.action)))
}

pub(super) fn unregistered_slot(state: &State, local: LocalId, b: &mut Budget) -> Check<()> {
    if !state.deferred.is_empty() {
        b.spend(lookup(state.deferred.len()))?;
        if state.deferred.contains_key(&local) {
            return Err(bad(
                "ordinary replacement or retirement of an active original defer slot",
            ));
        }
    }
    Ok(())
}
