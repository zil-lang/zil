trait Base{fn read(self:&Self)->i32{return 3}}
trait Child:Base{fn child(self:&Self)->i32;}
struct Meter{n:i32}
impl Base for Meter{}
impl Child for Meter{fn child(self:&Self)->i32{return self.n}}
fn peek(x:&dyn Child)->i32{return x.child()}
fn main(){}
