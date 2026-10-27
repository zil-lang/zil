resource R{n:i32}impl R(self:Self){fn set(n:i32){.n=n}}
fn make()->impl Fn()->i32{let r=R{n:0};return move()=>{r.set(42);return r.n}}fn main(){}
