# Numeric text and buffered I/O

`std.int` parses locale-independent decimal byte slices. `parse_u64` accepts
all values through 18446744073709551615, including an optional leading plus.
An empty slice is Empty; a sign alone, minus, whitespace, NUL, separators and
nondecimal bytes are InvalidDigit. Multiplication/addition are preceded by cutoff
checks; a larger value is Overflow, never a trap or truncation. `parse_u8`,
`parse_u16`, `parse_u32` and `parse_usize` check the corresponding unsigned range
before conversion. `parse_i8`, `parse_i16`, `parse_i32` and `parse_isize` apply
the full signed i64 decimal parser, then check the destination's lower and upper
bounds before conversion; out-of-range input is Overflow. `usize` and `isize`
limits come from the selected target's pointer width, including 32-bit CT.

The matching format functions return `Decimal {bytes:[u8;20],len:i64}` with no
allocation or terminator requirement, including one byte for zero. Borrow
`&decimal.bytes[..decimal.len]` to parse or write the formatted prefix.
Existing signed i64 rules remain unchanged.

`std.float` is a Linux/glibc ordinary library adapter using fixed C signatures,
not varargs or privileged compiler conversion operations. It uses `strtod_l`
and `strtof_l` directly so parsing f32 does not introduce double rounding via
f64. Decimal syntax is validated completely before conversion; input has an
optional sign, decimal significand with at least one digit, and optional signed
base-10 exponent with at least one digit. Whitespace, hexadecimal text, embedded
NUL and trailing data reject. Canonical lowercase `inf` and `nan`, with optional
sign, are also accepted. NaN text does not preserve payload/sign bits.

Finite overflow and a nonzero significand rounded to zero report Range.
Nonzero subnormal results are accepted. The input is copied into a temporary
terminated ByteBuffer; allocation failure returns Allocation and cleans that
buffer. An owned C numeric locale is installed on the calling thread and restored
before its allocation is freed, on every ordinary/error exit. Other threads'
locales are not modified. Failure to create/install the locale is Locale.

Formatting calls `strfromd` with 17 significant decimal digits for f64 and
`strfromf` with 9 for f32, returning `Decimal {bytes:[u8;64],len:i64}`. Output fits
the fixed buffer, includes signed zero and represents infinities/NaN canonically.
Finite binary64/binary32 values round-trip in round-to-nearest mode, including
subnormals. As for native floating arithmetic, the current floating rounding mode
applies; the library does not change an external caller's rounding mode. Text
conversion follows glibc's correctly rounded conversion rules and does not
promise the shortest possible decimal spelling.

`std.buffered_io.Reader` and `Writer` own an `std.io.File` and one initialized
64-byte buffer. Construction transfers the actual file owner. `read_byte`
returns checked EOF or one byte, retaining short reads for subsequent calls.
`read` copies up to the supplied window and may return a short successful count;
a zero-length window leaves the file untouched. EOF is cached.

`write` accepts a prefix and returns its exact count; it can fail only before
accepting any new bytes. `write_byte` accepts one byte or returns an error before
acceptance. `flush` retries successful short writes, removes only acknowledged
prefixes and retains the exact unwritten suffix on error. Zero progress and
impossible foreign counts reject rather than spinning or losing pending bytes.
`pending` reports the remaining buffered bytes. An I/O error does not rewind the
underlying descriptor.

`finish(writer)->Writer!Finished` flushes and transfers the file into the
`Finished` owning package on success. Failure returns the actual Writer with its
pending bytes and descriptor, allowing retry. `into_file(finished)` transfers the
file for further use. Dropping a Writer closes its descriptor once and abandons
pending bytes; destruction does not perform an unobservable fallible flush.
Callers requiring completed output must observe `finish` or `flush` explicitly.
These are synchronous POSIX interfaces; EINTR and EAGAIN remain explicit I/O
errors, preserving progress for a caller-chosen retry policy.
