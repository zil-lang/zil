struct Context<life a>{leaf:&'a i32}
trait Read{fn read(self:&Self)->i32;}
fn failed_create<life a>(value:&'a i32)->DynCreateError!OwnedDyn<dyn Read,Context<'a>>{
    return error_err::<DynCreateError,OwnedDyn<dyn Read,Context<'a>>>(DynCreateError.Allocation)
}
fn main()->i32{
    var value=1;
    match failed_create(&value){
        .err(_)=>{value=42;return value-42},
        .ok(owner)=>{return owner.read()-1}
    }
}
