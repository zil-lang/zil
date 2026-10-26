// Pure ZIL Win64 platform consumer. There is no C/Rust window host.
// These private unsafe declarations follow the documented x64 Windows ABI.
// Static callback reentry cannot inspect any currently borrowed ZIL record.
struct Rect { left: i32; top: i32; right: i32; bottom: i32 }
struct Point { x: i32; y: i32 }
struct Message {
    window: c_handle; message: i32; wparam: i64; lparam: i64
    time: i32; point: Point; private_data: i32
}
struct Paint {
    dc: c_handle; erase: i32; area: Rect; restore: i32; update: i32
    reserved: [u8; 32]
}
struct WindowClass {
    size: i32; style: i32
    procedure: extern "C" fn(c_handle, i32, i64, i64) -> i64
    class_extra: i32; window_extra: i32; instance: c_handle
    icon: c_handle; cursor: c_handle; background: c_handle
    menu_name: c_wstr; class_name: c_wstr; small_icon: c_handle
}

extern "C" fn GetModuleHandleW(name: c_wstr) -> c_handle;
extern "C" fn GetLastError() -> i32;
extern "C" fn RegisterClassExW(value: &WindowClass) -> u16
contract { provides(noescape(value), static_callbacks()); effects(ffi); };
extern "C" fn UnregisterClassW(name: c_wstr, instance: c_handle) -> i32
contract { provides(static_callbacks()); effects(ffi); };
extern "C" fn CreateWindowExW(ex_style: i32, class_name: c_wstr, title: c_wstr,
    style: i32, x: i32, y: i32, width: i32, height: i32,
    parent: c_handle, menu: c_handle, instance: c_handle, parameter: c_handle) -> c_handle
contract { provides(static_callbacks()); effects(ffi); };
extern "C" fn DefWindowProcW(window: c_handle, message: i32, wparam: i64, lparam: i64) -> i64
contract { provides(static_callbacks()); effects(ffi); };
extern "C" fn GetMessageW(message: &mut Message, window: c_handle, first: i32, last: i32) -> i32
contract { provides(noescape(message), static_callbacks()); effects(ffi); };
extern "C" fn TranslateMessage(message: &Message) -> i32
contract { provides(noescape(message), static_callbacks()); effects(ffi); };
extern "C" fn DispatchMessageW(message: &Message) -> i64
contract { provides(noescape(message), static_callbacks()); effects(ffi); };
extern "C" fn ShowWindow(window: c_handle, show: i32) -> i32
contract { provides(static_callbacks()); effects(ffi); };
extern "C" fn UpdateWindow(window: c_handle) -> i32
contract { provides(static_callbacks()); effects(ffi); };
extern "C" fn DestroyWindow(window: c_handle) -> i32
contract { provides(static_callbacks()); effects(ffi); };
extern "C" fn PostQuitMessage(code: i32);
extern "C" fn PostMessageW(window: c_handle, message: i32, wparam: i64, lparam: i64) -> i32;
extern "C" fn GetWindowLongPtrW(window: c_handle, index: i32) -> i64;
extern "C" fn SetWindowLongPtrW(window: c_handle, index: i32, value: i64) -> i64;
extern "C" fn SetWindowTextW(window: c_handle, text: c_wstr) -> i32
contract { provides(static_callbacks()); effects(ffi); };
// Only NULL is passed for this optional RECT pointer; no ZIL address is erased.
extern "C" fn InvalidateRect(window: c_handle, rect: c_handle, erase: i32) -> i32
contract { provides(static_callbacks()); effects(ffi); };
extern "C" fn BeginPaint(window: c_handle, paint: &mut Paint) -> c_handle
contract { provides(noescape(paint), static_callbacks()); effects(ffi); };
extern "C" fn EndPaint(window: c_handle, paint: &Paint) -> i32
contract { provides(noescape(paint), static_callbacks()); effects(ffi); };
extern "C" fn FillRect(dc: c_handle, area: &Rect, brush: c_handle) -> i32
contract { provides(noescape(area)); effects(ffi); };
extern "C" fn CreateSolidBrush(color: i32) -> c_handle;
extern "C" fn DeleteObject(value: c_handle) -> i32;
extern "C" fn CreateFontW(height: i32, width: i32, escapement: i32, orientation: i32,
    weight: i32, italic: i32, underline: i32, strikeout: i32, charset: i32,
    precision: i32, clip: i32, quality: i32, pitch: i32, face: c_wstr) -> c_handle;
extern "C" fn SelectObject(dc: c_handle, object: c_handle) -> c_handle;
extern "C" fn SetBkMode(dc: c_handle, mode: i32) -> i32;
extern "C" fn SetTextColor(dc: c_handle, color: i32) -> i32;
extern "C" fn TextOutW(dc: c_handle, x: i32, y: i32, text: c_wstr, units: i32) -> i32;
extern "C" fn GetCursor() -> c_handle;
extern "C" fn putchar(character: i32) -> i32;
extern "C" fn fflush(stream: c_handle) -> i32;

fn digits(value: i64) {
    if value >= 10 { digits(value / 10) }
    unsafe { putchar(48 + i32(value % 10)) }
}
fn trace(event: i32, value: i64) {
    unsafe { putchar(event); putchar(58) }
    if value < 0 { unsafe { putchar(45) }; digits(-value) }
    else { digits(value) }
    unsafe { putchar(10); fflush(c_null()) }
}

resource RegisteredClass { instance: c_handle; name: c_wstr }
impl Drop for RegisteredClass {
    fn drop(self: Self) { unsafe { trace(85, i64(UnregisterClassW(.name, .instance))) } }
}
resource OwnedWindow { raw: c_handle }
impl Drop for OwnedWindow {
    fn drop(self: Self) { unsafe { trace(87, i64(DestroyWindow(.raw))) } }
}
resource Brush { raw: c_handle }
impl Drop for Brush {
    fn drop(self: Self) { unsafe { trace(66, i64(DeleteObject(.raw))) } }
}
resource Font { raw: c_handle }
impl Drop for Font {
    fn drop(self: Self) { unsafe { trace(84, i64(DeleteObject(.raw))) } }
}

fn paint_window(window: c_handle) {
    var paint = Paint { dc: c_null(), erase: 0,
        area: Rect { left: 0, top: 0, right: 0, bottom: 0 }, restore: 0, update: 0,
        reserved: [0u8,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0] }
    unsafe {
        let dc = BeginPaint(window, &mut paint)
        if dc == c_null() { trace(70, 21); PostQuitMessage(21); return }
        let state = GetWindowLongPtrW(window, 0)
        let raw_background = CreateSolidBrush(0x00F8F8F8)
        let color = if state % 2 == 0 { 0x00F4DFC7 } else { 0x00C9EDCA }
        let raw_button = CreateSolidBrush(color)
        if raw_background == c_null() || raw_button == c_null() {
            if raw_background != c_null() { DeleteObject(raw_background) }
            if raw_button != c_null() { DeleteObject(raw_button) }
            EndPaint(window, &paint); trace(70, 22); PostQuitMessage(22); return
        }
        let background = Brush { raw: raw_background }
        let button = Brush { raw: raw_button }
        let raw_font = CreateFontW(-20, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0,
            c_wstr("WenQuanYi Zen Hei"))
        if raw_font == c_null() { EndPaint(window, &paint); trace(70, 24); PostQuitMessage(24); return }
        let font = Font { raw: raw_font }
        let old_font = SelectObject(dc, font.raw)
        if old_font == c_null() { EndPaint(window, &paint); trace(70, 25); PostQuitMessage(25); return }
        let button_area = Rect { left: 36, top: 104, right: 404, bottom: 162 }
        var okay = FillRect(dc, &paint.area, background.raw) != 0
        okay = FillRect(dc, &button_area, button.raw) != 0 && okay
        SetBkMode(dc, 1); SetTextColor(dc, 0x00352D28)
        okay = TextOutW(dc, 36, 28, c_wstr("ZIL / ZUI — 首窗"), 14) != 0 && okay
        if state == 0 {
            okay = TextOutW(dc, 36, 64, c_wstr("State 0 | 等待输入"), 14) != 0 && okay
        } else if state == 1 {
            okay = TextOutW(dc, 36, 64, c_wstr("State 1 | 已经更新"), 14) != 0 && okay
        } else {
            okay = TextOutW(dc, 36, 64, c_wstr("State 2+ | 再次更新"), 15) != 0 && okay
        }
        okay = TextOutW(dc, 52, 124, c_wstr("Click here / Space to update"), 28) != 0 && okay
        okay = TextOutW(dc, 36, 200, c_wstr("Esc or Close to release resources"), 33) != 0 && okay
        SelectObject(dc, old_font)
        if EndPaint(window, &paint) == 0 || !okay { trace(70, 23); PostQuitMessage(23) }
        trace(80, state)
    }
}

fn update(window: c_handle) {
    unsafe {
        let state = GetWindowLongPtrW(window, 0)
        let next = if state < 999 { state + 1 } else { 0 }
        SetWindowLongPtrW(window, 0, next)
        if next == 0 { SetWindowTextW(window, c_wstr("ZIL ZUI - state 0")) }
        else if next == 1 { SetWindowTextW(window, c_wstr("ZIL ZUI - state 1")) }
        else { SetWindowTextW(window, c_wstr("ZIL ZUI - state 2+")) }
        InvalidateRect(window, c_null(), 0)
        trace(69, next)
    }
}

fn window_proc(window: c_handle, message: i32, wparam: i64, lparam: i64) -> i64 {
    unsafe {
        if message == 1 { SetWindowLongPtrW(window, 0, 0); return 0 }
        if message == 15 { paint_window(window); return 0 }
        if message == 256 {
            if wparam == 27 { PostMessageW(window, 16, 0, 0); return 0 }
            if wparam == 32 || wparam == 13 { update(window); return 0 }
        }
        if message == 514 {
            let x = lparam % 65536
            let y = (lparam / 65536) % 65536
            if x >= 36 && x < 404 && y >= 104 && y < 162 { update(window) }
            return 0
        }
        // Close schedules orderly main-scope cleanup; only OwnedWindow destroys it.
        if message == 16 { trace(81, 0); PostQuitMessage(0); return 0 }
        if message == 2 { trace(68, 1); return 0 }
        return DefWindowProcW(window, message, wparam, lparam)
    }
}

// mode 0: interactive; mode 1: fail creation; mode 2: duplicate registration.
pub fn run(mode: i32) -> i32 {
    unsafe {
        let instance = GetModuleHandleW(c_wstr_null())
        if instance == c_null() { trace(70, 10); return 10 }
        let class_name = c_wstr("ZIL.ZUI.Draft18.首窗")
        let class = WindowClass { size: 80, style: 3,
            procedure: c_callback(window_proc), class_extra: 0, window_extra: 8,
            instance: instance, icon: c_null(), cursor: GetCursor(), background: c_null(),
            menu_name: c_wstr_null(), class_name: class_name, small_icon: c_null() }
        let atom = RegisterClassExW(&class)
        if atom == 0u16 { trace(70, i64(GetLastError())); return 11 }
        let registered = RegisteredClass { instance: instance, name: class_name }
        trace(82, 1)
        if mode == 2 {
            let duplicate = RegisterClassExW(&class)
            if duplicate != 0u16 { trace(70, 40); return 40 }
            trace(88, i64(GetLastError())); return 43
        }
        let requested_class = if mode == 1 { c_wstr("ZIL.Missing.Class") } else { registered.name }
        let raw = CreateWindowExW(0, requested_class, c_wstr("ZIL ZUI - state 0"),
            0x00CF0000, 100, 80, 480, 320, c_null(), c_null(), instance, c_null())
        if raw == c_null() { trace(88, i64(GetLastError())); return 42 }
        let window = OwnedWindow { raw: raw }
        trace(67, 1)
        ShowWindow(window.raw, 5); UpdateWindow(window.raw)
        var message = Message { window: c_null(), message: 0, wparam: 0, lparam: 0,
            time: 0, point: Point { x: 0, y: 0 }, private_data: 0 }
        loop {
            let available = GetMessageW(&mut message, c_null(), 0, 0)
            if available == -1 { trace(70, i64(GetLastError())); return 15 }
            if available == 0 { return i32(message.wparam) }
            TranslateMessage(&message)
            DispatchMessageW(&message)
        }
    }
}
