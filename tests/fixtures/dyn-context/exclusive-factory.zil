struct Context<life a>{leaf:&'a mut i32}
struct Provider<life a>{pointer:&'a mut i32}
trait Change{fn step(self:&mut Self)->i32;}
impl<life a> Change for Provider<'a>{
    fn step(self:&mut Self)->i32{*self.pointer+=1;return *self.pointer}
}
fn create<life a>(value:&'a mut i32)->DynCreateError!OwnedDyn<dyn Change,Context<'a>>{
    return owned_dyn::<dyn Change,Context<'a>>(Provider::<'a>{pointer:value})
}
fn run<life a>(value:&'a mut i32)->i32{
    match create(value){
        .err(_)=>{return 99},
        .ok(value)=>{var owner=value;let first=owner.step();return first+owner.step()}
    }
}
fn main()->i32{var value=41;let result=run(&mut value);return result+value-128}
