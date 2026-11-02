import model
pub fn identity<F:FnOnce(i32)->i32>(f:F)->F{return f}
pub fn branch<F:FnOnce(i32)->i32>(f:F,right:bool)->F{
    let current=f
    if right{return current}
    return current
}
pub fn nested<F:FnOnce(i32)->i32>(f:F,right:bool)->F{
    let current=identity(f)
    return branch(current,right)
}
pub fn step<F:FnMut(i32)->i32>(f:F,delta:i32)->F{
    var current=f
    current(delta)
    return identity(current)
}
pub fn with_union<F:FnOnce(i32)->i32>(f:F,data:model.Meta!model.Packet)->F{return f}
pub fn with_product<F:FnOnce(i32)->i32>(f:F,data:model.Event)->F{return f}
pub fn owned_identity<F:FnOnce(model.Event)->model.Reply>(f:F)->F{return f}
pub fn owned_branch<F:FnOnce(model.Event)->model.Reply>(f:F,data:model.Packet,right:bool)->F{
    let current=f
    if right{return owned_identity(current)}
    return current
}
pub fn owned_nested<F:FnOnce(model.Event)->model.Reply>(f:F,data:model.Packet,right:bool)->F{
    return owned_branch(owned_identity(f),data,right)
}
pub fn fn_twice<F:Fn(i32)->i32>(f:F)->i32{
    let current=identity(f)
    return current(1)+current(2)
}
