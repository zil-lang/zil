package Pair<life a,life b>{manages bytes:ByteBuffer;left:[&'a i32;2];right:[&'b i32;2]}
fn make<life a,life b>(first:&'a i32,second:&'b i32)->Pair<'b,'a>{
    var bytes=bytes_new()
    return Pair::<'b,'a>{bytes:give bytes,left:[second,second],right:[first,first]}
}
fn identity<life w,life a,life b>(input:&'w Pair<'a,'b>)->&'w Pair<'a,'b>{return input}
fn main()->i32{
    var first=7
    var second=9
    let pair=make(&first,&second)
    let window=identity(&pair)
    let pointer=window.left[1]
    first=11
    return *pointer-9
}
