from pathlib import Path
import hashlib
p=Path('crates/zil-cli/src/main.rs');s=p.read_text();raw=s.encode()
assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()=='bfeb75a5fd80d54e154a8f984805cdd5a14caede'
def rep(a,b):
 global s
 assert s.count(a)==1,(a,s.count(a))
 s=s.replace(a,b,1)
rep('mod qc;','mod qc;\nmod resident;')
rep('Options:\\n  --cache-dir', 'Options:\\n  --resident               Explicit JSON stdio session for check/build; compile/clear/stats/quit\\n  --resident-capacity N    Raw query entries, 8..256 (default: 32; not a byte bound)\\n  --resident-reset-after N Drop all retained state every N requests, 1..4096 (default: 64)\\n  --cache-dir')
rep('    let mut input = None;','    let mut resident_mode = false;\n    let mut resident_config = resident::Config::default();\n    let mut resident_tuned = false;\n    let mut input = None;')
rep('            "--json" => {}','''            "--json" => {}
            "--resident" => resident_mode = true,
            "--resident-capacity" | "--resident-reset-after" => {
                let flag = args[index].as_str();
                if index + 1 == args.len() { return usage("resident option requires a value", json); }
                index += 1;
                let Ok(value) = args[index].parse::<usize>() else { return usage("invalid resident capacity/reset count", json); };
                resident_tuned = true;
                if flag == "--resident-capacity" {
                    if !(8..=256).contains(&value) { return usage("resident capacity must be 8..256 entries", json); }
                    resident_config.capacity = value;
                } else {
                    if !(1..=4096).contains(&value) { return usage("resident reset count must be 1..4096", json); }
                    resident_config.reset_after = value;
                }
            }''')
rep('''                    "--syntax-lock" => {
                        let contents''','''                    "--syntax-lock" => {
                        resident_config.syntax_lock = Some(PathBuf::from(value));
                        let contents''')
rep('''                        options.cancellation =
                            CancellationToken::with_timeout(Duration::from_millis(millis));''','''                        resident_config.timeout = Some(Duration::from_millis(millis));
                        options.cancellation =
                            CancellationToken::with_timeout(Duration::from_millis(millis));''')
rep('''    if options.incremental_o2 &&''','''    if resident_mode && (!matches!(action.as_str(), "check" | "build") || !json) {
        return usage("--resident requires check/build and --json", json);
    }
    if resident_tuned && !resident_mode {
        return usage("resident capacity/reset options require --resident", json);
    }
    if options.incremental_o2 &&''')
rep('''    let input = if action == "test"
''','''    if resident_mode {
        return resident::run(&input, &options, resident_config);
    }
    let input = if action == "test"
''')
p.write_text(s)
print('Integrated explicit check/build resident mode; standard option policy retained')
