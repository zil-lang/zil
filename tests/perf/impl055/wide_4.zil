resource Token { value:i32 }
fn read(t:Token)->i32 {return t.value}

fn main()->i32 {
let t0=Token{value:0};
let t1=Token{value:1};
let t2=Token{value:2};
let t3=Token{value:3};
let read_all=()=>t0.value+t1.value+t2.value+t3.value;
var n:i32=0;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
n+=1;
return read_all()+n;
}
