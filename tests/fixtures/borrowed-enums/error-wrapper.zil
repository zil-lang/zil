enum Problem{Missing}
enum Choice<life a>{Empty,Value(&'a i32)}
fn make<life a>(pointer:&'a i32,tag:i32)->Problem!Choice<'a>{
 if tag<0{return Problem.Missing()}
 if tag==0{return Choice::<'a>.Empty()}
 return Choice::<'a>.Value(pointer)
}
fn relay<life a>(value:Problem!Choice<'a>)->Problem!Choice<'a>{return value}
fn empty_error<life a>(pointer:&'a i32)->Problem!Choice<'a>{return Problem.Missing()}
fn read<life a>(pointer:&'a i32,tag:i32)->Problem!i32{
 let local=relay(make(pointer,tag))?
 match local{
  Choice.Empty()=>{return 3},
  Choice.Value(value)=>{return *value}
 }
}
fn main()->Problem!i32{
 let number=17
 var spare=1
 let vacant=empty_error(&spare)
 spare=9
 let answer=read(&number,1)?
 return answer+spare-26
}
