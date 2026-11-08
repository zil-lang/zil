package Job<life b>{manages bytes:ByteBuffer;items:[&'b i32;2]}
fn make<life b>(first:&'b i32,second:&'b i32)->Job<'b>{
    var bytes=bytes_new()
    return Job::<'b>{bytes:give bytes,items:[first,second]}
}
fn identity<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{return input}
fn relay<life b>(job:Job<'b>)->Job<'b>{return job}
fn main()->i32{
    var first=7
    var second=9
    let job=relay(make(&first,&second))
    let window=identity(&job)
    let pointer=window.items[1]
    first=11
    return *pointer-9
}
