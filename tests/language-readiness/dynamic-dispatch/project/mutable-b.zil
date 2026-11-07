import a;
pub struct Meter { pub n:i32 }
impl a.Change for Meter {
    fn apply(self:&mut Self)->(){self.n=7;return ()}
    fn replace(self:&mut Self,value:i32)->i32{self.n=7;return self.n}
}
