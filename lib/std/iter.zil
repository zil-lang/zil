import std.store

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
    type Item;
    fn next<life a>(self:&'a mut Self)->Option<&'a Self.Item>;
}

// Copy the yielded item before releasing the receiver loan.

pub fn next_copy<I:LendingIterator>(cursor:&mut I)->Option<I.Item> where I.Item:Copy {
    match cursor.next(){
        Option.Some(value)=>{return Option::<I.Item>.Some(*value)},
        Option.None()=>{return Option::<I.Item>.None()}
    }
}

// Owning iteration. An error means exhaustion; obtaining the next element
// transfers its complete ownership from the iterator to the caller.
pub trait Iterator {
    type Item:StoreElement+SyncDrop;
    fn next(self:&mut Self)->void!Self.Item;
}

// Ordinary callback protocols keep both their input and output identities.
pub trait Mapper<T:StoreElement+SyncDrop> {
    type Output:StoreElement+SyncDrop;
    fn map(self:&mut Self,value:T)->Self.Output;
}
pub trait Predicate<T:StoreElement+SyncDrop> {
    fn keep(self:&mut Self,value:&T)->bool;
}

// Drain visits the last stored element first and never allocates on next.
pub package Drain<T:StoreElement+SyncDrop> { manages items:Store<T> }
pub fn drain<T:StoreElement+SyncDrop>(give items:Store<T>)->Drain<T> {
    return Drain::<T>{items:give items}
}
impl<T:StoreElement+SyncDrop> Iterator for Drain<T> {
    type Item:StoreElement=T;
    fn next(self:&mut Self)->void!T { return store_pop(self.items) }
}

// Prepare the reverse stack once, so its subsequent owning traversal retains
// the source insertion order. Preparation failure drops all acquired owners.
pub fn into_iter<T:StoreElement+SyncDrop>(give items:Store<T>)->void!Drain<T> {
    let reversed:Store<T>=store_new()
    if !store_reserve(reversed,store_len(items)) {return ()}
    while store_len(items)>0 {
        let value=store_pop(items)?
        store_push(reversed,value)
    }
    return Drain::<T>{items:give reversed}
}

// Adapters are lazy, own their complete source and callback, and can themselves
// be used as the source of another adapter without an intermediate collection.
pub package Map<I:Iterator+Value+SyncDrop,M:Mapper<I.Item>+Value+SyncDrop> { input:I; callback:M }
pub fn map<I:Iterator+Value+SyncDrop,M:Mapper<I.Item>+Value+SyncDrop>(input:I,callback:M)->Map<I,M> {
    return Map::<I,M>{input:input,callback:callback}
}
impl<I:Iterator+Value+SyncDrop,M:Mapper<I.Item>+Value+SyncDrop> Iterator for Map<I,M> {
    type Item:StoreElement=M.Output;
    fn next(self:&mut Self)->void!M.Output {
        let value=self.input.next()?
        return self.callback.map(value)
    }
}

pub package Filter<I:Iterator+Value+SyncDrop,P:Predicate<I.Item>+Value+SyncDrop> { input:I; predicate:P }
pub fn filter<I:Iterator+Value+SyncDrop,P:Predicate<I.Item>+Value+SyncDrop>(input:I,predicate:P)->Filter<I,P> {
    return Filter::<I,P>{input:input,predicate:predicate}
}
impl<I:Iterator+Value+SyncDrop,P:Predicate<I.Item>+Value+SyncDrop> Iterator for Filter<I,P> {
    type Item:StoreElement=I.Item;
    fn next(self:&mut Self)->void!I.Item {
        while true {
            let value=self.input.next()?
            if self.predicate.keep(&value) {return value}
        }
    }
}

pub fn next<I:Iterator>(cursor:&mut I)->void!I.Item { return cursor.next() }

pub package Collected<T:StoreElement+SyncDrop> { manages items:Store<T> }
// A failed allocation drops the yielded element, the partial result and the
// remaining source exactly once. Success transfers the complete result.
pub fn collect<I:Iterator+Value+SyncDrop>(input:I)->void!Collected<I.Item> {
    var cursor=input
    let items:Store<I.Item>=store_new()
    while true {
        match cursor.next() {
            .ok(value)=>{
                match store.push_growing(items,value) {
                    .ok(stored)=>{},
                    .err(rejected)=>{return ()}
                }
            },
            .err(exhausted)=>{return Collected::<I.Item>{items:give items}}
        }
    }
}

pub fn for_each_owned<I:Iterator+Value,F:FnMut(I.Item)->void>(cursor:&mut I,callback:&mut F) {
    while true {
        match cursor.next() {
            .ok(value)=>{callback(value)},
            .err(exhausted)=>{return}
        }
    }
}

pub fn for_each_lent<I:LendingIterator+Value,F:for<life a> FnMut(&'a I.Item)->void>(cursor:&mut I,callback:&mut F) {
    while true {
        match cursor.next() {
            Option.Some(value)=>{callback(value)},
            Option.None()=>{return}
        }
    }
}

// Transfer the collected storage to its next owning consumer.
pub fn into_store<T:StoreElement+SyncDrop>(values:Collected<T>)->Store<T> contract {result(give)} {return give values.items}

// The accumulator may itself own storage. Each callback consumes its previous
// accumulator and the yielded element, then returns the next complete owner.
pub fn fold_owned<I:Iterator+Value+SyncDrop,A:Value+SyncDrop,F:FnMut(A,I.Item)->A>(input:I,initial:A,callback:&mut F)->A {
    var cursor=input
    var result=initial
    while true {
        match cursor.next() {
            .ok(value)=>{result=callback(result,value)},
            .err(exhausted)=>{return result}
        }
    }
}
