# 保存整个组件的闭包

`main.zil`：跨模块工厂返回拥有完整 Window 的 FnMut；调用方通过普通泛型 helper 反复借用同一环境，支持计数、替换、take/refill、分支恢复及错误退出。组件按 model、surface、subscription 的声明顺序清理。标准输入 `+r-q`、`tbbq`、`+fq` 可观察更新与失败。

`lifecycle.zil`：普通工厂接完整 Package 返回 Fn、调用/整体移动/未调用清理；FnMut 失败后再次调用、whole-move 后补回、循环字段补回；FnOnce 的部分/整体/错误消费。额外 Managed 捕获分布在 Package 前后，固定输出区分捕获顺序与普通局部逆初始化顺序。

`fd.zil`：通过真实 dup/close 效果确认工厂返回后两个文件描述符仍有效，替换旧字段立即释放、保留字段和新字段继续有效，环境退出后都已关闭。

每个程序都有独立 Rust 行为模型；`impl079_package_captures.rs` 对 ZIL/Rust O0/O2 共四种真实二进制执行固定 20 个输入、80 次进程对照，检查 stdout、stderr、退出码。除此还有 CT 拒绝和 LIR 绑定变异测试。固定预期不从 ZIL 输出生成。该组件是同步文本应用，不包含平台窗口、GPU、动态容器或异步。
