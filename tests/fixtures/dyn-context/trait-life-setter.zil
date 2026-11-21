struct Context<life a>{value:&'a i32}
struct Pointer<life a>{value:&'a i32}
trait Set<life a>{
    fn set(self:&mut Self,next:&'a i32);
    fn read(self:&Self)->i32;
}
impl<life p> Set<'p> for Pointer<'p>{
    fn set(self:&mut Self,next:&'p i32){self.value=next}
    fn read(self:&Self)->i32{return *self.value}
}
fn create<life a>(initial:&'a i32)->DynCreateError!OwnedDyn<dyn Set<'a>,Context<'a>>{
    return owned_dyn::<dyn Set<'a>,Context<'a>>(Pointer::<'a>{value:initial})
}
fn update<life a>(owner:&mut OwnedDyn<dyn Set<'a>,Context<'a>>,next:&'a i32){
    owner.set(next)
}
fn run<life a>(initial:&'a i32,next:&'a i32)->i32{
    match create(initial){
        .err(_)=>{return 42},
        .ok(value)=>{
            var owner=value
            let before=owner.read()
            update(&mut owner,next)
            return before+owner.read()
        }
    }
}
fn main()->i32{
    let first=20
    let second=22
    return run(&first,&second)-42
}
