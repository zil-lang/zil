trait Read { type A: Copy; type B: Copy; }
struct P {}
impl Read for P { type A = Self.B; type B = Self.A; }
fn main() {}
