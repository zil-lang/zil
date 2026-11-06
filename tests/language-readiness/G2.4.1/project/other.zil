import api

pub struct Token{number:i32}
pub fn number(value:Token)->i32{return value.number}
pub struct TokenCursor{values:[Token;2];index:usize}
pub fn make()->TokenCursor{
    return TokenCursor{values:[Token{number:7},Token{number:11}],index:0usize}
}
impl api.LendingIterator for TokenCursor{
    type Item=Token;
    fn next<life q>(self:&'q mut Self)->Option<&'q Token>{
        if self.index>=2usize{return Option::<&'q Token>.None()}
        let i=self.index;
        self.index+=1usize;
        return Option::<&'q Token>.Some(&self.values[i]);
    }
}
