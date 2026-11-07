pub fn first<life a,T:Copy,life b>(value:&'a T,other:&'b T)->&'a T{return value}
pub fn selected_type<T:Copy>(value:T)->Type{return T}
