resource Token { value:i32 }
fn consume(give token:Token) {}
fn independent(b0:bool,b1:bool,b2:bool) {
  let r0=Token{value:0}
  let r1=Token{value:1}
  let r2=Token{value:2}
  if b0 { consume(give r0) }
  if b1 { consume(give r1) }
  if b2 { consume(give r2) }
}
fn main()->i32{independent(false,true,false);return 0}
