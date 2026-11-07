pub trait Read { fn value(self: &Self) -> i32; }
pub fn read<T>(item: &T) -> i32 where T: Read => item.value()
