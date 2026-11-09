import protocol

pub fn next_tagged<life a,I:protocol.LendingIterator+protocol.Tag,T:Copy>(cursor:&'a mut I,tag:T)->Option<&'a I.Item> {
    let actual=cursor.tag()
    if actual<0 {return Option::<&'a I.Item>.None()}
    let copied=tag
    let copied_again=tag
    return cursor.next()
}
