
struct Context<life a,life b>{left:&'a i32,right:&'b i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn run<life a,life b>(value:&'a i32,other:&'b i32)->i32{
    match owned_dyn::<dyn Read,Context<'a,'b>>(Provider::<'a>{pointer:value}){
        .err(_)=>{return 99},.ok(owner)=>{return owner.read()}
    }
}
fn main()->i32{let first=42;let second=99;return run(&first,&second)-42}
