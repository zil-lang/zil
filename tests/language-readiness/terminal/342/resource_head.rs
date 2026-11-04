//! Actual Package-owned storage head projection and access-mode retention.
use super::*;
const READ: &str = "package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{return bytes_len(p.data)}fn push(p:&mut Text){bytes_reserve(p.data,1);bytes_push(p.data,42u8)}fn view(p:&Text)->&[u8]{return bytes_view(p.data)}fn result()->i64{var text=Text{data:bytes_new()};push(&mut text);let bytes=view(&text);return i64(bytes[0])+size(&text)-1}";
#[test]
fn terminal342_owned_storage_head_projects_readonly_and_exclusive_actual_modes() {
    let report = compile_source(&source(READ), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}
#[test]
fn terminal342_shared_storage_head_cannot_launder_a_write() {
    for text in [
        "package Text{manages data:ByteBuffer}fn bad(p:&Text){bytes_truncate(p.data,0)}fn main(){}",
        "package Text{manages data:ByteBuffer}fn mutate(b:ByteBuffer){bytes_truncate(b,0)}fn bad(p:&Text){mutate(p.data)}fn main(){}",
        "package Text{manages data:ByteBuffer}fn bad(p:&Text){let d=p.data;let f=()=>bytes_truncate(d,0);f()}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
    }
}
