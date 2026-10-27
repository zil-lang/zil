import model;resource R{n:i32}package P{manages r:R}impl P{fn finish(self:Self)->i32=>.r.n+100}
fn main()->i32{let a=P{r:R{n:1}};let b=model.make();return a.finish()+b.finish()-143}
