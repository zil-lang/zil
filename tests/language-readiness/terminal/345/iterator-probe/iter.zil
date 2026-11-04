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
