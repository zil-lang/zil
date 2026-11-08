extern "C" fn putchar(c:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
struct Refs<life a>{pointer:&'a i32}
package Job<life a>{manages bytes:ByteBuffer;manages mark:Mark;refs:Refs<'a>}
fn relay<life a>(job:Job<'a>)->Job<'a>{return job}
fn read<life a>(job:&Job<'a>)->i32{return *job.refs.pointer}
fn make<life a>(pointer:&'a i32)->Job<'a>{
    var bytes=bytes_new()
    bytes_reserve(bytes,1)
    bytes_push(bytes,65u8)
    return Job::<'a>{bytes:give bytes,mark:Mark{code:82},refs:Refs::<'a>{pointer:pointer}}
}
fn main()->i32{
    var value=7
    if true{
        let job=make(&value)
        let moved=relay(job)
        if read(&moved)!=7{return 1}
    }
    value=9
    if value==9{return 0}
    return 2
}
