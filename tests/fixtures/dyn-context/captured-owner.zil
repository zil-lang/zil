struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn keep<F:Fn()->i32>(input:F)->F{return input}
fn run<life a>(data:&'a i32)->i32{
    match owned_dyn::<dyn Read,Context<'a>>(Provider::<'a>{pointer:data}){
        .err(_)=>{return 99},
        .ok(owner)=>{
            let callback=keep(move()=>owner.read());
            return callback()+callback();
        }
    }
}
fn main()->i32{let data=42;return run(&data)-84}
