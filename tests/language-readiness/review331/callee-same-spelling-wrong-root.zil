fn keep<T:Copy,life a,life b>(first:&'a T,second:&'b T)->&'a T{return first}
fn wrong<U:Copy,life a,life b>(first:&'a U,second:&'b U)->&'a U{return keep(second,first)}
fn main()->i32{return 0}
