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
fn identity<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{return input}fn main()->i32{var value=7;var spare=0;let job=make(&value);let window=&job;spare=11;if bytes_len(window.bytes)==0 && *window.refs.pointer==7{return 0}return 1}