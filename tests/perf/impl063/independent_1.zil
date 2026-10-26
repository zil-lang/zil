resource Token { value:i32 }
fn consume(give token:Token) {}
fn independent(b0:bool) {
  let r0=Token{value:0}
  if b0 { consume(give r0) }
}
fn main()->i32{independent(false);return 0}
