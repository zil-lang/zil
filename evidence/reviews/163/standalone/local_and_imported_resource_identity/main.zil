import model;resource R{n:i32}impl R(self:Self){fn read()->i32=>.n+100;fn finish(give self:Self)->i32=>.n+100}
fn main()->i32{let a=R{n:1};let b=model.make();if a.read()!=101||b.read()!=42{return 1};return (give b).finish()-42}
