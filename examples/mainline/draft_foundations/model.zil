import api;pub struct Row{pub value:i32}impl api.Factory<i32> for Row{fn make(value:i32)->Self{return Row{value:value}}}
