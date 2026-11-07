trait Config<Tag:Copy>{
    const COUNT:usize;
    const FLAG:bool=true;
    fn count(self:&Self)->usize{return Self.COUNT}
}
struct Box<T:Copy>{value:T}
impl<T:Copy> Config<i32> for Box<T>{const COUNT:usize=7usize;}
impl<T:Copy> Config<bool> for Box<T>{const COUNT:usize=3usize;const FLAG:bool=false;}
fn number<T:Config<i32>>(p:&T)->usize{return T.COUNT+p.count()}
fn other<T:Config<bool>>()->usize{return T.COUNT}
fn flag<T:Config<bool>>()->bool{return T.FLAG}
fn main()->i32{
    let p=Box::<i32>{value:7};
    if flag::<Box<i32>>(){return 1}
    return i32(number(&p)+other::<Box<i32>>())-17
}
