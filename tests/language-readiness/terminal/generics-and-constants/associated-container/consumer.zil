import api
import model
pub struct Holder<T:api.Read> { pub value:T.Item }
pub fn wrap<T:api.Read>(source:&T)->Holder<T>{return Holder::<T>{value:source.read()}}
pub fn take<T:api.Read>(holder:Holder<T>)->T.Item{return holder.value}
type Scalar=model.Source.Item;
pub fn direct(value:Scalar)->Scalar{return value}
type Maybe<T:api.Read>=Option<T.Item>;
pub fn maybe<T:api.Read>(source:&T)->Maybe<T>{return Option::<T.Item>.Some(source.read())}
type Bound<Read:api.Read>=Option<Read.Item>;
pub fn renamed<Read:api.Read>(source:&Read)->Bound<Read>{return Option::<Read.Item>.Some(source.read())}
