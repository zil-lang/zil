
extern "C" fn getchar()->i32;
extern "C" fn putchar(n:i32)->i32;
fn emit(n:i32){unsafe{putchar(n)}}
resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}
fn run<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32{
 let root=R{code:82};var total=0;
 for outer in 0..n{
  let outer_guard=R{code:79+outer};var inner=0;
  while inner<3{
   let inner_guard=R{code:97+inner};inner+=1;
   match (outer==1,inner==2){
    (true,true)=>{return total+f(outer*10+inner)},
    (false,true)=>{break},
    (_,false)=>{if inner==1{continue}}
   }
   total+=f(inner)
  }
  total+=f(9);
  if outer==0{continue}
  emit(88)
 }
 return total
}
fn main()->i32{
 var n=0;unsafe{n=getchar()-48};var calls=0;
 var f=move(value:i32)=>{calls+=1;emit(48+calls);return value+calls};
 return run(&mut f,n)
}
