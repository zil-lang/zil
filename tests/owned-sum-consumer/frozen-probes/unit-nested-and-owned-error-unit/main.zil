extern "C" fn getchar()->i32;
extern "C" fn putchar(code:i32)->i32;
fn read()->i32{unsafe{return getchar()}}
fn emit(code:i32){unsafe{putchar(code)}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
package Leaf{manages mark:Mark;manages bytes:ByteBuffer}
fn make_flat(fail:bool)->void!Leaf{
    let mark=Mark{code:65}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1){return ()}
    if fail{return ()}
    bytes_push(bytes,7u8)
    return Leaf{mark:give mark,bytes:give bytes}
}
package Root{child:Leaf;tag:i32}
package InitError{manages mark:Mark}
fn make_nested(fail:bool)->void!Root{
    let value=make_flat(fail)?
    return Root{child:value,tag:7}
}
fn owned_failure()->InitError!void{return InitError{mark:Mark{code:69}}}
fn main()->void!i32{
    let input=read()
    emit(83)
    if input==50{owned_failure();emit(82);return 0}
    let result=make_nested(input==49)
    let held=result?
    emit(82)
    return 0
}
