struct Holder<life b>{pointer:&'b i32}
fn hold<life b>(pointer:&'b i32)->Holder<'b>{return Holder::<'b>{pointer:pointer}}
fn select<life h,life b>(left:&'h mut Holder<'b>,right:&'h mut Holder<'b>,choose:bool)->&'h mut Holder<'b>{
 if choose{return give left}
 return give right
}
fn make<life h,life b>(left:&'h mut Holder<'b>,right:&'h mut Holder<'b>,replacement:&'b i32,choose:bool)->impl FnOnce()->i32{
 let target=select(give left,give right,choose)
 return move()=>{let selected=give target;selected.pointer=replacement;return 0}
}
fn run(choose:bool)->i32{
 let a=10
 let b=20
 let replacement=12
 var left=hold(&a)
 var right=hold(&b)
 let f=make(&mut left,&mut right,&replacement,choose)
 f()
 let x=*left.pointer
 let y=*right.pointer
 if choose{return x+y-32}
 return x+y-22
}
fn main()->i32{return run(true)+run(false)}
