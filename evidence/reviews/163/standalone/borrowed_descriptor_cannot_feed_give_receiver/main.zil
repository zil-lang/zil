resource R{n:i32}impl R{fn finish(give self:Self)->i32=>.n}
fn main()->i32{let r=R{n:42};let alias=r;return (give alias).finish()}
