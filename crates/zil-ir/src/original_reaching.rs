//! Reaching direct writes to an original mutable pointer slot. Each consumer
//! independently checks the lexical binder, RHS types and executable Stores.
use std::sync::Arc;
use zil_base::NodeId;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum OriginalSlotWrite {
    #[default]
    Unknown,
    Write(NodeId),
    Joined(Arc<[NodeId]>),
    Exposed,
}
impl OriginalSlotWrite {
    pub fn assign(&mut self, writer: NodeId) {
        if !matches!(self, Self::Exposed) {
            *self = Self::Write(writer);
        }
    }
    pub fn writers(&self) -> &[NodeId] {
        match self {
            Self::Write(node) => std::slice::from_ref(node),
            Self::Joined(rows) => rows,
            _ => &[],
        }
    }
    pub fn merged<E>(
        &self,
        other: &Self,
        mut pay: impl FnMut(usize) -> Result<(), E>,
    ) -> Result<Self, E> {
        match (self, other) {
            (Self::Exposed, _) | (_, Self::Exposed) => return Ok(Self::Exposed),
            (Self::Unknown, _) | (_, Self::Unknown) => return Ok(Self::Unknown),
            (Self::Write(a), Self::Write(b)) if a == b => return Ok(self.clone()),
            (Self::Joined(a), Self::Joined(b)) if Arc::ptr_eq(a, b) => return Ok(self.clone()),
            _ => {}
        }
        let left = self.writers();
        let right = other.writers();
        let size = left.len().saturating_add(right.len());
        pay(size.saturating_mul(4))?;
        let mut union = Vec::with_capacity(size);
        let mut a = left.iter().peekable();
        let mut b = right.iter().peekable();
        while let (Some(x), Some(y)) = (a.peek(), b.peek()) {
            match x.cmp(y) {
                std::cmp::Ordering::Less => union.push(*a.next().unwrap()),
                std::cmp::Ordering::Greater => union.push(*b.next().unwrap()),
                std::cmp::Ordering::Equal => {
                    union.push(*a.next().unwrap());
                    b.next();
                }
            }
        }
        union.extend(a.copied());
        union.extend(b.copied());
        Ok(if union.as_slice() == left {
            self.clone()
        } else if union.as_slice() == right {
            other.clone()
        } else {
            Self::Joined(union.into())
        })
    }
}
