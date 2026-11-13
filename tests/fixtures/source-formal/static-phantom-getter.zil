struct Context<life a>{text:&'a [u8]}
struct Provider<life a>{code:i32}
trait Text<life a>{fn text(self:&Self)->&'a [u8];}
impl<life a> Text<'a> for Provider<'a>{fn text(self:&Self)->&'a [u8]{return "abc"}}
fn create<life a>(text:&'a [u8])->DynCreateError!OwnedDyn<dyn Text<'a>,Context<'a>>{
    return owned_dyn::<dyn Text<'a>,Context<'a>>(Provider::<'a>{code:42});
}
fn inspect<life a>(owner:&OwnedDyn<dyn Text<'a>,Context<'a>>)->i32{
    let text=owner.text();
    return i32(text[0])-97;
}
fn main()->i32{
    match create("unused"){
        .err(_)=>{return 99},
        .ok(owner)=>{return inspect(&owner)}
    }
}
