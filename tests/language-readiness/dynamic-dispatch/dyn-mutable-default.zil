trait Change{
    fn update(self:&mut Self)->();
    fn wrapped(self:&mut Self)->(){return self.update()}
}
struct A{n:i32}
struct B{n:i32}
impl Change for A{fn update(self:&mut Self)->(){self.n=20;return ()}}
impl Change for B{fn update(self:&mut Self)->(){self.n=7;return ()}}
fn run(x:&mut dyn Change)->(){return x.wrapped()}
extern "C" fn getchar()->i32;
fn main()->i32{
    var a=A{n:1};var b=B{n:2};var input:i32=0;
    unsafe{input=getchar();}
    if input==49 {run(&mut a)} else {run(&mut b)}
    return a.n+b.n
}
