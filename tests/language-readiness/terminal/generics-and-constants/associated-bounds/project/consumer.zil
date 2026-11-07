import api

pub fn relay<I:api.Read<i32>>(value:&I)->Option<I.Item>{
    return Option::<I.Item>.Some(value.read())
}
