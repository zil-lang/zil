struct Refs<life a,life b>{first:&'a i32;second:&'b i32}
package Job<life a,life b>{manages bytes:ByteBuffer;refs:Refs<'b,'a>}
fn make<life a,life b>(first:&'a i32,second:&'b i32)->Job<'a,'b>{
    var bytes=bytes_new()
    return Job::<'a,'b>{bytes:give bytes,refs:Refs::<'b,'a>{first:second,second:first}}
}
fn relay<life a,life b>(job:Job<'a,'b>)->Job<'a,'b>{return job}
fn read<life a,life b>(job:&Job<'a,'b>)->i32{return *job.refs.first*10+*job.refs.second}
fn main()->i32{
    var first=3
    var second=4
    let job=relay(make(&first,&second))
    if read(&job)==43{return 0}
    return 1
}
