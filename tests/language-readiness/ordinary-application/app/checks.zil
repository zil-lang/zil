import model.records
#[test]
fn complete_integer_identity()->bool {return records.echo(4294967295u32)==4294967295u32}
