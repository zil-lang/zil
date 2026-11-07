trait Read {fn read(self:&Self)->i32;}
fn read_it(x:&dyn Read)->i32{return x.read()}
fn main()->i32{return 0}
