# 纯 ZIL Win64 首窗消费者（draft18）

这里是编译器的真实平台消费者。`main.zil` 导入 `platform.zil`，经 Parse、Expand、CT、Types、Analyze、独立 Verify、Lower 和 LLVM 产生 Win64 PE。窗口注册、WndProc、消息循环、键鼠事件、状态更新、GDI 绘制和资源释放均由 ZIL 源码发起。没有手写 C 窗口宿主、命令回放层或 ZIL 对象指针全局表。

## 构建与操作

使用具有 x86 COFF 工具的 LLVM，显式建立导入库；输出目录必须尚不存在：

```bash
python3 tools/platform/087/imports.py --out out/win64-imports
cargo build --offline --locked -p zil-cli
target/debug/zil build examples/mainline/win32/main.zil \
  --target win64 --opt O2 --clang clang --linker lld-link \
  --windows-lib out/win64-imports/kernel32.lib \
  --windows-lib out/win64-imports/kernelbase.lib \
  --windows-lib out/win64-imports/user32.lib \
  --windows-lib out/win64-imports/gdi32.lib \
  --windows-lib out/win64-imports/msvcrt.lib \
  -o out/zil-window.exe --json
```

`build` 生成 `.exe` 和对应 `.exe.ll`，报告分别记录 clang 和 lld-link 的实际参数。交叉目标不由 `zil run` 隐式启动；在 Windows 运行生成的 PE，或在准备好的 Wine 环境显式运行。实际验证环境见作者和正式验收收据；Wine 结果只代表兼容层执行。

窗口初始状态为 0。空格/Enter 或点击按钮令状态加 1，标题、文本和按钮颜色随之改变；窗口大小改变触发重绘。Esc 或系统关闭操作退出消息循环，由 ZIL 资源作用域释放窗口后注销窗口类。示例为观察中文结果显式请求 `WenQuanYi Zen Hei` 字体；验收环境提供该字体。其他机器应安装该字体或在平台源码中选择其已安装的中文字体，其他字体的回退行为未在本轮声称验证。

`fail_create.zil` 请求不存在的窗口类，预期退出 42；`fail_register.zil` 重复注册同类，预期退出 43。两条路径均先成功注册一次，只注销已取得的类，不销毁未取得的窗口。正常路径的末尾日志依次为 `D:1`、`W:1`、`U:1`：收到销毁消息、DestroyWindow 成功、UnregisterClassW 成功。每次绘制的字体和两个画刷都单独释放。

## 独立机器对照

`crates/zil-driver/tests/impl087_native.rs` 在 Linux 上执行真实 ZIL 源码的 u16 算术、转换、陷阱及窄 C ABI；其中的 C 文件是 ABI 测试 oracle，完全不进入窗口应用。独立审核另检验超过寄存器参数数量的静态回调、混合 record 布局和静态值保留。

在隔离 X11 显示、Wine、xdotool、ImageMagick `import` 和 Pillow 已准备的环境中：

```bash
python3 tools/platform/087/verify.py \
  --compiler target/debug/zil --imports out/win64-imports \
  --wine /absolute/path/to/wine64 --out out/win64-check-001
```

该验证创建新输出目录，保留命令、stdout/stderr、阶段产物、PE、截图和 SHA256；对 O0/O2 分别执行两个失败入口、真实窗口的键盘/鼠标/像素变化/resize/退出、独立 C ABI 和大栈探针。O0 的大栈 PE 必须实际导入并执行 `__chkstk`；O2 可能消去本地数组，因此不强求保留该指令。测试源使用 1024 个 i64、8192 字节的缓冲区，并保持编译器默认逻辑预算。

早期作者验证中 4096 元素的 u16 栈夹具正确触发独立 Verify 预算上限，随后另一版夹具误用了未支持的 i64 字面量后缀。两次失败均保留；修正的是测试输入，未提高默认预算、放宽验证或关闭栈探针。

## 有限能力与下一阶段

`c_handle` 是不可解引用的不透明 Copy 外部值，不能转换成 ZIL 借用或整数；它自身不证明 OS 对象的存活与唯一释放。此例以显式资源包装取得的 Win32 句柄。状态使用 Win32 分配的 8 字节窗口扩展槽保存一个整数；没有将 ZIL 存储地址交给可保留回调。

`c_wstr` 只来自合法 UTF16 静态字面量或 NULL。`c_callback` 只取无环境、静态定义且签名受限的真实 C ABI 函数地址。标有 `static_callbacks()` 的 unsafe 声明可重入这些静态函数，但不得借机把当前借出的 ZIL 地址交给回调或跨调用保留；按地址传递 record 仍必须遵守 `noescape`。这没有开放带捕获的长期 OS 回调。

本消费者建立首窗启动所需的真实语言—ABI—OS 链。它没有完成独立 ZUI 仓库、组件树、动态容器、长期可变环境、布局、焦点/IME、可访问性、DirectWrite/DirectX 或跨平台渲染。GDI 仅用于验证绘制链；下一步应把首窗边界迁入 ZUI 平台层，按实际需求推进长期状态与事件订阅的所有权，再实现布局和绘制后端。原生 Windows 机器和驱动验收仍须另做。

## 外部 ABI 依据

- [Microsoft WNDCLASSEXW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-wndclassexw)
- [Microsoft WNDPROC](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nc-winuser-wndproc)
- [Microsoft x64 ABI](https://learn.microsoft.com/en-us/cpp/build/x64-software-conventions?view=msvc-170)
- [Microsoft __chkstk](https://learn.microsoft.com/en-us/windows/win32/devnotes/-win32-__chkstk)

布局规则以当前 ZIL 规范为准；上述官方文档和真正的 C 编译器/目标执行构成外部对照。仅支持已登记的 64 位小端 Linux SysV 与 Win64 MSVC 严格子集。
