
extern "C" fn putchar(n:i32)->i32;
pub resource Mark{pub code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
pub struct Pair{pub left:i32,pub right:i32}
pub package Owner{manages marker:Mark;pub bias:i32}
pub fn owner(code:i32,bias:i32)->Owner{return Owner{marker:Mark{code:code},bias:bias}}


fn choose<life a>(left:&'a Pair,right:&'a Pair,first:bool)->&'a i32{
 if first{return &left.left}
 return &right.right
}
pub fn make<life a>(owner:Owner,left:&'a Pair,right:&'a Pair,first:bool)->impl Fn()->i32{
 let target=choose(left,right,first)
 return move()=>owner.bias+*target
}


fn main()->i32{
 let left=Pair{left:20,right:91}
 let right=Pair{left:92,right:22}
 let f=make(owner(65,1),&left,&right,true)
 let g=make(owner(66,2),&left,&right,false)
 let saved=f
 return saved()+g()-45
}
