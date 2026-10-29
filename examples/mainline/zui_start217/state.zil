import portable
pub resource Mark{pub code:i32}
impl Drop for Mark{fn drop(self:Self){portable.emit(.code)}}
pub package Node<F,G>{manages marker:Mark;pub active:F;pub once:G;pub id:i32;pub parent:i32}
pub fn make<F:FnMut(i32)->i32,G:FnOnce()->i32>(active:F,once:G,id:i32,parent:i32)->Node<F,G>{
    return Node::<F,G>{marker:Mark{code:64+id},active:active,once:once,id:id,parent:parent}
}
pub fn finish<F:FnMut(i32)->i32,G:FnOnce()->i32>(value:Node<F,G>)->i32{
    var current=value
    portable.emit(62)
    let result=current.once()
    if result<0{portable.emit(70);return 1}
    return 0
}
