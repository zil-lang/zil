pub fn step<F:Fn()->bool!i32>(callback:&F)->bool!i32{
    return callback()
}
