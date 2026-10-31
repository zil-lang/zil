import rules;pub struct Item{n:i32}impl rules.Read for Item{fn value(self:&Self)->i32=>self.n}pub fn make(n:i32)->Item=>Item{n:n}
impl Item{fn secret(self:&Self)->i32=>.n}