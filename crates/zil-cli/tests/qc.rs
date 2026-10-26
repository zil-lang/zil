use std::process::Command;

#[test]
fn help_and_missing_tool_are_distinct() {
    let help = Command::new(env!("CARGO_BIN_EXE_zil"))
        .args(["qc", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--stop-file"));
    let bad = Command::new(env!("CARGO_BIN_EXE_zil"))
        .args(["qc", "--tool", "/no/such/zil-quality-tool", "hunt"])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("E-QC-TOOL"));
}

#[cfg(unix)]
#[test]
fn forwards_literal_arguments_and_real_exit_status() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("zil-qc-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("worker with spaces");
    std::fs::write(&path, "#!/bin/sh\nprintf '%s\\n' \"$@\"\nexit 37\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zil"))
        .arg("qc")
        .arg("--tool")
        .arg(&path)
        .args(["hunt", "--case=a; echo UNWANTED", "--seed=3"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(37));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "hunt\n--case=a; echo UNWANTED\n--seed=3\n"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
