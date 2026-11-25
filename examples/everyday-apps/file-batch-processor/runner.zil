// Concurrent multi-worker directory batch runner with atomic temp-file writes.
import std.fs
import std.int
import std.io
import std.path
import std.store
import transform

pub enum BatchError {
    MissingInputDir,
    InvalidOutputDir,
    ReadDirFailed,
    ProcessFailed,
    WriteFailed,
    ThreadFailed,
    Allocation
}

pub package FileReport {
    manages name: ByteBuffer;
    pub stats: transform.FileStats
}

package SharedBatch {
    manages input_dir: ByteBuffer;
    manages output_dir: ByteBuffer;
    manages files: Store<Managed<ByteBuffer>>;
    pub next_index: i64;
    manages reports: Store<FileReport>;
    pub failed_code: i32
}

package WorkItem {
    pub has_work: bool;
    manages input_dir: ByteBuffer;
    manages output_dir: ByteBuffer;
    manages file_name: ByteBuffer
}

fn copy_bytes(src: &[u8]) -> BatchError!ByteBuffer contract { result(create) } {
    let out = bytes_new()
    if !bytes_reserve(out, src.len()) { return BatchError.Allocation }
    var i: i64 = 0
    while i < src.len() {
        if !bytes_push(out, src[i]) { return BatchError.Allocation }
        i += 1
    }
    return give out
}

fn make_temp_name(file_name: &[u8]) -> BatchError!ByteBuffer contract { result(create) } {
    let suffix = ".tmp"
    let out = bytes_new()
    if !bytes_reserve(out, 1 + file_name.len() + suffix.len()) {
        return BatchError.Allocation
    }
    if !bytes_push(out, 46u8) { return BatchError.Allocation }
    var i: i64 = 0
    while i < file_name.len() {
        if !bytes_push(out, file_name[i]) { return BatchError.Allocation }
        i += 1
    }
    var j: i64 = 0
    while j < suffix.len() {
        if !bytes_push(out, suffix[j]) { return BatchError.Allocation }
        j += 1
    }
    return give out
}

fn process_single_file(
    input_dir: &[u8],
    output_dir: &[u8],
    file_name: &[u8]
) -> BatchError!transform.FileStats {
    var in_path = bytes_new()
    match path.join(input_dir, file_name) {
        .err(_) => { return BatchError.ProcessFailed },
        .ok(p) => { in_path = give p }
    }
    var out_path = bytes_new()
    match path.join(output_dir, file_name) {
        .err(_) => { return BatchError.WriteFailed },
        .ok(p) => { out_path = give p }
    }
    let tmp_name = make_temp_name(file_name)?
    var tmp_path = bytes_new()
    match path.join(output_dir, bytes_view(tmp_name)) {
        .err(_) => { return BatchError.WriteFailed },
        .ok(p) => { tmp_path = give p }
    }

    var raw = bytes_new()
    match fs.read_to_bytes(bytes_view(in_path)) {
        .err(_) => { return BatchError.ProcessFailed },
        .ok(b) => { raw = give b }
    }

    match transform.process_text(bytes_view(raw)) {
        .err(_) => { return BatchError.ProcessFailed },
        .ok(transformed) => {
            match fs.write_bytes(bytes_view(tmp_path), transform.content_bytes(&transformed)) {
                .err(_) => {
                    match fs.remove_file(bytes_view(tmp_path)) {
                        .ok(_) => {},
                        .err(_) => {}
                    }
                    return BatchError.WriteFailed
                },
                .ok(_) => {}
            }
            match fs.rename(bytes_view(tmp_path), bytes_view(out_path)) {
                .err(_) => {
                    match fs.remove_file(bytes_view(tmp_path)) {
                        .ok(_) => {},
                        .err(_) => {}
                    }
                    return BatchError.WriteFailed
                },
                .ok(_) => {}
            }
            return transformed.stats
        }
    }
}

fn copy_file_name_at(files: Store<Managed<ByteBuffer>>, idx: i64) -> BatchError!ByteBuffer contract { result(create) } {
    let name_ref = store_at(files, idx)
    return copy_bytes(bytes_view(*name_ref))
}

fn take_next_work(cell: Mutex<SharedBatch>) -> WorkItem {
    return mutex_with(cell, (state: &mut SharedBatch) -> WorkItem => {
        if state.failed_code != 0 || state.next_index >= store_len(state.files) {
            return WorkItem {
                has_work: false,
                input_dir: bytes_new(),
                output_dir: bytes_new(),
                file_name: bytes_new()
            }
        }
        let idx = state.next_index
        state.next_index = idx + 1
        match copy_bytes(bytes_view(state.input_dir)) {
            .err(_) => {
                state.failed_code = 3
                return WorkItem {
                    has_work: false,
                    input_dir: bytes_new(),
                    output_dir: bytes_new(),
                    file_name: bytes_new()
                }
            },
            .ok(in_copy) => {
                match copy_bytes(bytes_view(state.output_dir)) {
                    .err(_) => {
                        state.failed_code = 3
                        return WorkItem {
                            has_work: false,
                            input_dir: bytes_new(),
                            output_dir: bytes_new(),
                            file_name: bytes_new()
                        }
                    },
                    .ok(out_copy) => {
                        match copy_file_name_at(state.files, idx) {
                            .err(_) => {
                                state.failed_code = 3
                                return WorkItem {
                                    has_work: false,
                                    input_dir: bytes_new(),
                                    output_dir: bytes_new(),
                                    file_name: bytes_new()
                                }
                            },
                            .ok(name_copy) => {
                                return WorkItem {
                                    has_work: true,
                                    input_dir: give in_copy,
                                    output_dir: give out_copy,
                                    file_name: give name_copy
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

fn push_report_locked(state: &mut SharedBatch, report: FileReport) -> i32 {
    match store.push_growing(state.reports, report) {
        .err(_) => {
            state.failed_code = 3
            return 3
        },
        .ok(_) => { return 0 }
    }
}

fn record_report(cell: Mutex<SharedBatch>, report: FileReport) -> i32 {
    var guard = mutex_lock(cell)
    let state = mutex_value(&mut guard)
    let rc = push_report_locked(state, report)
    mutex_unlock(guard)
    return rc
}

fn worker(cell: Mutex<SharedBatch>) -> i32 {
    loop {
        let item = take_next_work(cell)
        if !item.has_work {
            break
        }
        match process_single_file(
            bytes_view(item.input_dir),
            bytes_view(item.output_dir),
            bytes_view(item.file_name)
        ) {
            .err(err_kind) => {
                var code: i32 = 1
                match err_kind {
                    BatchError.WriteFailed() => { code = 2 },
                    _ => { code = 1 }
                }
                let ignored = mutex_with(cell, (state: &mut SharedBatch) -> i32 => {
                    if state.failed_code == 0 {
                        state.failed_code = code
                    }
                    return 0
                })
                if ignored != 0 { return 1 }
                return code
            },
            .ok(stats) => {
                match copy_bytes(bytes_view(item.file_name)) {
                    .err(_) => { return 3 },
                    .ok(name_copy) => {
                        let report = FileReport {
                            name: give name_copy,
                            stats: stats
                        }
                        let rc = record_report(cell, report)
                        if rc != 0 { return rc }
                    }
                }
            }
        }
    }
    return 0
}

fn print_u64(val: u64) -> io.Error!void {
    let dec = int.format_u64(val)
    return io.print(&dec.bytes[0..dec.len])
}

fn print_sorted_reports(reports: Store<FileReport>) -> BatchError!void {
    let count = store_len(reports)
    let order: Store<i64> = store_new()
    var i: i64 = 0
    while i < count {
        match store.push_growing(order, i) {
            .err(_) => { return BatchError.Allocation },
            .ok(_) => {}
        }
        i += 1
    }

    // Insertion sort indices by filename for deterministic output across threads.
    var a: i64 = 1
    while a < count {
        let key_idx = *store_at(order, a)
        var b = a - 1
        while b >= 0 {
            let cur_idx = *store_at(order, b)
            let cmp = transform.bytes_cmp(
                bytes_view(store_at(reports, cur_idx).name),
                bytes_view(store_at(reports, key_idx).name)
            )
            if cmp > 0 {
                let dst = store_at_mut(order, b + 1)
                *dst = cur_idx
                b -= 1
            } else {
                break
            }
        }
        let place = store_at_mut(order, b + 1)
        *place = key_idx
        a += 1
    }

    var total_lines: i64 = 0
    var total_words: i64 = 0
    var total_redacted: i64 = 0
    var total_bytes: i64 = 0
    var pos: i64 = 0
    while pos < count {
        let idx = *store_at(order, pos)
        let rep = store_at(reports, idx)
        total_lines += rep.stats.lines
        total_words += rep.stats.words
        total_redacted += rep.stats.redacted
        total_bytes += rep.stats.output_bytes

        match io.print("FILE ") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print(bytes_view(rep.name)) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print(" lines=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print_i64(rep.stats.lines) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print(" words=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print_i64(rep.stats.words) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print(" redacted=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print_i64(rep.stats.redacted) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print(" bytes=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print_i64(rep.stats.output_bytes) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.print(" checksum=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match print_u64(rep.stats.checksum) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        match io.println("") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
        pos += 1
    }

    match io.print("TOTAL files=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    match io.print_i64(count) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    match io.print(" lines=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    match io.print_i64(total_lines) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    match io.print(" words=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    match io.print_i64(total_words) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    match io.print(" redacted=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    match io.print_i64(total_redacted) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    match io.print(" bytes=") { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    match io.println_i64(total_bytes) { .ok(_) => {}, .err(_) => { return BatchError.WriteFailed } }
    return ()
}

fn is_target_in_dir(input_dir: &[u8], name: &[u8]) -> BatchError!bool {
    if !transform.is_target_file(name) {
        return false
    }
    match path.join(input_dir, name) {
        .err(_) => { return BatchError.ReadDirFailed },
        .ok(full_path) => { return fs.is_file(bytes_view(full_path)) }
    }
}

pub fn run_batch(input_dir: &[u8], output_dir: &[u8]) -> BatchError!void {
    if !fs.is_dir(input_dir) {
        return BatchError.MissingInputDir
    }
    match fs.mkdir_all(output_dir) {
        .err(_) => { return BatchError.InvalidOutputDir },
        .ok(_) => {}
    }

    let targets: Store<Managed<ByteBuffer>> = store_new()
    match fs.read_dir(input_dir) {
        .err(_) => { return BatchError.ReadDirFailed },
        .ok(raw_entries) => {
            var idx: i64 = 0
            while idx < store_len(raw_entries) {
                let entry_ref = store_at(raw_entries, idx)
                let keep = is_target_in_dir(input_dir, bytes_view(*entry_ref))?
                if keep {
                    let copied = copy_bytes(bytes_view(*entry_ref))?
                    match store.push_growing(targets, give copied) {
                        .err(_) => { return BatchError.Allocation },
                        .ok(_) => {}
                    }
                }
                idx += 1
            }
        }
    }

    let in_copy = copy_bytes(input_dir)?
    let out_copy = copy_bytes(output_dir)?
    let reports: Store<FileReport> = store_new()
    match mutex_create(SharedBatch {
        input_dir: give in_copy,
        output_dir: give out_copy,
        files: give targets,
        next_index: 0,
        reports: give reports,
        failed_code: 0
    }) {
        .err(_) => { return BatchError.Allocation },
        .ok(cell) => {
            let (created, child_rc, parent_rc) = thread_scope(
                worker,
                cell,
                () -> i32 => worker(cell)
            )
            if !created { return BatchError.ThreadFailed }
            if child_rc != 0 || parent_rc != 0 {
                if child_rc == 2 || parent_rc == 2 { return BatchError.WriteFailed }
                return BatchError.ProcessFailed
            }

            let failed = mutex_with(cell, (state: &mut SharedBatch) -> i32 => {
                return state.failed_code
            })
            if failed != 0 {
                if failed == 2 { return BatchError.WriteFailed }
                return BatchError.ProcessFailed
            }

            var guard = mutex_lock(cell)
            let state = mutex_value(&mut guard)
            let res = print_sorted_reports(state.reports)
            mutex_unlock(guard)
            return res
        }
    }
}
