package Job<life b>{manages bytes:ByteBuffer;items:[&'b i32;2]}
fn make<life b>(first:&'b i32,second:&'b i32)->Job<'b>{
    var bytes=bytes_new();bytes_reserve(bytes,8);bytes_push(bytes,65u8);bytes_push(bytes,66u8)
    return Job::<'b>{bytes:give bytes,items:[first,second]}
}
fn identity<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{return input}
fn relay<life b>(job:Job<'b>)->Job<'b>{return job}
fn main()->i32{
    var first=7
    var second=9
    let job=relay(make(&first,&second))
    let window=identity(&job)
    let description=window.bytes;let length=bytes_len(description);let capacity=bytes_capacity(description);let pointer=window.items[1]
    first=11
    if length==2 && capacity>=8 && *pointer==9{return 0}return 1
}
