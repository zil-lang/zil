trait Read{fn read(self:&Self)->i32;}fn unused<T:Read>(x:&T)->i32=>x.read()
fn sample()->i32=>0