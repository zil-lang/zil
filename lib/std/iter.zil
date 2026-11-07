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

// In-place heapsort for Copy slices. Equal elements may change order.
// The caller's comparison must define a consistent strict order.
pub fn sort_unstable_by<T:Copy,F:FnMut(T,T)->bool>(values:&mut [T],less:&mut F) {
    let length=values.len()
    if length<2 { return }
    var start=length/2
    while start>0 {
        start-=1
        var root=start
        while root<=(length-2)/2 {
            var child=root*2+1
            if child+1<length && less(values[child],values[child+1]) {child+=1}
            if !less(values[root],values[child]) {break}
            let previous=values[root]
            values[root]=values[child]
            values[child]=previous
            root=child
        }
    }
    var end=length
    while end>1 {
        end-=1
        let previous=values[0]
        values[0]=values[end]
        values[end]=previous
        if end>1 {
            var root:i64=0
            while root<=(end-2)/2 {
                var child=root*2+1
                if child+1<end && less(values[child],values[child+1]) {child+=1}
                if !less(values[root],values[child]) {break}
                let parent=values[root]
                values[root]=values[child]
                values[child]=parent
                root=child
            }
        }
    }
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
