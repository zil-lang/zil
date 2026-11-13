struct Context<life a>{text:&'a [u8]}
struct Provider<life a>{text:&'a [u8]}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return i32(self.text[0])}}
fn create<life a>()->DynCreateError!OwnedDyn<dyn Read,Context<'a>>{
    return owned_dyn::<dyn Read,Context<'a>>(Provider::<'a>{text:"abc"})
}
fn main()->i32{match create(){.err(_)=>{return 99},.ok(owner)=>{return owner.read()-97}}}
