resource Token { value:i32 }
fn consume(give token:Token) {}
fn independent(b0:bool,b1:bool,b2:bool,b3:bool,b4:bool,b5:bool) {
  let r0=Token{value:0}
  let r1=Token{value:1}
  let r2=Token{value:2}
  let r3=Token{value:3}
  let r4=Token{value:4}
  let r5=Token{value:5}
  if b0 { consume(give r0) }
  if b1 { consume(give r1) }
  if b2 { consume(give r2) }
  if b3 { consume(give r3) }
  if b4 { consume(give r4) }
  if b5 { consume(give r5) }
}
fn main()->i32{independent(false,true,false,true,false,true);return 0}
