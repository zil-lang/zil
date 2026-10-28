fn main()->i32{let x=2;let f=()=>x;var i=0;var sum=0;while i<3{sum+=f();i+=1}return sum-6}
