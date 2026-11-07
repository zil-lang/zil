trait Change{fn set(self:&mut Self,value:i32)->();fn read(self:&Self)->i32;}
package A{manages data:ByteBuffer;pub n:i32}
package B{manages data:ByteBuffer;pub n:i32;pub extra:i32}
impl Change for A{
    fn set(self:&mut Self,value:i32)->(){self.n=value;return ()}
    fn read(self:&Self)->i32{return self.n}
}
impl Change for B{
    fn set(self:&mut Self,value:i32)->(){self.extra=value;return ()}
    fn read(self:&Self)->i32{return self.extra}
}
fn select(which:bool)->DynCreateError!OwnedDyn<dyn Change>{
    if which{return owned_dyn::<dyn Change>(A{data:bytes_new(),n:1})}
    return owned_dyn::<dyn Change>(B{data:bytes_new(),n:2,extra:3})
}
fn run(which:bool,value:i32)->DynCreateError!i32{
    var owned=select(which)?;
    owned.set(value);
    return owned.read()
}
fn main()->i32{
    let left=run(true,7);
    let right=run(false,9);
    var total:i32=0;
    match left{.ok(v)=>{total+=v},.err(_)=>{return 100}}
    match right{.ok(v)=>{total+=v},.err(_)=>{return 100}}
    return total
}
