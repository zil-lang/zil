trait Read{fn read(self:&Self)->i32;}struct Widget{n:i32}impl Read for Widget{fn read(self:&Self)->i32{return self.n}}fn peek(x:&dyn Read)->i32{return x.read()}fn main()->i32{return 0}
