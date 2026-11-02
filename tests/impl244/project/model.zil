import portable
pub resource Mark{pub code:i32}
impl Drop for Mark{fn drop(self:Self){portable.emit(.code)}}
pub package Leaf{manages mark:Mark;manages bytes:ByteBuffer}
pub package Items{manages values:Store<Leaf>}
pub package Session{subscriptions:Items;nodes:Items;pub ready:bool}
pub package Fault{partial:Items;manages note:Mark}
pub fn leaf(give mark:Mark,give bytes:ByteBuffer)->Leaf{
    return Leaf{mark:give mark,bytes:give bytes}
}
pub fn items(give values:Store<Leaf>)->Items{return Items{values:give values}}
pub fn failure(partial:Items)->Fault{return Fault{partial:partial,note:Mark{code:69}}}
pub fn complete(subscriptions:Items,nodes:Items)->Session{
    return Session{subscriptions:subscriptions,nodes:nodes,ready:true}
}
