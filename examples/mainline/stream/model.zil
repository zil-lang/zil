// Ordinary exported user types; LegacyOption and Result have no compiler privileges.
pub enum LegacyOption<T> { None, Some(T) }
pub enum Result<E, T> { Err(E), Ok(T) }

pub struct Stats {
    pub accepted: i32
    pub rejected: i32
    pub total: i32
}

pub fn empty() -> Stats => Stats { accepted: 0, rejected: 0, total: 0 }
pub fn identity<T>(value: T) -> T => value
