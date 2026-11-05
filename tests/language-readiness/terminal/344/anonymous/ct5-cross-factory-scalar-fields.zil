struct Bundle<life a,T:Copy,#N:i32>{flag:bool;small:u8;wide:u16;int:i32;signed:i64;count:u64;index:usize;narrow:f32;large:f64;unit:()}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Bundle<'a,T,N>{return Bundle::<'a,T,N>{flag:true,small:9u8,wide:300u16,int:N,signed:42i64,count:43u64,index:44usize,narrow:4.5f32,large:1.5f64,unit:()}}
fn forward<life a,#N:i32,T:Copy>(marker:Type)->Bundle<'a,T,N>{return make::<T,N,'a>(marker)}
fn result<life s>(p:&'s i32)->i32{let value:Bundle<'s,bool,42>=#(forward::<'s,42,bool>(bool));if !value.flag{return 1}if value.small!=9u8{return 2}if value.wide!=300u16{return 3}if value.int!=42{return 4}if value.signed!=42i64{return 5}if value.count!=43u64{return 6}if value.index!=44usize{return 7}if value.narrow!=4.5f32{return 8}if value.large!=1.5f64{return 9}return value.int}
fn main()->i32{let n=0;return result(&n)}
