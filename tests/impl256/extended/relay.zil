import model
pub fn identity<F:FnOnce(i32)->i32>(f:F)->F{return f}
pub fn store_first<F:Fn(i32)->i32>(items:model.Items,f:F)->F{
    let held=model.checked(items)
    return f
}
pub fn union_last<F:FnOnce(i32)->i32>(prefix:i32,data:model.FaultData!model.Packet,f:F,right:bool)->F{
    if right{return identity(f)}
    return f
}
pub fn address<F:FnMut(&mut i32)->i32>(items:model.Items,f:F)->F{
    let held=model.checked(items)
    return f
}
pub fn slice<F:Fn(&[u8])->i32>(prefix:i32,f:F,items:model.Items)->F{
    let held=model.checked(items)
    return f
}
pub fn spin<F:FnOnce(i32)->i32>(count:i32,f:F)->F{
    var current=f
    for i in 0..count{current=identity(current)}
    return current
}
pub fn tick<F:FnMut(i32)->i32>(count:i32,f:F)->F{
    var current=f
    for i in 0..count{current(1)}
    return current
}
pub fn later<F:FnOnce()->i32>(early:model.Items,f:F,late:model.Items)->F{return f}
