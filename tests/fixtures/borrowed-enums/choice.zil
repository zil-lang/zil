enum Choice<life a,life b>{Empty,Left(&'a i32),Right(&'b i32)}
fn choose<life a,life b>(left:&'a i32,right:&'b i32,tag:i32)->Choice<'a,'b>{
 if tag==1{return Choice::<'a,'b>.Left(left)}
 if tag==2{return Choice::<'a,'b>.Right(right)}
 return Choice::<'a,'b>.Empty()
}
fn relay<life a,life b>(value:Choice<'a,'b>)->Choice<'a,'b>{return value}
fn read<life a,life b>(value:Choice<'a,'b>)->i32{
 let local=relay(value)
 match local{
  Choice.Empty()=>{return 3},
  Choice.Left(pointer)=>{return *pointer},
  Choice.Right(pointer)=>{return *pointer}
 }
}
fn main()->i32{
 let left=17
 let right=22
 return read(choose(&left,&right,0))+read(choose(&left,&right,1))+read(choose(&left,&right,2))-42
}
