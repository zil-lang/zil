pub trait Read { fn read(self: &Self) -> i32; }
pub struct Cursor { pub value: i32 }
impl Read for Cursor { fn read() -> i32 => .value }
pub fn cursor(value: i32) -> Cursor => Cursor { value: value }
