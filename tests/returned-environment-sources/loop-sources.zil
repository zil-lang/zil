
fn make(a:&i32,b:&i32,n:i32)->impl Fn()->i32{
 var pointer=a;var i=0
 while i<n{pointer=b;i+=1}
 return move()=>*pointer
}
fn main()->i32{let a=20;let b=22;let first=make(&a,&b,0);let second=make(&a,&b,1);return first()+second()-42}
