enum Selection<T,U>{Left(T),Right(U),Empty}
fn choose<life a,life b>(left:&'a i32,right:&'b i32,tag:i32)->Selection<&'a i32,&'b i32>{
 if tag==1{return Selection::<&'a i32,&'b i32>.Left(left)}
 if tag==2{return Selection::<&'a i32,&'b i32>.Right(right)}
 return Selection::<&'a i32,&'b i32>.Empty()
}
fn relay<life a,life b>(value:Selection<&'a i32,&'b i32>)->Selection<&'a i32,&'b i32>{return value}
fn read<life a,life b>(value:Selection<&'a i32,&'b i32>)->i32{
 let local=relay(value)
 match local{
  Selection.Empty()=>{return 3},
  Selection.Left(pointer)=>{return *pointer},
  Selection.Right(pointer)=>{return *pointer}
 }
}
fn main()->i32{
 let left=17
 let right=22
 return read(choose(&left,&right,0))+read(choose(&left,&right,1))+read(choose(&left,&right,2))-42
}
