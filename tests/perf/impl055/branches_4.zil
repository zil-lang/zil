resource Token { value:i32 }
fn read(t:Token)->i32 {return t.value}

fn examine(flag:bool)->i32 {
let t=Token{value:7}; let alias=t; var n:i32=0;
if flag {n+=read(alias)+0} else {n+=0}
if flag {n+=read(alias)+1} else {n+=1}
if flag {n+=read(alias)+2} else {n+=2}
if flag {n+=read(alias)+3} else {n+=3}
return n;
}
fn main()->i32 {return examine(true)}
