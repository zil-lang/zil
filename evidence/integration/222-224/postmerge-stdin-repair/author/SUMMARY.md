# 223 main CI stdin 驱动返修（作者限定证据）

原 main run 41101248493 的默认工作区为 2860/1/0；`entry_error.zil` 不读取 stdin，旧驱动仍向管道写入 `b'0'`，子进程合法先退出时产生 BrokenPipe。原失败由非作者保留，本记录不改写该失败。

代码提交 `e38ce27fda432e7a55b4a203f4fd047756d9512e` 只改 `review223_machine.rs`：无输入消费者使用 `None` / `Stdio::null`；需要 `getchar` 的 heterogeneous 使用 `Some(mode)` 并继续严格 `write_all(...).unwrap()`。receipt 明确输出 null 或数值字节，未吞 BrokenPipe。原两个 ZIL 输入、allocator shim、全部 exit/stdout/stderr 与分配器判据不变。

新独立 target 上显式启用 llvm-api，限定两项测试 2/0/0；4 个源/优化组合通过真实 LLVM API Builder 编译，60 次 native 执行中 2 次无输入、58 次显式输入；50 次分配器观察包含 34 次真实分配失败，live 均为 0。精确源码同一提交的单文件 rustfmt 和该测试目标 Clippy 均通过。第一次省略 llvm-api 的命令只选 0 项，原日志按 InvalidEvidence/ZeroSelection 保留，不计通过。

完整命令、工具链、测试可执行文件哈希、原输出/阶段产物/native 程序/API 对象均在 raw-evidence.tar.gz，MANIFEST.json 逐文件绑定字节。作者没有运行正式全量或自审；ct_for 独立审查及 root 新完整 CI 尚待完成。完整语言/ZUI、B0/181、产品和原生 Windows 不因此完成。
