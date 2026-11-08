struct Refs<life a>{pointer:&'a i32}
package Job<life a>{manages bytes:ByteBuffer;refs:Refs<'a>}
fn make<life a>(pointer:&'a i32)->Job<'a>{
    var bytes=bytes_new()
    return Job::<'a>{bytes:give bytes,refs:Refs::<'a>{pointer:pointer}}
}
fn read<life a>(job:&Job<'a>)->i32{
    let child=&job.refs
    return *child.pointer
}
fn keep<life a>(input:&mut Job<'a>)->i32{return 7}fn main()->i32{var value=7;var job=make(&value);let exclusive=&mut job;let current=exclusive.bytes;let observed=0;return keep(exclusive)-7}