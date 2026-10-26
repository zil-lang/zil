use super::{options, record, request, source};
use std::{fs, path::Path};
use zil_base::{FileId, Fingerprint, ModuleId, ResultCategory};
use zil_driver::{CompilationSession, CompileOptions, CompileReport};

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| e.to_string())
}

fn path_request(
    out: &Path,
    common: &Path,
    records: &mut Vec<String>,
    session: &mut CompilationSession,
    name: &str,
    mut config: CompileOptions,
    expected: ResultCategory,
) -> Result<CompileReport, String> {
    config.emit_dir = Some(out.join(format!("{name}-dumps")));
    let report = session.compile_path(&common.join("main.zil"), &config);
    let snapshot = out.join(format!("{name}-sources"));
    fs::create_dir(&snapshot).map_err(|e| e.to_string())?;
    for path in ["main.zil", "a.zil", "b.zil", "c.zil", "a/mod.zil"] {
        let input = common.join(path);
        if input.is_file() {
            let target = snapshot.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&input, target).map_err(|e| e.to_string())?;
        }
    }
    record(out, records, name, session, report.clone(), expected)?;
    Ok(report)
}

pub fn run(out: &Path, common: &Path, records: &mut Vec<String>) -> Result<(), String> {
    let mut session = CompilationSession::with_capacity(4096);
    let initial = source("fn constant()->i32=>2\nfn main()->i32=>#(constant())\n");
    request(
        out,
        records,
        "extra-ct-cold",
        &mut session,
        &initial,
        options(),
        ResultCategory::Success,
    )?;
    let edited = source("fn constant()->i32=>3\nfn main()->i32=>#(constant())\n");
    request(
        out,
        records,
        "extra-ct-edit",
        &mut session,
        &edited,
        options(),
        ResultCategory::Success,
    )?;
    let identities = [
        ("extra-file-id", FileId(27), ModuleId(0), "089-guards.zil"),
        ("extra-module-id", FileId(0), ModuleId(17), "089-guards.zil"),
        (
            "extra-path",
            FileId(0),
            ModuleId(0),
            "another-089-guards.zil",
        ),
    ];
    for (name, file, module, path) in identities {
        let mut input = edited.clone();
        input.file_id = file;
        input.module_id = module;
        input.path = path.into();
        request(
            out,
            records,
            name,
            &mut session,
            &input,
            options(),
            ResultCategory::Success,
        )?;
    }
    request(
        out,
        records,
        "extra-comment-newline",
        &mut session,
        &source("/* first\n second */\nfn constant()->i32=>3\nfn main()->i32=>#(constant())\n"),
        options(),
        ResultCategory::Success,
    )?;
    let mut changed_build = options();
    changed_build.build_inputs = Fingerprint::of(b"089-changed-registry-input");
    request(
        out,
        records,
        "extra-build-input",
        &mut session,
        &edited,
        changed_build,
        ResultCategory::Success,
    )?;

    fs::create_dir(common).map_err(|e| e.to_string())?;
    let main = "import a\nimport b\nfn main()->i32=>a.value()+b.value()-3\n";
    write(&common.join("main.zil"), main)?;
    write(&common.join("a.zil"), "pub fn value()->i32=>1\n")?;
    write(&common.join("b.zil"), "pub fn value()->i32=>2\n")?;
    let mut paths = CompilationSession::with_capacity(4096);
    path_request(
        out,
        common,
        records,
        &mut paths,
        "path-cold",
        options(),
        ResultCategory::Success,
    )?;
    path_request(
        out,
        common,
        records,
        &mut paths,
        "path-hot",
        options(),
        ResultCategory::Success,
    )?;
    for field in ["tokens", "nodes", "bytes", "depth"] {
        let mut low = 0usize;
        let mut high = 4096usize;
        let mut attempt = 0;
        while high - low > 1 {
            let n = low + (high - low) / 2;
            let mut config = options();
            match field {
                "tokens" => config.limits.max_tokens = n,
                "nodes" => config.limits.max_nodes = n,
                "bytes" => config.limits.max_source_bytes = n,
                _ => config.limits.max_call_depth = n,
            }
            let mut fresh = CompilationSession::default();
            let result = fresh.compile_path(&common.join("main.zil"), &config);
            match result.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n,
                _ => return Err(format!("unexpected {field} boundary: {}", result.to_json())),
            }
            let expected = result.status;
            record(
                out,
                records,
                &format!("path-search-{field}-{attempt}-{n}"),
                &fresh,
                result,
                expected,
            )?;
            attempt += 1;
        }
        for (suffix, n, expected) in [
            ("n", high, ResultCategory::Success),
            ("n-1", high - 1, ResultCategory::ResourceLimit),
            ("recovery", high, ResultCategory::Success),
        ] {
            let mut config = options();
            match field {
                "tokens" => config.limits.max_tokens = n,
                "nodes" => config.limits.max_nodes = n,
                "bytes" => config.limits.max_source_bytes = n,
                _ => config.limits.max_call_depth = n,
            }
            path_request(
                out,
                common,
                records,
                &mut paths,
                &format!("path-{field}-{suffix}"),
                config,
                expected,
            )?;
        }
    }
    write(&common.join("main.zil"), &format!("import c\n{main}"))?;
    write(&common.join("c.zil"), "pub fn value()->i32=>19\n")?;
    path_request(
        out,
        common,
        records,
        &mut paths,
        "path-id-reordered",
        options(),
        ResultCategory::Success,
    )?;
    write(&common.join("main.zil"), main)?;
    fs::remove_file(common.join("a.zil")).map_err(|e| e.to_string())?;
    path_request(
        out,
        common,
        records,
        &mut paths,
        "path-missing-import",
        options(),
        ResultCategory::SafeReject,
    )?;
    write(&common.join("a.zil"), "pub fn value()->i32=>1\n")?;
    fs::create_dir(common.join("a")).map_err(|e| e.to_string())?;
    write(&common.join("a/mod.zil"), "pub fn value()->i32=>1\n")?;
    path_request(
        out,
        common,
        records,
        &mut paths,
        "path-ambiguous-import",
        options(),
        ResultCategory::SafeReject,
    )?;
    fs::remove_dir_all(common.join("a")).map_err(|e| e.to_string())?;
    path_request(
        out,
        common,
        records,
        &mut paths,
        "path-import-recovery",
        options(),
        ResultCategory::Success,
    )?;
    let mut collision = options();
    collision.emit_dir = Some(common.to_path_buf());
    // ArtifactPlan treats an existing dependency path as protected output.
    collision.output = Some(common.join("main.zil"));
    collision.stop_after = zil_driver::Stage::Build;
    let result = paths.compile_path(&common.join("main.zil"), &collision);
    if !result
        .diagnostics
        .iter()
        .any(|d| d.code == "E-OUTPUT-SOURCE")
    {
        return Err("output conflict did not retain E-OUTPUT-SOURCE".into());
    }
    record(
        out,
        records,
        "path-output-collision",
        &paths,
        result,
        ResultCategory::InfraError,
    )?;
    let config = options();
    config.cancellation.cancel();
    path_request(
        out,
        common,
        records,
        &mut paths,
        "path-cancelled-hot",
        config,
        ResultCategory::Cancelled,
    )?;
    path_request(
        out,
        common,
        records,
        &mut paths,
        "path-cancel-recovery",
        options(),
        ResultCategory::Success,
    )?;
    fs::remove_dir_all(common).map_err(|e| e.to_string())?;
    Ok(())
}
