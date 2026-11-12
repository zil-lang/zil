struct Holder<life a>{pointer:&'a i32}
struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn create<life a>(holder:&mut Holder<'a>)->DynCreateError!OwnedDyn<dyn Read,Context<'a>>{
    return owned_dyn::<dyn Read,Context<'a>>(Provider::<'a>{pointer:holder.pointer})
}
fn run<life a>(data:&'a i32,next:&'a i32)->i32{
    var holder=Holder::<'a>{pointer:data};
    match create(&mut holder){.err(_)=>{return 99},.ok(owner)=>{return owner.read()}}
}
fn main()->i32{let first=42;let next=91;return run(&first,&next)-42}
