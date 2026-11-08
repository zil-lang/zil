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
package Head{manages bytes:ByteBuffer}fn head()->Head{var bytes=bytes_new();return Head{bytes:give bytes}}fn hold_head(input:&mut Head)->i32{return 7}fn inspect_heads(buffer:ByteBuffer,owner:&mut Head)->i64{let observed=bytes_len(buffer);hold_head(owner);return observed}fn main()->i32{var first=head();var second=head();let old=first.bytes;let exclusive=&mut second;let observed=inspect_heads(old,exclusive);if observed==0{return 0}return 1}