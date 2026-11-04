struct SliceIter<life s,T:Copy>{slice:&'s [T];index:i64}
fn iter<life s,T:Copy>(slice:&'s [T])->SliceIter<'s,T>{return SliceIter::<'s,T>{slice:slice,index:0}}
impl<life s,T:Copy> SliceIter<'s,T>{
    fn next(self:&mut Self)->Option<&'s T>{
        if self.index==self.slice.len(){return Option::<&'s T>.None()}
        let index=self.index;
        self.index+=1;
        return Option::<&'s T>.Some(&self.slice[index]);
    }
}
fn result()->i64{
    let values:[i64;3]=[10,20,12];
    var cursor=iter(&values[..]);
    let first=cursor.next();
    let second=cursor.next();
    let third=cursor.next();
    let last=cursor.next();
    match first{
        Option.Some(a)=>{match second{
            Option.Some(b)=>{match third{
                Option.Some(c)=>{match last{
                    Option.None()=>{return *a+*b+*c},
                    Option.Some(_)=>{return 0}
                }},
                Option.None()=>{return 0}
            }},
            Option.None()=>{return 0}
        }},
        Option.None()=>{return 0}
    }
}
