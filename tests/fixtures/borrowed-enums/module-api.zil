pub enum Choice<life a,life b>{Empty,Left(&'a i32),Right(&'b i32)}
pub fn select<life a,life b>(left:&'a i32,right:&'b i32)->Choice<'a,'b>{
 return Choice::<'a,'b>.Left(left)
}
pub fn relay<life a,life b>(value:Choice<'a,'b>)->Choice<'a,'b>{return value}
pub fn read<life a,life b>(value:Choice<'a,'b>)->i32{
 let local=relay(value)
 match local{
  Choice.Empty()=>{return 3},
  Choice.Left(pointer)=>{return *pointer},
  Choice.Right(pointer)=>{return *pointer}
 }
}
