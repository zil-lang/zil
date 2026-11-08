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
fn main()->i32{
    let value=7
    let job=make(&value)
    if read(&job)==7{return 0}
    return 1
}
