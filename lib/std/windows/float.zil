// Windows x64 UCRT floating-point text conversion using explicit C numeric
// locale (_create_locale / _strtod_l / _strtof_l / _snprintf_l).
extern "C" fn _create_locale(category: i32, locale: &u8) -> c_handle
contract { provides(noescape(locale)); effects(ffi); };
extern "C" fn _free_locale(locale: c_handle);
extern "C" fn _strtod_l(source: &u8, end: c_handle, locale: c_handle) -> f64
contract { provides(noescape(source)); effects(ffi); };
extern "C" fn _strtof_l(source: &u8, end: c_handle, locale: c_handle) -> f32
contract { provides(noescape(source)); effects(ffi); };
// On Win64 x64 ABI, args 0..3 occupy RCX/RDX/R8/R9 and arg 4 (`value: f64`)
// occupies stack slot [rsp+32] identically for variadic and non-variadic calls.
extern "C" fn _snprintf_l(output: &mut [u8; 64], size: usize, format: &u8, locale: c_handle, value: f64) -> i32
contract { provides(noescape(output), noescape(format)); effects(ffi); };
extern "C" fn zil_glibc_errno_exchange(next: i32) -> i32;
import std.io

pub enum Error { Empty, Invalid, Range, Allocation, Locale, Format }
pub struct Decimal { pub bytes: [u8; 64]; pub len: i64 }

resource Errno { saved: i32 }
impl Drop for Errno { fn drop(self: Self) { unsafe { zil_glibc_errno_exchange(.saved) } } }
fn errno() -> Errno contract { result(create) } {
    var saved: i32 = 0
    unsafe { saved = zil_glibc_errno_exchange(0) }
    return Errno { saved: saved }
}

resource Locale { handle: c_handle }
impl Drop for Locale { fn drop(self: Self) { unsafe { _free_locale(.handle) } } }
fn locale() -> Error!Locale contract { result(create) } {
    let name = [67u8, 0u8]
    var handle = c_null()
    // LC_NUMERIC = 4 on Windows UCRT
    unsafe { handle = _create_locale(4, &name[0]) }
    if handle == c_null() { return Error.Locale }
    return Locale { handle: handle }
}

fn validate(source: &[u8]) -> Error!bool {
    if source.len() == 0 { return Error.Empty }
    var at: i64 = 0
    if source[0] == 43u8 || source[0] == 45u8 { at = 1 }
    let start = at
    var digits: i64 = 0
    var nonzero = false
    while at < source.len() && source[at] >= 48u8 && source[at] <= 57u8 {
        if source[at] != 48u8 { nonzero = true }
        digits += 1
        at += 1
    }
    if at < source.len() && source[at] == 46u8 {
        at += 1
        while at < source.len() && source[at] >= 48u8 && source[at] <= 57u8 {
            if source[at] != 48u8 { nonzero = true }
            digits += 1
            at += 1
        }
    }
    if digits == 0 { return Error.Invalid }
    if at < source.len() && (source[at] == 101u8 || source[at] == 69u8) {
        at += 1
        if at < source.len() && (source[at] == 43u8 || source[at] == 45u8) { at += 1 }
        let exponent = at
        while at < source.len() && source[at] >= 48u8 && source[at] <= 57u8 { at += 1 }
        if at == exponent { return Error.Invalid }
    }
    if at != source.len() || start == source.len() { return Error.Invalid }
    return nonzero
}

fn terminated(source: &[u8]) -> Error!ByteBuffer contract { result(create) } {
    if source.len() == 9223372036854775807 { return Error.Allocation }
    let bytes = bytes_new()
    if !bytes_reserve(bytes, source.len() + 1) { return Error.Allocation }
    var at: i64 = 0
    while at < source.len() {
        if !bytes_push(bytes, source[at]) { return Error.Allocation }
        at += 1
    }
    if !bytes_push(bytes, 0u8) { return Error.Allocation }
    return give bytes
}

fn special(source: &[u8]) -> i32 {
    var at: i64 = 0
    var negative = false
    if source.len() > 0 && (source[0] == 43u8 || source[0] == 45u8) {
        negative = source[0] == 45u8
        at = 1
    }
    if source.len() - at != 3 { return 0 }
    if source[at] == 105u8 && source[at + 1] == 110u8 && source[at + 2] == 102u8 {
        if negative { return 2 }
        return 1
    }
    if source[at] == 110u8 && source[at + 1] == 97u8 && source[at + 2] == 110u8 {
        return 3
    }
    return 0
}

pub fn parse_f64(source: &[u8]) -> Error!f64 {
    let kind = special(source)
    var zero: f64 = 0.0
    if kind == 1 { return 1.0 / zero }
    if kind == 2 { return -1.0 / zero }
    if kind == 3 { return zero / zero }
    let nonzero = validate(source)?
    let saved_errno = errno()
    let bytes = terminated(source)?
    let scope = locale()?
    var value: f64 = 0.0
    var status: i32 = 0
    unsafe {
        zil_glibc_errno_exchange(0)
        value = _strtod_l(&bytes_view(bytes)[0], c_null(), scope.handle)
        status = zil_glibc_errno_exchange(0)
    }
    if value > 1.7976931348623157e308 || value < -1.7976931348623157e308 || (nonzero && value == 0.0) || (status == 34 && (value == 1.7976931348623157e308 || value == -1.7976931348623157e308)) {
        return Error.Range
    }
    return value
}

pub fn parse_f32(source: &[u8]) -> Error!f32 {
    let kind = special(source)
    var zero: f32 = 0.0f32
    if kind == 1 { return 1.0f32 / zero }
    if kind == 2 { return -1.0f32 / zero }
    if kind == 3 { return zero / zero }
    let nonzero = validate(source)?
    let saved_errno = errno()
    let bytes = terminated(source)?
    let scope = locale()?
    var value: f32 = 0.0f32
    var status: i32 = 0
    unsafe {
        zil_glibc_errno_exchange(0)
        value = _strtof_l(&bytes_view(bytes)[0], c_null(), scope.handle)
        status = zil_glibc_errno_exchange(0)
    }
    if value > 3.4028234663852886e38f32 || value < -3.4028234663852886e38f32 || (nonzero && value == 0.0f32) || (status == 34 && (value == 3.4028234663852886e38f32 || value == -3.4028234663852886e38f32)) {
        return Error.Range
    }
    return value
}

pub fn format_f64(value: f64) -> Error!Decimal {
    let saved_errno = errno()
    let scope = locale()?
    let format = [37u8, 46u8, 49u8, 55u8, 103u8, 0u8]
    var bytes = io.empty_buffer()
    var count: i32 = 0
    unsafe { count = _snprintf_l(&mut bytes, 64usize, &format[0], scope.handle, value) }
    if count < 0 || count >= 64 { return Error.Format }
    return Decimal { bytes: bytes, len: i64(count) }
}

pub fn format_f32(value: f32) -> Error!Decimal {
    let saved_errno = errno()
    let scope = locale()?
    let format = [37u8, 46u8, 57u8, 103u8, 0u8]
    var bytes = io.empty_buffer()
    var count: i32 = 0
    unsafe { count = _snprintf_l(&mut bytes, 64usize, &format[0], scope.handle, f64(value)) }
    if count < 0 || count >= 64 { return Error.Format }
    return Decimal { bytes: bytes, len: i64(count) }
}
