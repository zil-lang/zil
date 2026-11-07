struct Tag<life a>{n:i32}
type Code=for<life a> fn(&'a Tag<'a>)->i32;
struct Holder<T:Copy>{n:i32}
fn make()->Holder<Code>{return Holder::<Code>{n:42}}
fn main()->i32{let h:Holder<Code>=#(make());return h.n}
