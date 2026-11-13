struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32,count:i32}
trait Counter{fn step(self:&mut Self)->i32;}
impl<life a> Counter for Provider<'a>{
    fn step(self:&mut Self)->i32{self.count+=1;return self.count+*self.pointer}
}
fn keep<F:FnMut()->i32>(input:F)->F{return input}
fn run<life a>(data:&'a i32)->i32{
    match owned_dyn::<dyn Counter,Context<'a>>(Provider::<'a>{pointer:data,count:0}){
        .err(_)=>{return 99},
        .ok(owner)=>{
            var current=owner;
            var callback=keep(move()=>current.step());
            return callback()+callback();
        }
    }
}
fn main()->i32{let data=20;return run(&data)-43}
