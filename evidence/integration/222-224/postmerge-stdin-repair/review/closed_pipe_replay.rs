//! Independent stdio control; this is not a ZIL compiler or a CI rerun.
//! The forced-old mode fixes the child-first schedule with wait(), before the
//! old unconditional write. No stdout/stderr collection happens before write.
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().expect("forced-old or fixed-none");
    let executable = PathBuf::from(args.next().expect("actual entry_error executable"));
    let out = PathBuf::from(args.next().expect("control evidence directory"));
    assert!(args.next().is_none());
    assert!(matches!(mode.as_str(), "forced-old" | "fixed-none"));
    fs::create_dir_all(&out).unwrap();
    let mut child = Command::new(executable)
        .stdin(if mode == "forced-old" {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if mode == "forced-old" {
        let mut stdin = child.stdin.take().unwrap();
        // Deliberately force the schedule admitted by the old driver. This is
        // a separate counterexample, not an assertion about the old CI stdout.
        assert_eq!(child.wait().unwrap().code(), Some(1));
        stdin
            .write_all(b"0")
            .expect("strict stdin write before output collection");
    }
    let output = child.wait_with_output().unwrap();
    fs::write(out.join("collected.stdout"), &output.stdout).unwrap();
    fs::write(out.join("collected.stderr"), &output.stderr).unwrap();
    fs::write(
        out.join("output-collected.json"),
        format!("{{\"exit\":{}}}\n", output.status.code().unwrap()),
    )
    .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"ABCDEF");
    assert!(output.stderr.is_empty());
}
