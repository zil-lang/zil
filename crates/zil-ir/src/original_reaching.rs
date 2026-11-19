//! The last direct write to an original lexical pointer slot. Consumers still
//! check the binder, RHS, complete type and every reaching executable Store.
use zil_base::NodeId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OriginalSlotWrite {
    #[default]
    Unknown,
    Write(NodeId),
    Exposed,
}
impl OriginalSlotWrite {
    pub fn assign(&mut self, writer: NodeId) {
        if *self != Self::Exposed {
            *self = Self::Write(writer);
        }
    }
    pub fn merged(self, other: Self) -> Self {
        if self == other {
            self
        } else if self == Self::Exposed || other == Self::Exposed {
            Self::Exposed
        } else {
            Self::Unknown
        }
    }
}
