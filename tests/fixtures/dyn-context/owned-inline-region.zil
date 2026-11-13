struct Pair{left:i32,right:i32}
struct Frame{first:Pair,second:Pair}
struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn create<life a>(value:&'a Frame)->DynCreateError!OwnedDyn<dyn Read,Context<'a>>{
    return owned_dyn::<dyn Read,Context<'a>>(Provider::<'a>{pointer:&value.first.left})
}
fn main()->i32{
    let value=Frame{first:Pair{left:42,right:43},second:Pair{left:91,right:92}};
    match create(&value){.err(_)=>{return 99},.ok(owner)=>{return owner.read()-42}}
}
