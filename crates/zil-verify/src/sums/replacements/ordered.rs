//! Remove only repeated solves whose actual summary reads have not changed.
//! Function order and the original round boundaries remain observable here.
use super::*;
pub(super) struct Ordered {
    readers: Vec<BTreeSet<usize>>,
    current: Vec<bool>,
    next: Vec<bool>,
}
fn search(n: usize, b: &mut Budget) -> Check<()> {
    b.spend(
        n.checked_ilog2()
            .map_or(1, |n| n as usize + 2)
            .saturating_add(1),
    )
}
impl Ordered {
    pub(super) fn new(count: usize, b: &mut Budget) -> Check<Self> {
        b.spend(count)?;
        b.spend(count)?;
        b.spend(count)?;
        Ok(Self {
            readers: vec![BTreeSet::new(); count],
            current: vec![true; count],
            next: vec![false; count],
        })
    }
    pub(super) fn run(&mut self, index: usize, b: &mut Budget) -> Check<bool> {
        b.spend(2)?;
        let dirty = self
            .current
            .get_mut(index)
            .ok_or_else(|| invalid("summary scheduler function absent"))?;
        Ok(std::mem::replace(dirty, false))
    }
    pub(super) fn observe(
        &mut self,
        caller: usize,
        reads: BTreeSet<usize>,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(reads.len())?;
        #[cfg(test)]
        trace::observed(caller, &reads);
        for callee in reads {
            b.spend(1)?;
            let readers = self
                .readers
                .get_mut(callee)
                .ok_or_else(|| invalid("summary scheduler callee absent"))?;
            search(readers.len(), b)?;
            readers.insert(caller);
        }
        Ok(())
    }
    pub(super) fn changed(&mut self, callee: usize, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        let readers = self
            .readers
            .get(callee)
            .ok_or_else(|| invalid("summary scheduler changed callee absent"))?;
        b.spend(readers.len())?;
        for &caller in readers {
            b.spend(3)?;
            let dirty = if caller > callee {
                &mut self.current
            } else {
                &mut self.next
            };
            *dirty
                .get_mut(caller)
                .ok_or_else(|| invalid("summary scheduler caller absent"))? = true;
        }
        Ok(())
    }
    pub(super) fn advance(&mut self, b: &mut Budget) -> Check<bool> {
        b.spend(self.next.len())?;
        if !self.next.iter().any(|flag| *flag) {
            return Ok(false);
        }
        b.spend(2)?;
        // Every current position was visited in ascending order, including
        // foreign and clean positions. It is the empty next-round buffer.
        std::mem::swap(&mut self.current, &mut self.next);
        Ok(true)
    }
}
pub(super) fn record(reads: &RefCell<BTreeSet<usize>>, entry: usize, b: &mut Budget) -> Check<()> {
    let mut reads = reads.borrow_mut();
    search(reads.len(), b)?;
    reads.insert(entry);
    Ok(())
}
