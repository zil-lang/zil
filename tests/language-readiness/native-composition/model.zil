import rules

pub struct MutableHolder<life a> { pointer:&'a mut i32 }
pub fn hold<life a>(pointer:&'a mut i32)->MutableHolder<'a> {
    return MutableHolder::<'a>{pointer:pointer};
}
pub fn increment<life a>(holder:&mut MutableHolder<'a>) {
    let pointer=holder.pointer;
    *pointer=*pointer+1;
}
pub fn bump(value:i32)->i32 {
    var result=value;
    if true {
        var holder=hold(&mut result);
        increment(&mut holder);
    }
    return result;
}

pub struct Number {pub value:i32}
impl rules.Read for Number {
    type Item:Copy=i32;
    fn read()->Self.Item {return .value;}
}
pub fn number(value:i32)->Number {return Number{value:value};}
