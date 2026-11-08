extern "C" fn putchar(c:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
enum Fault{Stopped}
struct Refs<life a>{pointer:&'a i32}
package Job<life a>{manages bytes:ByteBuffer;manages mark:Mark;refs:Refs<'a>}
fn relay<life a>(job:Job<'a>)->Job<'a>{return job}
fn stop<life a>(pointer:&'a i32)->Fault!i32{
    var bytes=bytes_new()
    bytes_reserve(bytes,1)
    bytes_push(bytes,65u8)
    let job=Job::<'a>{bytes:give bytes,mark:Mark{code:70},refs:Refs::<'a>{pointer:pointer}}
    let moved=relay(job)
    if *moved.refs.pointer!=7{return 3}
    return Fault.Stopped
}
fn main()->i32{
    var value=7
    match stop(&value){.ok(_)=>{return 1},.err(_)=>{}}
    value=9
    if value==9{return 0}
    return 2
}
