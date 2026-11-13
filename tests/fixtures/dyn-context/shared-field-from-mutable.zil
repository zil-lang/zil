struct Pair{left:i32,right:i32}
struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn create<life a>(value:&'a mut Pair)->DynCreateError!OwnedDyn<dyn Read,Context<'a>>{
    return owned_dyn::<dyn Read,Context<'a>>(Provider::<'a>{pointer:&value.left})
}
fn run<life a>(value:&'a mut Pair)->i32{
    match create(value){.err(_)=>{return 99},.ok(owner)=>{return owner.read()}}
}
fn main()->i32{var value=Pair{left:42,right:99};return run(&mut value)-42}
