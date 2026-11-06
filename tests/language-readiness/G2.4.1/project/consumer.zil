import api

pub fn next_copy<I:api.LendingIterator>(cursor:&mut I)->Option<I.Item>{
    match cursor.next(){
        Option.Some(value)=>{return Option::<I.Item>.Some(*value)},
        Option.None()=>{return Option::<I.Item>.None()}
    }
}
