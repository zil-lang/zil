import api
pub struct Maker{}
pub struct CountMaker{}
pub package Parcel{manages items:Store<i32>}
impl api.Factory for Maker{type Item=Parcel;}
impl api.Factory for CountMaker{type Item=i32;}
pub fn parcel()->Parcel{
    let items:Store<i32>=store_new()
    return Parcel{items:give items}
}
pub fn length(value:&Parcel)->i64{return store_len(value.items)}
