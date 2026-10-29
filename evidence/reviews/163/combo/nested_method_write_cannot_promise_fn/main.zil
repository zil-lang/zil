resource R{n:i32}impl R(self:Self){fn add(){.n+=1}}
fn make()->impl Fn()->i32{let r=R{n:0};return move()=>{let inner=()=>r.add();inner();return r.n}}fn main(){}
