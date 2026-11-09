import protocol
pub fn duplicate<T:protocol.Duplicate>(value:&T)->T {return value.duplicate()}
pub fn try_duplicate<T:protocol.Duplicate>(value:&T,fail:bool)->protocol.Error!T {return value.try_duplicate(fail)}
