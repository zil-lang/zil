pub trait Change {
    fn apply(self:&mut Self)->();
    fn replace(self:&mut Self,value:i32)->i32;
}
pub struct Meter { pub n:i32 }
impl Change for Meter {
    fn apply(self:&mut Self)->(){self.n=20;return ()}
    fn replace(self:&mut Self,value:i32)->i32{self.n=value;return self.n}
}
pub fn run(x:&mut dyn Change)->(){return x.apply()}
pub fn replace_it(x:&mut dyn Change,value:i32)->i32{return x.replace(value)}
