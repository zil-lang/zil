fn with_counter<F:FnOnce(&AtomicI32)->i32>(initial:i32,body:F)->(bool,i32) {
    return atomic_scope(initial,body)
}
fn main()->i32 {
    let (ok,value)=with_counter(7,(cell:&AtomicI32)->i32=>atomic_load(cell))
    if !ok {return 1} return value-7
}
