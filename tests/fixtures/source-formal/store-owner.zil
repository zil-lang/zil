struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn run<life a>(data:&'a i32)->i32{
    let items:Store<OwnedDyn<dyn Read,Context<'a>>>=store_new();
    if !store_reserve(items,1){return 98}
    match owned_dyn::<dyn Read,Context<'a>>(Provider::<'a>{pointer:data}){
        .err(_)=>{return 99},
        .ok(owner)=>{
            match store_push(items,owner){.err(_)=>{return 97},.ok(_)=>{}}
            let value=store_at(items,0);
            return value.read();
        }
    }
}
fn main()->i32{let data=42;return run(&data)-42}
