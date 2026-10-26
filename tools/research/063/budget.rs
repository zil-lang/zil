//! Logical work is shared over a complete candidate or oracle invocation.
pub type R<T> = Result<T, String>;
pub const LIMIT: usize = 100_000_000;
pub const MAX_BITS: usize = 4096;
pub const MAX_NODES: usize = 300_000;
pub const MAX_CACHE: usize = 600_000;
pub const MAX_ROWS: usize = 100_000;
pub const MAX_POINTS: usize = 20_000;
pub const MAX_SLOTS: usize = 4096;
pub const MAX_RELATIONS: usize = 200_000;
#[derive(Debug)]
pub struct Work {
    pub used: usize,
    pub limit: usize,
}
impl Work {
    pub fn new(limit: usize) -> Self {
        Self { used: 0, limit }
    }
    pub fn charge(&mut self, n: usize) -> R<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or("ResourceLimit: work overflow")?;
        if self.used > self.limit {
            return Err(format!(
                "ResourceLimit: work {} > {}",
                self.used, self.limit
            ));
        }
        Ok(())
    }
}
