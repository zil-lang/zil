extern "C" fn putchar(c:i32)->i32;
resource Mark{code:i32;check:[i32;2]}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code+.check[0]-.check[1])}}}
package Leaf{manages marker:Mark;pad:u8}
package Branch{left:Leaf;right:Leaf;serial:i64}
package Root{prefix:u8;child:Branch;manages end:Mark;tail:i64}
fn marker(code:i32)->Mark contract{result(create)}{return Mark{code:code,check:[13,13]}}
fn make(code:i32)->Root{
    return Root{prefix:9,child:Branch{left:Leaf{marker:marker(code),pad:1},right:Leaf{marker:marker(code+1),pad:2},serial:71},end:marker(code+2),tail:97}
}
fn push(s:Store<Root>,value:Root)->Root!void{return store_push(s,value)}
fn relay(s:Store<Root>,value:Root)->Root!void{push(s,value)?;return ()}
fn main()->Root!void{
    let s:Store<Root>=store_new()
    if !store_reserve(s,1){return ()}
    store_push(s,make(65))
    relay(s,make(68))?
    unsafe{putchar(88)}
    return ()
}
