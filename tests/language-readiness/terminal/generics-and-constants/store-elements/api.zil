pub package Basket<K:Copy+StoreElement,V:Copy+StoreElement>{
    manages pairs:Store<(K,V)>;
    manages copies:Store<[K;2]>;
}
pub fn push_pair<K:Copy+StoreElement,V:Copy+StoreElement>(items:Store<(K,V)>,key:K,value:V)->bool{
    match store_push(items,(key,value)){.ok(_)=>{return true},.err(_)=>{return false}}
}
pub fn push_copies<K:Copy+StoreElement>(items:Store<[K;2]>,value:K)->bool{
    match store_push(items,[value,value]){.ok(_)=>{return true},.err(_)=>{return false}}
}
pub fn first<K:Copy+StoreElement,V:Copy+StoreElement>(basket:&Basket<K,V>)->K{
    let pair=*store_at(basket.pairs,0)
    return pair.0
}
pub fn replace<K:Copy+StoreElement,V:Copy+StoreElement>(basket:&mut Basket<K,V>,value:K){
    let window=store_at_mut(basket.copies,0)
    *window=[value,value]
}
pub fn pop_pair<K:Copy+StoreElement,V:Copy+StoreElement>(basket:&mut Basket<K,V>)->Option<(K,V)>{
    match store_pop(basket.pairs){.ok(pair)=>{return Option::<(K,V)>.Some(pair)},.err(_)=>{return Option::<(K,V)>.None()}}
}
pub fn build<K:Copy+StoreElement,V:Copy+StoreElement>(give pairs:Store<(K,V)>,give copies:Store<[K;2]>)->Basket<K,V>{
    return Basket::<K,V>{pairs:give pairs,copies:give copies}
}
pub fn copy_at<K:Copy+StoreElement,V:Copy+StoreElement>(basket:&Basket<K,V>)->K{
    return (*store_at(basket.copies,0))[0]
}
