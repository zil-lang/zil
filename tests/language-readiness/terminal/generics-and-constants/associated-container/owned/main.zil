import api
import model
fn main()->i32{
    let parcel=model.parcel()
    let again=api.relay::<model.Maker>(parcel)
    let count=api.relay::<model.CountMaker>(7)
    return i32(model.length(&again))+count-7
}
