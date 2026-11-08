import model;
pub fn run<F:FnMut(&model.Item)->void!model.Reply>(handler:&mut F,item:&model.Item)->void!model.Reply{
    return handler(item)
}
