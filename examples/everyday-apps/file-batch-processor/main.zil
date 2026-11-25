// Everyday App: Concurrent file batch processor with atomic output writes.
// Usage: file-batch-processor <input_dir> <output_dir>
import runner
import std.env
import std.io

fn main() -> i32 {
    if env.arg_count() < 3 {
        match io.eprintln("usage: file-batch-processor <input_dir> <output_dir>") {
            .ok(_) => {},
            .err(_) => {}
        }
        return 2
    }
    match env.arg(1) {
        Option.None() => { return 2 },
        Option.Some(in_dir) => {
            match env.arg(2) {
                Option.None() => { return 2 },
                Option.Some(out_dir) => {
                    match runner.run_batch(bytes_view(in_dir), bytes_view(out_dir)) {
                        .ok(_) => { return 0 },
                        .err(err) => {
                            match err {
                                runner.BatchError.MissingInputDir() => {
                                    match io.eprintln("error: missing or invalid input directory") {
                                        .ok(_) => {},
                                        .err(_) => {}
                                    }
                                    return 3
                                },
                                runner.BatchError.InvalidOutputDir() => {
                                    match io.eprintln("error: cannot prepare output directory") {
                                        .ok(_) => {},
                                        .err(_) => {}
                                    }
                                    return 4
                                },
                                _ => {
                                    match io.eprintln("error: batch processing failed") {
                                        .ok(_) => {},
                                        .err(_) => {}
                                    }
                                    return 5
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
