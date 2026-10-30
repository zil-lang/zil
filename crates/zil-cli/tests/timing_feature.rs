#![cfg(not(feature = "timings"))]
use std::process::Command;

#[test]
fn requested_uncompiled_timings_is_an_explicit_argument_error() {
    for json in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_zil"));
        command.args(["check", "not-read-for-unavailable-timings.zil", "--timings"]);
        if json {
            command.arg("--json");
        }
        let result = command.output().unwrap();
        assert_eq!(result.status.code(), Some(64));
        let output = String::from_utf8(if json { result.stdout } else { result.stderr }).unwrap();
        assert!(output.contains("--features timings"));
        if json {
            assert!(output.contains("E-CLI-ARGUMENT"));
        }
    }
}
