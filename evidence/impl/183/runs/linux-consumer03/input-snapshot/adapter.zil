import model;

pub fn buffer(give value:model.Error!ByteBuffer)->model.Error!ByteBuffer
    contract { result(give) } => value

pub fn store(give value:model.Error!Store<model.Item>)->model.Error!Store<model.Item>
    contract { result(give) } => value
