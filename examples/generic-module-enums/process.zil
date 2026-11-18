import model as m

pub fn convert(value: m.MaybeRow) -> m.Score {
    if let m.LegacyOption.Some(row) = value { return m.ok(row.value + row.weight) }
    return m.Result.Err(true)
}
