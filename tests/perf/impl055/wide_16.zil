resource Token { value:i32 }
fn read(t:Token)->i32 {return t.value}

fn main()->i32 {
let t0=Token{value:0};
let t1=Token{value:1};
let t2=Token{value:2};
let t3=Token{value:3};
let t4=Token{value:4};
let t5=Token{value:5};
let t6=Token{value:6};
let t7=Token{value:7};
let t8=Token{value:8};
let t9=Token{value:9};
let t10=Token{value:10};
let t11=Token{value:11};
let t12=Token{value:12};
let t13=Token{value:13};
let t14=Token{value:14};
let t15=Token{value:15};
let read_all=()=>t0.value+t1.value+t2.value+t3.value+t4.value+t5.value+t6.value+t7.value+t8.value+t9.value+t10.value+t11.value+t12.value+t13.value+t14.value+t15.value;
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
