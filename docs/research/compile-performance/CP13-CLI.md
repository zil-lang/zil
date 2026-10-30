# CP13：默认关闭的 driver / CLI 适配

此适配依赖 LLVM agent 的 backend core `6d3f01580c1ae921a1b26b410a8288acc8dff4ba`。新增 `CompileOptions.closed_executable: bool`，默认 false；完整构建在本次独立 Verify 之后把值传给 `BuildOptions.closed_executable`。没有改变 Verify、原对象缓存算法或语言接受性。

CLI 用无值开关显式请求：

```sh
zil build main.zil -o program --backend llvm-api --opt O2 --closed-executable --json
```

首版只允许 Linux host 的 `x86_64-unknown-linux-gnu` 完整 build。`check`、`run`、Win64 配置收到这个显式选项会在编译前给出 `E-CLI-ARGUMENT`，不会静默忽略。HELP 明确此模式不承诺生成 ZIL 身份符号作为 export ABI；具体内部化资格、外部符号/跨 CGU 保留及调用引用规则仍由 backend core 管理。

完整 JSON 的 `native_codegen` 保留 backend 提供的两个真实字段：

- `closed_executable`：当前生成是否启用该模式；不是仅从 CLI 文本推断。
- `internalized_functions`：本次 backend 变换的实际函数数，默认模式为 0。

这个开关只影响 Verify 之后的机器 linkage，因此明确从 CP12 前端 checkpoint key 排除。相同源程序切换该模式时仍能重用原始前端数据，随后必须重新 Verify；机器对象键包含变换后的实际 Module/linkage，不能借用旧模式的对象。

限定测试包含：LLVM API 与 Clang 两个 backend 的 default false/显式 true 及正内部化计数，实际程序状态 42；三种不适用 CLI 入口拒绝；同输入 false→true→true 时前端为 miss/hit/hit，对象为 miss/miss/hit，三次 Verify 均 1；原 CP12 的独立进程/重新安装身份/联合缓存控制；更新的 key 排除单元控制。日志位于 `evidence/research/compile-performance/CP13/cli-adapter/`。

这里只核对适配与当前行为，不计性能成绩。正式 O2 编译时间和相对旧完整 O2 产物的运行质量仍由协调者在冻结计划后独立计量。
