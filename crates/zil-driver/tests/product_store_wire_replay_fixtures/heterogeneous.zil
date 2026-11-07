extern "C" fn putchar(c:i32)->i32;
extern "C" fn getchar()->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
resource Empty{}
impl Drop for Empty{fn drop(self:Self){unsafe{putchar(90)}}}
package Leaf{manages bytes:ByteBuffer;manages mark:Mark;pad:u8;samples:[i32;3]}
package Branch{left:Leaf;manages zero:Empty;right:Leaf;stamp:i64}
package Root{prefix:u8;branch:Branch;manages suffix:Mark;tag:i64}
fn leaf(code:i32,size:i64)->void!Leaf{
    let mark=Mark{code:code}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,size){return ()}
    return Leaf{bytes:give bytes,mark:give mark,pad:3,samples:[3,5,7]}
}
fn make(code:i32,size:i64)->void!Root{
    let left=leaf(code,size)?
    let right=leaf(code+1,size+4)?
    return Root{prefix:9,branch:Branch{left:left,zero:Empty{},right:right,stamp:i64(code)},suffix:Mark{code:code+2},tag:size+21}
}
fn main()->void!i32{
    var mode:i32=0
    unsafe{mode=getchar()-48}
    if mode<1 || mode>4{return 9}
    let s:Store<Root>=store_new()
    var capacity:i64=1
    if mode==2{capacity=2}
    if !store_reserve(s,capacity){return ()}
    let first=make(65,7)?
    store_push(s,first)
    if mode==1{return 0}
    if mode==4{
        if !store_reserve(s,2){return ()}
        let p=store_pop(s)?
        return i32(p.tag)-28
    }
    let second=make(68,17)?
    store_push(s,second)
    if mode==2{
        let p=store_pop(s)?
        let tag=p.tag
        let branch=p.branch
        let stamp=branch.stamp
        let left=branch.left
        return i32(tag)+i32(stamp)+left.samples[2]-113
    }
    return 0
}
