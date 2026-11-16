struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32,number:i32}
trait View{fn view(self:&Self)->&i32;}
impl<life a> View for Provider<'a>{fn view(self:&Self)->&i32{return &self.number}}
fn run<life a>(value:&'a i32)->i32{
    match owned_dyn::<dyn View,Context<'a>>(Provider::<'a>{pointer:value,number:42}){
        .err(_)=>{return 99},
        .ok(owner)=>{let view=owner.view();return *view-42}
    }
}
fn main()->i32{let value=7;return run(&value)}
