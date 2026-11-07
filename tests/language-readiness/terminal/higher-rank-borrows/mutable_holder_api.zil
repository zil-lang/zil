pub struct MutableHolder<life a> { pointer: &'a mut i64 }

pub fn make<life a>(pointer: &'a mut i64) -> MutableHolder<'a> {
    return MutableHolder::<'a> { pointer: pointer };
}

pub fn increment<life a>(holder: &mut MutableHolder<'a>) {
    let pointer = holder.pointer;
    *pointer = *pointer + 1;
}
