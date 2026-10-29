import state
package Leaf{manages marker:state.Mark;manages bytes:ByteBuffer;sum:i32}
package Root{child:Leaf;id:i32}
pub fn active(id:i32)->impl FnMut(i32)->i32{
    var root=Root{child:Leaf{marker:state.Mark{code:96+id},bytes:bytes_new(),sum:id*10},id:id}
    return (event:i32)=>{
        var child=root.child
        if !bytes_reserve(child.bytes,bytes_len(child.bytes)+1){root.child=child;return -1}
        if !bytes_push(child.bytes,u8(event)){root.child=child;return -2}
        child.sum+=event
        let answer=child.sum
        root.child=child
        return answer
    }
}
pub fn once(id:i32)->impl FnOnce()->i32{
    let marker=state.Mark{code:48+id}
    let bytes=bytes_new()
    return move()=>{
        marker.code
        let owned=give bytes
        if !bytes_reserve(owned,1){return -1}
        if !bytes_push(owned,7u8){return -2}
        return 0
    }
}
