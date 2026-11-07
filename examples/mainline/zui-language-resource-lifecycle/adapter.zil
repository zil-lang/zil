import model
pub fn read<T: model.Read>(value: &T) -> i32 => value.read()
