trait Read{fn read(self:&Self)->i32;}
package A{manages data:ByteBuffer;pub n:i32}
impl Read for A{fn read(self:&Self)->i32{return self.n}}
fn build()->DynCreateError!OwnedDyn<dyn Read>{
    let data=bytes_new();
    bytes_reserve(data,64);
    return owned_dyn::<dyn Read>(A{data:give data,n:41})
}
fn read_one()->DynCreateError!i32{
    let owner=build()?;
    return owner.read()
}
fn main()->i32{
    let made=read_one();
    match made{
        .ok(value)=>{return value},
        .err(_)=>{return 77}
    }
}
