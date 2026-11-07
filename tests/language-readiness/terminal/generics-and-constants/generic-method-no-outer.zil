trait Echo{
    fn echo<T:Copy>(self:&Self,value:T)->T{
        let a=1;let b=2;let c=3;let d=4;let e=5;
        let f=6;let g=7;let h=8;let i=9;let j=10;
        return value
    }
}
struct P{}
impl Echo for P{}
fn twice<V:Echo>(p:&V)->i32{return p.echo(7)+p.echo(11)}
fn main()->i32{let p=P{};return twice(&p)-18}
