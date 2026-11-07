fn ready<life a>(pointer:&'a i32)->Poll<&'a i32>{return Poll::<&'a i32>.Ready(pointer)}
fn pending<life a>(pointer:&'a i32)->Poll<&'a i32>{return Poll::<&'a i32>.Pending()}
fn select<life a>(first:&'a i32,second:&'a i32)->Poll<&'a i32>{return ready(first)}
fn relay<life a>(value:Poll<&'a i32>)->Poll<&'a i32>{return value}
fn read<life a>(value:Poll<&'a i32>)->i32{
 let local=relay(value)
 match local{
  Poll.Pending()=>{return 3},
  Poll.Ready(pointer)=>{return *pointer}
 }
}
fn main()->i32{
 let number=39
 var other=1
 let empty=pending(&other)
 other=9
 let selected=select(&number,&other)
 other=11
 return read(empty)+read(selected)+other-53
}
