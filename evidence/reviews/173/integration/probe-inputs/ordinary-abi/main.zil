import model
import relay
import callbacks
fn main()->void!i32 {
    var callback=callbacks.keep()
    let error=model.failed()
    relay.error_round(error)
    let item=relay.success_round(model.ready())?
    if callback()!=1 || callback()!=2 {return ()}
    return model.consume(item)-66
}
