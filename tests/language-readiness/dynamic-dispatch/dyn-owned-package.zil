trait Read{fn read(self:&Self)->i32;}
package A{manages data:ByteBuffer;pub n:i32}
package B{manages data:ByteBuffer;pub n:i32;pub extra:i32}
impl Read for A{fn read(self:&Self)->i32{return self.n}}
impl Read for B{fn read(self:&Self)->i32{return self.n+self.extra}}
fn select(which:bool)->DynCreateError!OwnedDyn<dyn Read>{
    if which{return owned_dyn::<dyn Read>(A{data:bytes_new(),n:11})}
    return owned_dyn::<dyn Read>(B{data:bytes_new(),n:17,extra:2})
}
fn read_one(which:bool)->DynCreateError!i32{
    let owned=select(which)?;
    return owned.read()
}
fn main()->i32{
    let left=read_one(true);
    let right=read_one(false);
    return match left{.ok(v)=>v,.err(_)=>100}+match right{.ok(v)=>v,.err(_)=>100}
}
