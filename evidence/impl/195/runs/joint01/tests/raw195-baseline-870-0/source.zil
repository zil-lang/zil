
resource R{code:i32}impl Drop for R{fn drop(self:Self){}}
fn run<F:FnMut(&mut i32)->i32>(f:&mut F,value:&mut i32,n:i32)->i32{
 let outer=R{code:90};var total=0;
 for index in 0..n{
  let local=R{code:index};
  if index==0{continue}
  total+=f(value);
  if index==2{break}
 }
 return total
}
fn main()->i32{var calls=0;var f=move(p:&mut i32)=>{calls+=1;*p+=1;return calls};var value=0;return run(&mut f,&mut value,7)-3+value-2}
