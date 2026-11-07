trait Read{fn read(self:&Self)->i32;}
struct A{n:i32}
struct B{n:i32}
impl Read for A{fn read(self:&Self)->i32{return self.n}}
impl Read for B{fn read(self:&Self)->i32{return self.n}}
fn read_it(x:&dyn Read)->i32{return x.read()}
fn main()->i32{let a=A{n:20};let b=B{n:22};return read_it(&a)+read_it(&b)}
