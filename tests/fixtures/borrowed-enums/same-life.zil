enum Choice<life a>{Empty,Left(&'a i32),Right(&'a i32)}
fn select<life a>(left:&'a i32,right:&'a i32)->Choice<'a>{
 return Choice::<'a>.Left(left)
}
fn relay<life a>(value:Choice<'a>)->Choice<'a>{return value}
fn main()->i32{
 let left=17
 var right=2
 let local=relay(select(&left,&right))
 right=25
 match local{
  Choice.Empty()=>{return 1},
  Choice.Left(pointer)=>{return *pointer+right-42},
  Choice.Right(pointer)=>{return *pointer-42}
 }
}
