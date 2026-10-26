use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn run_and_json_run_forward_real_stdin_to_the_native_program() {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/stream/main.zil");
    for json in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_zil"));
        command.arg("run").arg(&app);
        if json {
            command.arg("--json");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"1 broken 2 -4 +5\n")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("zil run timed out (json={json})");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "json={json}: {output:?}");
        assert!(output.stderr.is_empty(), "json={json}: {output:?}");
        if json {
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.contains("\"status\":\"Success\""), "{text}");
            assert!(text.contains("\"exit_code\":0"), "{text}");
            assert!(text.contains("\"stdout\":\"4 1 23\\n\""), "{text}");
        } else {
            assert_eq!(output.stdout, b"4 1 23\n");
        }
    }
}
