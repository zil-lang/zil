trait Read{fn read(self:&Self)->i32;}
struct A{n:i32}
struct B{n:i32}
impl Read for A{fn read(self:&Self)->i32{return self.n}}
impl Read for B{fn read(self:&Self)->i32{return self.n}}
fn read_it(x:&dyn Read)->i32{return x.read()}
fn spare(x:&A)->i32{return 9}
fn select(which:bool)->i32{
    let a=A{n:20};let b=B{n:22};
    if which {return read_it(&a)} else {return read_it(&b)}
}
extern "C" fn getchar()->i32;
fn main()->i32{var value:i32=0;unsafe{value=getchar();}return select(value==49)}
