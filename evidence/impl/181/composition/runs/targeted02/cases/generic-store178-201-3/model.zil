
extern "C" fn putchar(x:i32)->i32;
pub resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
pub package Item{manages r:R;pub n:i32}
pub package Other{manages r:R;pub wide:i64}
pub fn item(n:i32)->Item=>Item{r:R{code:n},n:n}
pub fn other(n:i32)->Other=>Other{r:R{code:n},wide:i64(n)}
