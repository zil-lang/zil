# 固定布局事件包生成与普通源对照

这组消费者检验类型声明生成与 `where Copy` 能否在实际模块、普通泛型和机器后端中组合。二维指针载荷和三维触点载荷分别使用不同的名义类型；泛型 `duplicate` 必须依据声明的 `Copy` 约束复制整个事件值，再由普通函数读取所有载荷字段。

| 目录 | 类型与别名 | 复制约束 | 验证目的 |
| --- | --- | --- | --- |
| `plain` | 手写两套声明 | `T: Copy` | 既有普通源对照 |
| `generated` | `#for` 逐轮生成 `Payload`、`Event` 及适配函数 | `T: Copy` | 独立检验 228 |
| `where` | 手写两套声明 | `where T: Copy` | 独立检验 229 |
| `combined` | 同一生成模板 | `where T: Copy` | 检验两片组合 |

四份 `main.zil` 完全相同。`where` 与 `plain`、`combined` 与 `generated` 的唯一源码差异是约束的书写位置。每轮私有 `Event` 的名义身份必须独立，两个公开适配入口只返回普通整数；生成过程不需要字符串拼接类型名、反射或运行时类型表。

程序读取一个字节：`0` 处理二维载荷，输出 `J` 并返回 0；`1` 处理三维载荷，输出 `W` 并返回 0；其他输入不输出并返回 7。这是固定布局输入适配的语言消费者，尚不包含窗口、渲染或长期外部回调。

## 运行

先按仓库 `docs/LLVM-API.md` 配置支持的 LLVM 工具链，在仓库根执行：

```bash
cargo test --locked --offline -p zil-driver --features llvm-api --test impl228_consumer
cargo build --locked --offline -p zil-cli
mkdir -p out
"${CARGO_TARGET_DIR:-target}/debug/zil" build examples/mainline/generated_events228/combined/main.zil --backend llvm-api --opt O2 -o out/events
printf '0' | ./out/events
printf '1' | ./out/events
```

`impl228_consumer` 对四种源码各做 O0/O2 构建及三种输入执行，要求真实指定后端、无回退、精确 stdout/stderr 和退出码。最近反例验证：同布局的不同生成名义类型不能错接，后置 `Copy` 不能复制真实拥有资源。另有实际文件系统 CLI 的组合入口检查。

原有编译器对手写源的成功、对三组新写法的 `Unsupported`、公共保留名提取前后的精确预算、各次候选和执行结果分别保存在 `evidence/reviews/228-consumer/`。原始源、阶段产物和机器输出均归档并逐文件核验，基线重复执行不与最终消费者次数相加。消费者通过只说明这些实际受测路径，不能替代 228/229 的完整独立审查和主线正式验收。
