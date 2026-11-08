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
fn inspect(buffer:ByteBuffer)->i64{return bytes_len(buffer)}fn main()->i32{var value=7;var job=make(&value);var current=&job;var iteration=0;var sum=i64(0);while iteration<2{let description=current.bytes;let exclusive=&mut job;sum=sum+inspect(description);current=&*exclusive;iteration=iteration+1}return i32(sum)}
