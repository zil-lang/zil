trait Read<T:Copy>{fn read(self:&Self)->T;}
struct Meter{n:i32}
impl Read<i32> for Meter{fn read(self:&Self)->i32{return self.n}}
impl Read<bool> for Meter{fn read(self:&Self)->bool{return true}}
fn peek(x:&dyn Read<i32>)->i32{return x.read()}
fn flag(x:&dyn Read<bool>)->bool{return x.read()}
fn main(){}
