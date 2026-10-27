resource R{n:i32}impl R(self:Self){fn read()->i32=>.n}
fn select(a:R,b:R,flag:bool)->i32{let chosen=if flag a else b;return chosen.read()}
fn make()->impl Fn(bool)->i32{let a=R{n:20};let b=R{n:22};return move(flag:bool)=>select(a,b,flag)}
fn main()->i32{let f=make();return f(true)+f(false)-42}
