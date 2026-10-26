//! Private test bridge; the public Roam protocol stays in the Node adapter.
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;
use zil_base::{FileId, ModuleId, SourceUnit, json_string};
use zil_db::CancellationToken;
use zil_driver::{CompilationSession, CompileOptions, CompileReport};

struct State {
    session: CompilationSession,
    options: CompileOptions,
    root: PathBuf,
    source: String,
    cancel: String,
    disk: bool,
    fault: bool,
    last: Option<(String, CompileReport)>,
}

fn source(name: &str) -> Result<&'static str, String> {
    match name {
        "zero" => Ok("fn main() -> i32 { return 0 }"),
        "one" => Ok("fn main() -> i32 { return 1 }"),
        "ct" => Ok("fn main() -> i32 { return #(20 + 22) }"),
        "type-error" => Ok("fn main() -> bool { return 1 }"),
        "syntax-error" => Ok("fn main( -> i32 { return 1 }"),
        _ => Err("unknown source fixture".into()),
    }
}

fn compile(
    session: &mut CompilationSession,
    input: &SourceUnit,
    options: &CompileOptions,
    cancel: &str,
) -> CompileReport {
    let options = CompileOptions {
        cancellation: CancellationToken::default(),
        ..options.clone()
    };
    let token = options.cancellation.clone();
    if cancel == "before" {
        token.cancel();
    }
    session.compile_source_with_observer(input, &options, &mut |event| {
        if event.phase.as_str() == cancel {
            token.cancel();
        }
    })
}

impl State {
    fn snapshot(&self) -> String {
        format!(
            "{{\"source\":{},\"target\":{},\"cancel\":{},\"disk\":{},\"tokens\":{},\"nodes\":{},\"analysis\":{}}}",
            json_string(&self.source),
            json_string(&self.options.target.id),
            json_string(&self.cancel),
            self.disk,
            self.options.limits.max_tokens,
            self.options.limits.max_nodes,
            self.options.limits.max_analysis_steps,
        )
    }

    fn command(&mut self, line: &str) -> Result<String, String> {
        let fields: Vec<_> = line.split('\t').collect();
        match fields.as_slice() {
            ["state"] => {}
            ["edit", name] => {
                source(name)?;
                self.source = (*name).into();
            }
            ["target", name @ ("native-llvm" | "alternate-llvm")] => {
                self.options.target.id = (*name).into();
            }
            [
                "cancel",
                mode @ ("none" | "before" | "parse" | "types" | "verify"),
            ] => {
                self.cancel = (*mode).into();
            }
            ["limit", name, value] => {
                let value: usize = value.parse().map_err(|_| "invalid limit")?;
                if value > 10_000_000 {
                    return Err("limit exceeds test bridge quota".into());
                }
                match *name {
                    "tokens" => self.options.limits.max_tokens = value,
                    "nodes" => self.options.limits.max_nodes = value,
                    "analysis" => self.options.limits.max_analysis_steps = value,
                    _ => return Err("unknown limit".into()),
                }
            }
            ["cache", "on"] => self.disk = true,
            ["cache", "off"] => self.disk = false,
            ["cache", "clear"] => {
                self.session.clear();
                let directory = self.root.join("cache");
                if directory.exists() {
                    fs::remove_dir_all(directory).map_err(|error| error.to_string())?;
                }
            }
            ["cache", "corrupt"] => {
                let directory = self.root.join("cache");
                if directory.exists() {
                    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
                        let path = entry.map_err(|error| error.to_string())?.path();
                        if path.is_file() {
                            fs::write(path, b"ZILRAW01truncated")
                                .map_err(|error| error.to_string())?;
                        }
                    }
                }
            }
            ["check"] => {
                let input = SourceUnit::new(
                    FileId(0),
                    ModuleId(0),
                    self.root.join("input.zil").to_string_lossy(),
                    source(&self.source)?,
                );
                let warm_options = CompileOptions {
                    cache_dir: self.disk.then(|| self.root.join("cache")),
                    ..self.options.clone()
                };
                let stale = self
                    .last
                    .as_ref()
                    .filter(|(name, _)| self.fault && *name != self.source);
                let warm = if let Some((_, previous)) = stale {
                    previous.clone()
                } else {
                    let result = compile(&mut self.session, &input, &warm_options, &self.cancel);
                    self.last = Some((self.source.clone(), result.clone()));
                    result
                };
                let mut cold_session = CompilationSession::default();
                let cold = compile(&mut cold_session, &input, &self.options, &self.cancel);
                return Ok(format!(
                    "{{\"kind\":\"check\",\"state\":{},\"warm\":{},\"cold\":{},\"active\":{},\"cold_active\":{},\"cached\":{}}}",
                    self.snapshot(),
                    warm.to_json(),
                    cold.to_json(),
                    self.session.active_query_count(),
                    cold_session.active_query_count(),
                    self.session.cached_query_count(),
                ));
            }
            _ => return Err("invalid test bridge command".into()),
        }
        Ok(format!(
            "{{\"kind\":\"state\",\"state\":{},\"active\":{},\"cached\":{}}}",
            self.snapshot(),
            self.session.active_query_count(),
            self.session.cached_query_count(),
        ))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let root = PathBuf::from(arguments.next().ok_or("missing isolated work directory")?);
    let fault = match arguments.next().as_deref() {
        None | Some("none") => false,
        Some("stale-after-edit") => true,
        _ => return Err("unknown test fault".into()),
    };
    if arguments.next().is_some() {
        return Err("unexpected argument".into());
    }
    fs::create_dir_all(&root)?;
    let mut state = State {
        session: CompilationSession::default(),
        options: CompileOptions::default(),
        root,
        source: "zero".into(),
        cancel: "none".into(),
        disk: true,
        fault,
        last: None,
    };
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    loop {
        let mut line = String::new();
        if Read::take(&mut input, 257).read_line(&mut line)? == 0 {
            break;
        }
        if line.len() > 256 {
            return Err("test bridge command exceeds byte quota".into());
        }
        let reply = match state.command(line.trim_end_matches(['\r', '\n'])) {
            Ok(value) => value,
            Err(error) => format!("{{\"error\":{}}}", json_string(&error)),
        };
        writeln!(output, "{reply}")?;
        output.flush()?;
    }
    Ok(())
}
