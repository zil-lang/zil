enum Mix<T,life b>{FromT(T),Other(&'b i32)}
fn other<life a,life b>(first:&'a i32,second:&'b i32)->Mix<&'a i32,'b>{
 return Mix::<&'a i32,'b>.Other(second)
}
fn get<life a,life b>(value:Mix<&'a i32,'b>,fallback:&'a i32)->&'a i32{
 match value{
  Mix.FromT(pointer)=>{return pointer},
  Mix.Other(pointer)=>{return fallback}
 }
}
fn main()->i32{
 let first=17
 let second=25
 return *get(other(&first,&second),&first)-17
}
