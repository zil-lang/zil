// Everyday CLI application: persistent CLI Todo manager with atomic saves.
// Usage:
//   cli-todo-app <store_file> add <title>
//   cli-todo-app <store_file> list
//   cli-todo-app <store_file> done <id>
//   cli-todo-app <store_file> remove <id>
//   cli-todo-app <store_file> stats
import std.env
import std.int
import std.io
import storage
import commands

fn require_arg(index: i64) -> storage.TodoError!Managed<ByteBuffer> {
    match env.arg(index) {
        Option.Some(v) => { return v },
        Option.None() => { return storage.TodoError.BadArgs }
    }
}

fn out_print(s: &[u8]) -> storage.TodoError!void {
    match io.print(s) {
        .ok(_) => { return () },
        .err(_) => { return storage.TodoError.Io }
    }
}

fn out_println_i64(n: i64) -> storage.TodoError!void {
    match io.println_i64(n) {
        .ok(_) => { return () },
        .err(_) => { return storage.TodoError.Io }
    }
}

fn parse_positive_id(raw: &[u8]) -> storage.TodoError!i64 {
    match int.parse_i64(raw) {
        .ok(n) => {
            if n <= 0 {
                return storage.TodoError.InvalidId
            }
            return n
        },
        .err(_) => { return storage.TodoError.InvalidId }
    }
}

fn run_cli() -> storage.TodoError!void {
    let argc = env.arg_count()
    if argc < 3 {
        return storage.TodoError.BadArgs
    }
    let db_arg = require_arg(1)?
    let cmd_arg = require_arg(2)?
    let db_path = bytes_view(db_arg)
    let cmd = bytes_view(cmd_arg)
    let byte_limit: i64 = 16777216

    if storage.bytes_eq(cmd, "add") {
        if argc != 4 {
            return storage.TodoError.BadArgs
        }
        let title_arg = require_arg(3)?
        var list = storage.load(db_path, byte_limit)?
        let id = commands.add_todo(&mut list, bytes_view(title_arg), byte_limit)?
        storage.save_atomic(db_path, &list)?
        out_print("ADDED ")?
        out_println_i64(id)?
        return ()
    }

    if storage.bytes_eq(cmd, "list") {
        if argc != 3 {
            return storage.TodoError.BadArgs
        }
        let list = storage.load(db_path, byte_limit)?
        return commands.print_list(&list)
    }

    if storage.bytes_eq(cmd, "done") {
        if argc != 4 {
            return storage.TodoError.BadArgs
        }
        let id_arg = require_arg(3)?
        let target_id = parse_positive_id(bytes_view(id_arg))?
        var list = storage.load(db_path, byte_limit)?
        commands.mark_done(&mut list, target_id)?
        storage.save_atomic(db_path, &list)?
        out_print("DONE ")?
        out_println_i64(target_id)?
        return ()
    }

    if storage.bytes_eq(cmd, "remove") {
        if argc != 4 {
            return storage.TodoError.BadArgs
        }
        let id_arg = require_arg(3)?
        let target_id = parse_positive_id(bytes_view(id_arg))?
        var list = storage.load(db_path, byte_limit)?
        commands.remove_todo(&mut list, target_id)?
        storage.save_atomic(db_path, &list)?
        out_print("REMOVED ")?
        out_println_i64(target_id)?
        return ()
    }

    if storage.bytes_eq(cmd, "stats") {
        if argc != 3 {
            return storage.TodoError.BadArgs
        }
        let list = storage.load(db_path, byte_limit)?
        return commands.print_stats(&list)
    }

    return storage.TodoError.BadArgs
}

fn main() -> i32 {
    match run_cli() {
        .ok(_) => { return 0 },
        .err(e) => {
            match e {
                storage.TodoError.BadArgs() => {
                    let _ = io.eprintln("usage: cli-todo-app <store_file> <add|list|done|remove|stats> [arg]")
                    return 2
                },
                storage.TodoError.InvalidId() => {
                    let _ = io.eprintln("error: todo ID must be a positive integer")
                    return 2
                },
                storage.TodoError.EmptyTitle() => {
                    let _ = io.eprintln("error: todo title must not be empty")
                    return 2
                },
                storage.TodoError.NotFound() => {
                    let _ = io.eprintln("error: todo ID not found")
                    return 1
                },
                storage.TodoError.CorruptStore() => {
                    let _ = io.eprintln("error: corrupt todo store file")
                    return 1
                },
                storage.TodoError.InvalidUtf8() => {
                    let _ = io.eprintln("error: invalid UTF-8 input")
                    return 1
                },
                storage.TodoError.Allocation() => {
                    let _ = io.eprintln("error: allocation failed")
                    return 1
                },
                storage.TodoError.Io() => {
                    let _ = io.eprintln("error: filesystem I/O failure")
                    return 1
                },
            }
        }
    }
}
