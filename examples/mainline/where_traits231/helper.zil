import model as m

pub fn read<T>(value:&T)->i32 where T:m.Read=>value.get()

pub fn relay<U>(value:&U)->i32 where U:m.Read=>read(value)
