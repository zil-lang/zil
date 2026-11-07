import api
pub struct Source { pub value:i32 }
impl api.Read for Source { type Item=i32; fn read()->Self.Item{return .value} }
pub fn make()->Source{return Source{value:42}}
