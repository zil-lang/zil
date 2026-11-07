trait Change{fn set(self:&mut Self,value:i32)->();fn replace(self:&mut Self,value:i32)->i32;}
struct A{n:i32}
struct B{n:i32}
impl Change for A{
    fn set(self:&mut Self,value:i32)->(){self.n=value;return ()}
    fn replace(self:&mut Self,value:i32)->i32{self.n=value;return self.n}
}
impl Change for B{
    fn set(self:&mut Self,value:i32)->(){self.n=7;return ()}
    fn replace(self:&mut Self,value:i32)->i32{self.n=7;return self.n}
}
fn apply(target:&mut dyn Change,value:i32)->(){return target.set(value)}
fn replace_it(target:&mut dyn Change,value:i32)->i32{return target.replace(value)}
fn select(which:bool)->i32{
    var a=A{n:1};
    var b=B{n:2};
    if which {
        apply(&mut a,20);
        let before=a.n;
        let after=replace_it(&mut a,30);
        return before+after+a.n+b.n
    } else {
        apply(&mut b,22);
        let before=b.n;
        let after=replace_it(&mut b,50);
        return before+after+a.n+b.n
    }
}
extern "C" fn getchar()->i32;
fn main()->i32{var value:i32=0;unsafe{value=getchar();}return select(value==49)}
