
struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn read_owned<life a>(p:Provider<'a>)->i32{
    match owned_dyn::<dyn Read,Context<'a>>(p){.err(_)=>{return 42},.ok(owner)=>{return owner.read()}}
}
fn main()->i32{return 0}
