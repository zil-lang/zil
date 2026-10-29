resource R{n:i32}package P{manages r:R}impl P{fn finish(self:Self)->i32=>.r.n}
fn make()->impl FnMut(bool)->i32{var p=P{r:R{n:42}};return move(flag:bool)=>{let inner=move()=>p.finish();let n=inner();if flag{p=P{r:R{n:n+1}}};return n}}fn main(){}
