# 同步公共 helper 与拥有环境

`main.zil` 是五模块、有界文本事件应用：Window 完整拥有 Model、Subscription、Surface；一帧的字段 Desc view 经 `helpers.sample_twice(&view)` 调用两次，最后使用后仍可完整移动 Window。输入/输出与前一片 `package_ui` 相同，资源正常关闭轨迹为 `MSU`。它验证了公共 helper 作为真实消费者的链路；Subscription 仍未保存回调，这不是图形 ZUI 成品。

`helpers.zil` 导出一个具体环境参数的有限通用函数，经过真实模板实例化产生独立函数。`&F` 保留原环境，`&mut F` 更新原环境，按值 `F: FnOnce` 转移责任。公共合同含 Hold、Call、保守 writes 上界、Owned/Borrowed Cleanup 和有限调用轨迹，独立 Verify 后才进入接口。调用者不以强制内联 helper 私有正文来获得权限。

```zil
pub fn sample_twice<F: Fn() -> i32>(f: &F) -> i32 {
    f()
    return f()
}
```

`lifecycle.zil` 另用四模块覆盖清理和适配。`lifecycle.rs` 是独立手写行为对照；其 `shared_errors` 显式用 RefCell 表达共享 Fn 修改外部目标，仅用于行为比较，不是 ZIL 隐式生成的结构，也不构成两者运行/内存成本可比性。

| 输入 | 退出码 | stdout | 观察 |
|---|---:|---|---|
| `s` | 0 | `11A` | 拥有 Fn 借入两次后原环境再次调用，最后清理一次 |
| `m` | 0 | `23A` | 拥有 FnMut 两次更新后原环境第三次调用 |
| `0` | 0 | `AB0` | FnOnce 零次执行，环境按捕获顺序清理 |
| `1` | 0 | `AB1` | FnOnce 消费 A，随后清理剩余 B |
| `a` / `b` | 0 | `A3` / `A4` | 实际 Fn / FnMut 适配为按值 FnOnce |
| `o` | 0 | `33A` | 拥有 Fn 以 `&mut F` 适配 FnMut 后仍可再次调用 |
| `c` | 0 | `7` | Copy Fn 的原有按值 ABI 适配 FnOnce |
| `p` | 1 | `BAZ` | 后续实参 `?` 失败，helper 未进入，准备责任逆序清理 |
| `P` | 0 | `HAB3Z` | 同一准备路径成功提交，helper 实际进入 |
| `e` / `f` | 1 | `1A` / `12A` | 第一次 / 第二次回调错误后停止并清理 |
| `E` | 0 | `12:3:33A` | 回调两次成功，返回后第三次仍能使用原环境 |
| `d` / `D` | 1 / 0 | `1A` / `12:3A` | 共享 Fn 修改外部 Desc 的错误 / 正常路径 |

`crates/zil-driver/tests/impl064.rs` 对 UI 的 10 个输入和上述 15 个生命周期输入分别运行 ZIL/Rust O0/O2，共 100 次行为对照；保留阶段 IR、已核公开接口、真实构建命令、每次输入/输出/退出码。最终受测身份以 `evidence/impl/064/acceptance.json` 为准，不累计重复调试运行。

本片只支持一个具体环境参数、每路径最多两次 callback、有限 Copy callback 实参/结果及同步 Copy 错误。FnMut helper 通过 `&mut F` 接收；普通参数仍不可写。额外 helper 参数可为有限 Copy、Managed give 或完整 Package。循环/递归/转发 helper、保存或返回环境、拥有错误、平台窗口/GPU 仍需后续实现。Contract Return 不按错误标签分割关系；writes 是能力上界，可能保守拒绝安全程序。

运行：

```sh
cargo run -p zil-cli --offline --locked -- build examples/mainline/helper_ui/main.zil -o out/helper-ui --emit-dir out/helper-ui-ir --json
printf '++q' | out/helper-ui
cargo test -p zil-driver --test impl064 --offline --locked
```

工具链按 `tools/toolchain-profile.json` 配置。ABI 保留具体环境，借用拥有环境通过真实指针调用；未引入隐式堆分配、RC、锁或 `noalias`。
