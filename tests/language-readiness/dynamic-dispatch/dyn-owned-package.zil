trait Read{fn read(self:&Self)->i32;}
struct Meta{a:i32;b:i64}
enum Tag{Zero,Value(i32)}
package Inner{manages data:ByteBuffer;pub x:i32}
package A{manages data:ByteBuffer;pub n:i32}
package B{manages data:ByteBuffer;inner:Inner;manages spare:ByteBuffer;meta:Meta;tag:Tag;pair:(i32,i32);codes:[i32;2];pub n:i32;pub extra:i32}
impl Read for A{fn read(self:&Self)->i32{return self.n}}
impl Read for B{fn read(self:&Self)->i32{return self.extra}}
fn select(which:bool)->DynCreateError!OwnedDyn<dyn Read>{
    if which{return owned_dyn::<dyn Read>(A{data:bytes_new(),n:11})}
    return owned_dyn::<dyn Read>(B{data:bytes_new(),inner:Inner{data:bytes_new(),x:5},spare:bytes_new(),meta:Meta{a:1,b:2},tag:Tag.Value(4),pair:(3,4),codes:[1,2],n:17,extra:19})
}
fn read_one(which:bool)->DynCreateError!i32{
    let owned=select(which)?;
    return consume(owned)
}
fn consume(owned:OwnedDyn<dyn Read>)->i32{
    return owned.read()
}
fn main()->i32{
    let left=read_one(true);
    let right=read_one(false);
    var total:i32=0;
    match left{.ok(v)=>{total+=v},.err(_)=>{return 100}}
    match right{.ok(v)=>{total+=v},.err(_)=>{return 100}}
    return total
}
