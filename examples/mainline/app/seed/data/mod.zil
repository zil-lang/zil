pub struct Dataset {
    pub values: [i64; 8]
    pub checksum: i64
}

pub fn build() -> Dataset {
    var data = Dataset { values: [0, 0, 0, 0, 0, 0, 0, 0], checksum: 0 }
    let end: i64 = 8
    for i in 0..end {
        data.values[i] = 5000000000 + i * 3
    }
    data.values[2] = -7
    data.values[6] = -3
    for i in 0..end {
        data.checksum += data.values[i]
    }
    return data
}

pub fn weights() -> [i64; 2] => [2, 3]

pub fn checksum(data: &Dataset) -> &i64 => &data.checksum
