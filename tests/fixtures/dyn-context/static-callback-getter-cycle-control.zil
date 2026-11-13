struct Context<life a>{text:&'a [u8]}
struct Provider<life a>{text:&'a [u8]}
trait Text<life a>{fn text(self:&Self)->&'a [u8];}
impl<life a> Text<'a> for Provider<'a>{fn text(self:&Self)->&'a [u8]{return self.text}}
fn feedback<life a>(seed:&'a [u8],flag:bool)->&'a [u8]{
    match create(seed,flag){
        .err(_)=>{return feedback(seed,flag)},
        .ok(owner)=>{return owner.text()}
    }
}
fn create<life a>(seed:&'a [u8],flag:bool)->DynCreateError!OwnedDyn<dyn Text<'a>,Context<'a>>{
    let cb=(ignored:&[u8],pick:bool)=>{if pick{return "abc"};return "abc"};
    let text=cb(seed,flag);
    return owned_dyn::<dyn Text<'a>,Context<'a>>(Provider::<'a>{text:text});
}
fn main()->i32{let text=feedback("xyz",true);return i32(text[0])-97}
