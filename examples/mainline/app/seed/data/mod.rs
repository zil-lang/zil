#[derive(Clone, Copy)]
pub struct Dataset {
    pub values: [i64; 8],
    pub checksum: i64,
}

pub const fn build() -> Dataset {
    let mut data = Dataset {
        values: [0; 8],
        checksum: 0,
    };
    let mut i = 0;
    while i < data.values.len() {
        data.values[i] = 5_000_000_000 + i as i64 * 3;
        i += 1;
    }
    data.values[2] = -7;
    data.values[6] = -3;
    i = 0;
    while i < data.values.len() {
        data.checksum += data.values[i];
        i += 1;
    }
    data
}

pub const fn weights() -> [i64; 2] {
    [2, 3]
}

pub fn checksum(data: &Dataset) -> &i64 {
    &data.checksum
}
