trait Read{fn read(self:&Self)->i32;}
fn read_it(value:&dyn Read)->i32{return value.read()}
fn main()->i32{return 0}
