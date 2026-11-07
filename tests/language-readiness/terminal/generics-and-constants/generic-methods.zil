trait Ops<Tag:Copy>:Copy {
    type Item:Copy;
    const COUNT:usize;
    fn pick<T:Copy>(self:&Self,value:T)->T;
    fn relay<T:Copy>(self:&Self,value:T)->T{return self.pick(value)}
    fn number<#N:usize>(self:&Self)->usize{return N}
    fn count<T:Copy>(self:&Self,value:T)->usize{return Self.COUNT}
}
struct Box<T:Copy>{value:T}
impl<T:Copy> Ops<bool> for Box<T>{
    type Item=T;
    const COUNT:usize=3usize;
    fn pick<U>(value:U)->U{return value}
}
fn read<V:Ops<bool>>(p:&V)->i32{return p.relay(7)}
fn main()->i32{
    let p=Box::<bool>{value:true};
    if p.number::<9usize>()!=9usize{return 1}
    if p.count(true)!=3usize{return 2}
    return read(&p)-7
}
