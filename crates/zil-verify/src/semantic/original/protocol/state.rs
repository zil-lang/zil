//! Original owner slots and observations are separate namespaces. An
//! observation is useful only for the exact owner generation it observed.
use super::*;

pub(super) const OPEN: u8 = 1;
pub(super) const CLOSING: u8 = 2;
pub(super) const CLOSED: u8 = 4;
pub(super) const DEAD: u8 = 8;
pub(super) const LIVE: u8 = OPEN | CLOSING | CLOSED;

pub(super) fn bit(state: ProtocolState) -> u8 {
    match state {
        ProtocolState::Open => OPEN,
        ProtocolState::Closing => CLOSING,
        ProtocolState::Closed => CLOSED,
    }
}
pub(super) fn mask(states: &[ProtocolState]) -> u8 {
    states.iter().fold(0, |value, state| value | bit(*state))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Owner {
    pub(super) states: u8,
    pub(super) generation: u64,
    pub(super) task: bool,
    pub(super) owned: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Conditional {
    pub(super) root: Binder,
    pub(super) generation: u64,
    pub(super) pending: u8,
    pub(super) ready: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Value {
    Plain,
    Owner {
        root: Binder,
        borrowed: bool,
    },
    Fresh,
    FreshTask,
    FallibleTask,
    Poll {
        relations: Vec<Conditional>,
        tag: Option<ProtocolExit>,
    },
}
impl Value {
    fn width(&self) -> usize {
        match self {
            Self::Owner { root, .. } => binder_width(root),
            Self::Poll { relations, .. } => relations.iter().fold(1usize, |n, row| {
                n.saturating_add(binder_width(&row.root)).saturating_add(3)
            }),
            _ => 1,
        }
    }
}
fn binder_width(binder: &Binder) -> usize {
    match binder {
        Binder::Local { name, .. } => name.len().saturating_add(3),
        Binder::Capture { source, .. } => binder_width(source).saturating_add(1),
        _ => 3,
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub(super) owners: BTreeMap<Binder, Owner>,
    pub(super) values: BTreeMap<Binder, Value>,
}
impl State {
    pub(super) fn width(&self) -> usize {
        self.owners
            .keys()
            .fold(1usize, |n, key| {
                n.saturating_add(binder_width(key)).saturating_add(2)
            })
            .saturating_add(self.values.iter().fold(0usize, |n, (key, value)| {
                n.saturating_add(binder_width(key))
                    .saturating_add(value.width())
            }))
    }
    pub(super) fn copy(&self, b: &mut Budget) -> Check<Self> {
        b.spend(self.width())?;
        Ok(self.clone())
    }
    pub(super) fn invalidate(&mut self, root: &Binder, b: &mut Budget) -> Check<()> {
        b.spend(
            self.width()
                .saturating_mul(binder_width(root).saturating_add(1)),
        )?;
        for value in self.values.values_mut() {
            if matches!(value,Value::Poll{relations,..}if relations.iter().any(|row|&row.root==root))
            {
                *value = Value::Plain;
            }
        }
        Ok(())
    }
    pub(super) fn strip_observations(&mut self, b: &mut Budget) -> Check<()> {
        b.spend(self.width())?;
        for value in self.values.values_mut() {
            if matches!(value, Value::Poll { .. }) {
                *value = Value::Plain;
            }
        }
        Ok(())
    }
    pub(super) fn owner(&self, root: &Binder, b: &mut Budget) -> Check<&Owner> {
        b.spend(binder_width(root).saturating_add(1))?;
        let owner = self
            .owners
            .get(root)
            .ok_or_else(|| bad("original protocol operation has no initialized owner identity"))?;
        if owner.states == 0 || owner.states & DEAD != 0 {
            return Err(bad(
                "original protocol owner is consumed or only conditionally live",
            ));
        }
        Ok(owner)
    }
    pub(super) fn refine(
        &mut self,
        value: &Value,
        exit: ProtocolExit,
        b: &mut Budget,
    ) -> Check<bool> {
        let Value::Poll { relations, tag } = value else {
            return Ok(true);
        };
        if tag.is_some_and(|tag| tag != exit) {
            return Ok(false);
        }
        for row in relations {
            b.spend(binder_width(&row.root).saturating_add(2))?;
            let owner = self
                .owners
                .get_mut(&row.root)
                .ok_or_else(|| bad("original Poll observation lost its actual owner"))?;
            if owner.generation != row.generation {
                return Err(bad(
                    "original Poll observation belongs to an obsolete owner generation",
                ));
            }
            let selected = match exit {
                ProtocolExit::PollPending => row.pending,
                ProtocolExit::PollReady => row.ready,
                _ => {
                    return Err(bad(
                        "original Poll relation used as another return category",
                    ));
                }
            };
            owner.states &= selected;
            if owner.states == 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }
    // Loop heads deliberately discard observations. Their convergence key is
    // the finite state/slot graph, never a recycled poll or a numeric epoch.
    pub(super) fn same_loop(&self, other: &Self, b: &mut Budget) -> Check<bool> {
        b.spend(self.width().saturating_add(other.width()))?;
        Ok(self.values == other.values
            && self.owners.len() == other.owners.len()
            && self.owners.iter().all(|(root, owner)| {
                other
                    .owners
                    .get(root)
                    .is_some_and(|other| owner.states == other.states)
            }))
    }
}

pub(super) fn union_value(left: &Value, right: &Value, b: &mut Budget) -> Check<Value> {
    b.spend(left.width().saturating_add(right.width()))?;
    Ok(if left == right {
        left.clone()
    } else {
        Value::Plain
    })
}

impl Engine<'_> {
    pub(super) fn generation(&mut self, b: &mut Budget) -> Check<u64> {
        b.spend(1)?;
        let result = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| limit("original protocol generation identity exhausted"))?;
        Ok(result)
    }
    pub(super) fn update(
        &mut self,
        state: &mut State,
        root: &Binder,
        states: u8,
        b: &mut Budget,
    ) -> Check<u64> {
        state.owner(root, b)?;
        state.invalidate(root, b)?;
        let generation = self.generation(b)?;
        let owner = state
            .owners
            .get_mut(root)
            .ok_or_else(|| bad("original owner disappeared during phase update"))?;
        owner.states = states;
        owner.generation = generation;
        Ok(generation)
    }
    pub(super) fn join(&mut self, left: &State, right: &State, b: &mut Budget) -> Check<State> {
        b.spend(left.width().saturating_add(right.width()).saturating_mul(3))?;
        let mut result = left.copy(b)?;
        for (root, other) in &right.owners {
            match result.owners.get_mut(root) {
                Some(owner) => {
                    if owner.task != other.task || owner.owned != other.owned {
                        return Err(bad(
                            "original owner join changes its Task/registration or responsibility role",
                        ));
                    }
                    owner.states |= other.states;
                    if owner.generation != other.generation {
                        owner.generation = self.generation(b)?;
                    }
                }
                None => {
                    result.owners.insert(
                        root.clone(),
                        Owner {
                            states: other.states | DEAD,
                            generation: self.generation(b)?,
                            task: other.task,
                            owned: other.owned,
                        },
                    );
                }
            }
        }
        for (root, owner) in &mut result.owners {
            if !right.owners.contains_key(root) {
                owner.states |= DEAD;
                owner.generation = self.generation(b)?;
            }
        }
        for (key, value) in &mut result.values {
            *value = match right.values.get(key) {
                Some(other) => union_value(value, other, b)?,
                None => Value::Plain,
            };
        }
        for key in right.values.keys() {
            if !result.values.contains_key(key) {
                result.values.insert(key.clone(), Value::Plain);
            }
        }
        // Keeping a relation requires the exact generation on every path.
        for value in result.values.values_mut() {
            if matches!(value,Value::Poll{relations,..}if relations.iter().any(|row|result.owners.get(&row.root).is_none_or(|owner|owner.generation!=row.generation)))
            {
                *value = Value::Plain;
            }
        }
        Ok(result)
    }
    pub(super) fn merge(
        &mut self,
        destination: &mut Option<State>,
        source: Option<State>,
        b: &mut Budget,
    ) -> Check<()> {
        if let Some(source) = source {
            *destination = Some(match destination.take() {
                Some(old) => self.join(&old, &source, b)?,
                None => source,
            });
        }
        Ok(())
    }
    pub(super) fn bind(
        &mut self,
        state: &mut State,
        binder: Binder,
        value: Value,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(state.width().saturating_add(binder_width(&binder)))?;
        let value = self.task_bind(state, &binder, value, b)?;
        let value = if value == Value::Fresh {
            if state
                .owners
                .get(&binder)
                .is_some_and(|owner| owner.states != DEAD)
            {
                return Err(unsupported(
                    "original replacement reuses an owner birth while an earlier generation is live",
                    None,
                ));
            }
            let generation = self.generation(b)?;
            state.owners.insert(
                binder.clone(),
                Owner {
                    states: LIVE,
                    generation,
                    task: false,
                    owned: true,
                },
            );
            Value::Owner {
                root: binder.clone(),
                borrowed: false,
            }
        } else {
            value
        };
        if let Some(Value::Owner {
            root,
            borrowed: false,
        }) = state.values.get(&binder)
        {
            if !matches!(&value,Value::Owner{root:other,..}if other==root)
                && state
                    .owners
                    .get(root)
                    .is_some_and(|owner| owner.states != DEAD)
            {
                return Err(unsupported(
                    "original owner replacement requires its old generation to be consumed first",
                    None,
                ));
            }
        }
        state.values.insert(binder, value);
        Ok(())
    }
}
