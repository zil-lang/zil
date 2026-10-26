# 多模块标量示例

`main.zil` 从项目根目录导入 `math/arithmetic.zil` 和 `other.zil`。
两个库都有 `answer`，通过各自模块别名调用；`arithmetic` 的 `increment`
保持私有。库中的 `main` 是普通函数，运行入口由根模块的 `main` 提供。

```sh
cargo run --offline --locked -p zil-cli -- build examples/modules/main.zil -o /tmp/zil-modules
/tmp/zil-modules
```

预期进程退出码为 **42**。输出由编译器的真实 Core → Types → Analyze →
独立 Verify → LIR → LLVM → clang 管线生成，不翻译成 Rust。

当前文件适配器以入口文件的规范化父目录作为项目根；`import a.b as c`
读取根下唯一存在的 `a/b.zil` 或 `a/b/mod.zil`。两者同时存在报
`E-MODULE-AMBIGUOUS`，目录内其它文件不自动合并或导出。旧的斜杠路径报
`E-PARSE-IMPORT-SEPARATOR`，提示改用点；`a.zil` 中的 zil 仍是一个逻辑段。
默认别名是最后一个路径段。导入不得经符号链接逃出
根目录；内部符号链接别名复用同一 ModuleId。`pub fn` 才可跨模块调用。
`compile_source` 只编译调用者提供的单个输入；有 import 时请用
`compile_path` 或显式提供 `ProjectSources` 调用 `compile_project`。

这是标量模块切片。资源、聚合、闭包接口、语法导入、包解析与模块环还没有
贯通，不能将这里的 `ScalarInterface` 当作完整 ExportedInterface。

对应限定回归入口：

```sh
cargo test --offline --locked -p zil-driver --test projects
```

M001–M027 均属于 Cargo 普通测试，主树 `cargo test --workspace` 会覆盖。
