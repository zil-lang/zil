struct Provider{code:i32}
trait View{fn view(self:&Self)->&i32;}
impl View for Provider{fn view(self:&Self)->&i32{return &self.code}}
fn inspect(owner:&dyn View)->i32{
    let value=owner.view();
    return *value-42;
}
fn main()->i32{
    let provider=Provider{code:42};
    return inspect(&provider);
}
