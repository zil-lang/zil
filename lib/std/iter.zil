// Synchronous Copy-element iteration; callbacks retain their actual caller effects.
pub fn for_each<T:Copy,F:FnMut(T)->void>(values:&[T],callback:&mut F) {
    var index:i64=0
    while index<values.len(){callback(values[index]);index+=1}
}
pub fn fold<T:Copy,F:FnMut(T,T)->T>(values:&[T],initial:T,callback:&mut F)->T {
    var result=initial
    var index:i64=0
    while index<values.len(){result=callback(result,values[index]);index+=1}
    return result
}

pub fn any<T:Copy,F:FnMut(T)->bool>(values:&[T],predicate:&mut F)->bool {
    var index:i64=0
    while index<values.len(){if predicate(values[index]){return true};index+=1}
    return false
}
pub fn all<T:Copy,F:FnMut(T)->bool>(values:&[T],predicate:&mut F)->bool {
    var index:i64=0
    while index<values.len(){if !predicate(values[index]){return false};index+=1}
    return true
}
pub fn position<T:Copy,F:FnMut(T)->bool>(values:&[T],predicate:&mut F)->i64? {
    var index:i64=0
    while index<values.len(){if predicate(values[index]){return Option.Some(index)};index+=1}
    return Option.None
}
pub fn find<T:Copy,F:FnMut(T)->bool>(values:&[T],predicate:&mut F)->T? {
    var index:i64=0
    while index<values.len(){let value=values[index];if predicate(value){return Option.Some(value)};index+=1}
    return Option.None
}

// Each next result borrows this invocation of the receiver until its last use.
pub trait LendingIterator {
    type Item:Copy;
    fn next<life a>(self:&'a mut Self)->Option<&'a Self.Item>;
}

// Copy the yielded item before releasing the receiver loan.

pub fn next_copy<I:LendingIterator>(cursor:&mut I)->Option<I.Item>{
    match cursor.next(){
        Option.Some(value)=>{return Option::<I.Item>.Some(*value)},
        Option.None()=>{return Option::<I.Item>.None()}
    }
}
