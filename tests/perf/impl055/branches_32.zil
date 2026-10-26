resource Token { value:i32 }
fn read(t:Token)->i32 {return t.value}

fn examine(flag:bool)->i32 {
let t=Token{value:7}; let alias=t; var n:i32=0;
if flag {n+=read(alias)+0} else {n+=0}
if flag {n+=read(alias)+1} else {n+=1}
if flag {n+=read(alias)+2} else {n+=2}
if flag {n+=read(alias)+3} else {n+=3}
if flag {n+=read(alias)+4} else {n+=4}
if flag {n+=read(alias)+5} else {n+=5}
if flag {n+=read(alias)+6} else {n+=6}
if flag {n+=read(alias)+7} else {n+=7}
if flag {n+=read(alias)+8} else {n+=8}
if flag {n+=read(alias)+9} else {n+=9}
if flag {n+=read(alias)+10} else {n+=10}
if flag {n+=read(alias)+11} else {n+=11}
if flag {n+=read(alias)+12} else {n+=12}
if flag {n+=read(alias)+13} else {n+=13}
if flag {n+=read(alias)+14} else {n+=14}
if flag {n+=read(alias)+15} else {n+=15}
if flag {n+=read(alias)+16} else {n+=16}
if flag {n+=read(alias)+17} else {n+=17}
if flag {n+=read(alias)+18} else {n+=18}
if flag {n+=read(alias)+19} else {n+=19}
if flag {n+=read(alias)+20} else {n+=20}
if flag {n+=read(alias)+21} else {n+=21}
if flag {n+=read(alias)+22} else {n+=22}
if flag {n+=read(alias)+23} else {n+=23}
if flag {n+=read(alias)+24} else {n+=24}
if flag {n+=read(alias)+25} else {n+=25}
if flag {n+=read(alias)+26} else {n+=26}
if flag {n+=read(alias)+27} else {n+=27}
if flag {n+=read(alias)+28} else {n+=28}
if flag {n+=read(alias)+29} else {n+=29}
if flag {n+=read(alias)+30} else {n+=30}
if flag {n+=read(alias)+31} else {n+=31}
return n;
}
fn main()->i32 {return examine(true)}
