// Independent public-API controls for a4bec09fbaff29a353ad21571e183166de0968dc.
use std::{collections::BTreeMap, fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, ProjectSources, QueryDisposition, Stage};

fn equivalent(a: &CompileReport, b: &CompileReport) {
    assert_eq!(a.status, b.status, "{}\n{}", a.to_json(), b.to_json());
    assert_eq!(a.phase, b.phase);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.input_fingerprint, b.input_fingerprint);
    assert_eq!(a.source_files, b.source_files);
    assert_eq!(a.scalar_interfaces, b.scalar_interfaces);
    assert_eq!(a.verification_runs, b.verification_runs);
    assert_eq!(a.query_events.iter().map(|e|e.phase).collect::<Vec<_>>(), b.query_events.iter().map(|e|e.phase).collect::<Vec<_>>());
}
fn success(r: &CompileReport) { assert_eq!(r.status, ResultCategory::Success, "{}",r.to_json()); }
fn fresh(p: &ProjectSources, o: &CompileOptions) -> CompileReport { CompilationSession::default().compile_project(p,o) }
fn no_parse_exec(r: &CompileReport) { assert_eq!(r.query_events.iter().filter(|e| e.phase == Phase::Parse && e.disposition==QueryDisposition::Executed).count(), 0); }
fn project(dir: &PathBuf) -> ProjectSources {
    let mut paths = BTreeMap::new();
    let mut modules = Vec::new();
    for i in 0..40 {
        let path = dir.join(format!("m{i}.zil"));
        let text = if i==0 { "import m1\n#if(m1.choose(7)){fn main()->i32{return m1.value();}}else{fn main()->i32{return 2;}}".into() }
            else if i==1 { "fn leaf(n:i32)->bool{return n==7} pub fn choose(n:i32)->bool{return leaf(n)} pub fn value()->i32{let x=#(21+21);return x;}".into() }
            else { format!("pub fn unused()->i32{{return {i};}}") };
        fs::write(&path,&text).unwrap();
        modules.push(SourceUnit::new(FileId(i),ModuleId(i),path.to_string_lossy(),text));
        paths.insert(vec![format!("m{i}")],ModuleId(i));
    }
    ProjectSources{root:ModuleId(0),modules,paths}
}
fn main() {
    let dir=std::env::temp_dir().join(format!("zil-stage3-independent-{}",std::process::id()));
    fs::create_dir(&dir).unwrap();
    let p=project(&dir);
    let options=CompileOptions{stop_after:Stage::Types,..Default::default()};
    let mut session=CompilationSession::with_capacity(8);
    let cold=session.compile_project(&p,&options); success(&cold); assert_eq!(cold.verification_runs,0);
    let warm=session.compile_project(&p,&options); success(&warm); equivalent(&warm,&cold);no_parse_exec(&warm);
    let verified=session.compile_project(&p,&CompileOptions::default());success(&verified);no_parse_exec(&verified);assert_eq!(verified.verification_runs,1);
    equivalent(&verified,&fresh(&p,&CompileOptions::default()));
    println!("PASS Types-only publication -> warm Types -> current Verify; CT selection and CT expression");
    for dimension in 0..11 {
        let mut current=options.clone();
        match dimension {
            0=>current.limits.max_source_bytes+=1,
            1=>current.limits.max_tokens+=1,
            2=>current.limits.max_nodes+=1,
            3=>current.limits.max_functions+=1,
            4=>current.limits.max_blocks+=1,
            5=>current.limits.max_instructions+=1,
            6=>current.limits.max_locals+=1,
            7=>current.limits.max_values+=1,
            8=>current.limits.max_analysis_steps+=1,
            9=>current.limits.max_ct_steps+=1,
            10=>current.limits.max_call_depth+=1,
            _=>unreachable!()
        }
        let mut s=CompilationSession::with_capacity(8); success(&s.compile_project(&p,&options));
        let changed=s.compile_project(&p,&current);success(&changed);equivalent(&changed,&fresh(&p,&current));
        assert!(changed.query_events.iter().any(|e|e.phase==Phase::Types && e.disposition==QueryDisposition::Executed));
        let unchanged=s.compile_project(&p,&current);success(&unchanged);no_parse_exec(&unchanged);
        equivalent(&unchanged,&changed);
    }
    println!("PASS all 11 logical budget fields separately invalidate Types before subsequent exact reuse");
    for dimension in 0..3 {
        let set = |l:&mut Limits, n:usize| match dimension {0=>l.max_tokens=n,1=>l.max_nodes=n,2=>l.max_ct_steps=n as u64,_=>unreachable!()};
        let mut lo=0;let mut hi=1_000_000;
        while lo<hi {
            let mid=lo+(hi-lo)/2;let mut o=options.clone();set(&mut o.limits,mid);
            if fresh(&p,&o).status==ResultCategory::Success {hi=mid;}else{lo=mid+1;}
        }
        assert!(lo>0 && lo<1_000_000);
        let mut exact=options.clone();set(&mut exact.limits,lo);
        let mut s=CompilationSession::with_capacity(8);success(&s.compile_project(&p,&exact));
        let warm=s.compile_project(&p,&exact);success(&warm);no_parse_exec(&warm);equivalent(&warm,&fresh(&p,&exact));
        let mut insufficient=exact.clone();set(&mut insufficient.limits,lo-1);
        let denied=s.compile_project(&p,&insufficient);equivalent(&denied,&fresh(&p,&insufficient));
        assert_eq!(denied.status,ResultCategory::ResourceLimit);
        println!("PASS exact / N-1 dimension={dimension} N={lo} denied_phase={:?} code={}",denied.phase,denied.diagnostics[0].code);
    }
    // The source bytes and every logical key stay unchanged; only physical identity changes.
    let mut s=CompilationSession::with_capacity(8); success(&s.compile_project(&p,&options));
    fs::remove_file(dir.join("m2.zil")).unwrap();
    fs::hard_link(dir.join("m1.zil"),dir.join("m2.zil")).unwrap();
    let guarded=s.compile_project(&p,&options);equivalent(&guarded,&fresh(&p,&options));
    assert_eq!(guarded.diagnostics[0].code,"E-OUTPUT-SOURCE");assert!(guarded.query_events.is_empty());
    println!("PASS same-input warm request reruns current physical source alias guard before any frontend replay");
    fs::remove_dir_all(dir).unwrap();
}
