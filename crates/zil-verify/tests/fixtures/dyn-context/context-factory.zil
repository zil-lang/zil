
struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn read_owned<life a>(p:Provider<'a>)->i32{
    match owned_dyn::<dyn Read,Context<'a>>(p){.err(_)=>{return 99},.ok(owner)=>{return owner.read()}}
}
fn create<life a>(value:&'a i32,other:&'a i32)->DynCreateError!OwnedDyn<dyn Read,Context<'a>>{
    return owned_dyn::<dyn Read,Context<'a>>(Provider::<'a>{pointer:value})
}
fn keep<life a>(owner:OwnedDyn<dyn Read,Context<'a>>)->OwnedDyn<dyn Read,Context<'a>>{
    return owner
}
fn start<life a>(value:&'a i32,other:&'a i32)->i32{
    match create(value,other){.err(_)=>{return 99},.ok(owner)=>{let forwarded=keep(owner);return forwarded.read()}}
}
fn main()->i32{let value=42;let other=99;return start(&value,&other)-42}
