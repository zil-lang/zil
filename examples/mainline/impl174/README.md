# 174：State 直接保存动态 Store

这一组真实 ZIL 程序让完整 `State` 同时拥有一个 `Ticket` 和一个
`Store<Item>`。每个 `Item` 又拥有 `Ticket` 与真实分配的 `ByteBuffer`。
`State` 可以经过普通模块函数的参数和返回值转交，动态字段可以整个取出、
补回或替换；所有退出路径仍沿原有字段顺序清理实际拥有的前缀。

## 源码与执行

`model.zil` 定义元素及字节校验；`storage.zil` 准备运行时指定长度的批次；
`state.zil` 保存并更新完整状态。三个入口分别是 `main.zil`、
`take_exit.zil` 与 `reserve_recovery.zil`，每个入口实际经过四个模块。

```zil
pub package State {
    manages ticket: model.Ticket;
    manages children: Store<model.Item>;
    pub revision: i32
}

pub fn replace(input: State, count: i32) -> void!State {
    var current = input
    current.children = storage.into_store(storage.prepare(count, 67)?)
    if store_len(current.children) != i64(count) { return () }
    if count > 0 { storage.check_tail(current.children, 66 + count)? }
    current.revision = 1
    return current
}
```

上面的替换先完整准备右侧。右侧准备失败时，原 `State` 和原有 A/B 前缀
继续承担原来的清理责任。成功时，旧 A/B 前缀先清理，再提交 C/D/E 新前缀。
`give input.children` 则取出整个 Store，后续部分 `State` 退出只清理尚存字段。
示例中的字母来自实际 `Drop` 调用，不是模拟清理事件。

从仓库根构建并运行：

```bash
cargo build -p zil-cli --bin zil --offline
target/debug/zil build examples/mainline/impl174/main.zil -o /tmp/zil174-main
printf 3 | /tmp/zil174-main
```

如果使用独立 `CARGO_TARGET_DIR`，将 CLI 路径替换为该目录中的 `debug/zil`。

| 入口及输入 | 实际输出 | 退出码 | 检查的责任路径 |
|---|---|---:|---|
| `main`，`0` | `ABZ` | 0 | 新字段为空，旧动态前缀仍完整释放 |
| `main`，`1` | `ABZC` | 0 | 旧字段先清理，最后按 State 字段顺序释放 Z/C |
| `main`，`3` | `ABZCDE` | 0 | 运行时三元素批次跨模块转交与替换 |
| `main`，`9` | 空 | 9 | 未创建 State 的输入分支 |
| `take_exit` | `ABZ` | 1 | 取出的 Store 与剩余父壳分别清理一次 |
| `reserve_recovery` | `ZAB` | 0 | 原 Store 扩容、读取真实尾项、回插及退出清理 |

## 本片有限合同

Store 仍需显式 `manages` 字段。通过字段读取取得的 Desc 来自这个实际字段，
替换或转交之后不能用旧 Desc 重新指向新实例。完整 State 可作为普通参数、
返回值、拥有错误载荷或普通完整固定数组的元素；缺字段的父壳不得整体转交。

元素保持 172 的有限完整 Package：非零布局、直接用户资源或 ByteBuffer
Managed 字段，以及 Copy 字段。嵌套 Store 元素和 Store/含 Store 的 State
捕获仍关闭。170 的拥有数组字段仍保留其原有叶形状，因此不能把含 Store 的
State 再嵌入这种数组字段。本片支持的后端范围与 172 相同，为 Linux x64。

State 继续内联保存原三字 Store 头。清理撤销父壳或字段责任标志后调用
Store 专用 drop helper；该 helper 清理 `[0, len)` 并释放真实分配。
`teardown=None` 不再使 Package 字段清理遗漏 Store，且没有引入 Box、RC 或
按静态 capacity 展开的字段槽。本片不增加 IR/Wire 指令。

## 实际验证与已保留失败

实现与测试提交是 `9a878f2f91aae4efd4b851eeb8979cacc21ffdd1`。
后续实际消费者、相交回归和质量检查绑定
`cc6508f7491db70452a006873c814063afe54c88`，二者的 `crates` 树完全相同：
`506a899b2204d3f5abdd398defebce1b98d216d4`。使用公共 schema/Wire 21，
规范摘要为 `8aecd0f052b3d8eb099d19d26cd0bad56b26d3a7be91c79aca5a962a010711be`。

- 新增 174 suite 的 13 项和保留/迁移后的 172 suite 的 16 项全部通过。
  新 suite 包含 24 次 O0/O2 原生执行、10 个源码精确拒绝、7 对 raw/Wire
  独立拒绝，以及 Verify `N=22550`、`N-1=22549` 的精确预算边界。
- `consumer.py` 实际完成 6 次 Verify1 构建及 50 次 O0/O2 执行：12 次普通
  运行、10 次分配计数、28 次分配失败注入。全部 38 次带分配统计的运行
  都得到 `live=0` 和 `allocated=frees`。失败点包括新 State 准备、替换
  批次的每次分配，以及保留旧内容后继续运行的 reserve 失败。
- ByteBuffer 字段、实际字段来源、拥有错误载荷、普通拥有数组及 170 字段
  状态机共 8 个既有 suite、88 项相交回归通过。格式检查、涉及的 5 个库
  和 172/174 两个测试目标的 Clippy `-D warnings` 通过。

上述是施工方定向验证，不代替非作者审核、集成全量或产品门。13 项测试中的
消费者普通运行和 50 次运行中部分输入重复，不能据此扩大功能覆盖结论。

首次 Analyze 旧字段白名单拒绝、旧 target 在公共接口合并后的 StableCrateId
碰撞，以及迁移前的旧拒绝预期与新夹具错误均保留实际记录。生产修正只包括
已授权的精确 Store 字段叶和本片定义发现/清理；旧 target 碰撞通过隔离新
target 解决，测试夹具错误没有放宽生产诊断或验证规则。具体命令、阶段
输出和失败日志见 `evidence/impl/174/`，普通源码包可以单独使用而无需证据。

## Rust 四目标与 ZUI 前置

| 目标 | 本片实证与限制 |
|---|---|
| 同类安全 | 实际字段责任、独立 raw/Wire 核验、描述失效和失败清理已覆盖本片有限合同；完整语言安全仍未完成 |
| 运行与内存成本 | 真实动态分配平衡、前缀析构顺序和内联头有实证；未做相对 Rust 的运行时间、峰值内存或尾延迟测量 |
| 表达与维护 | 普通 State 可以直接保存并跨模块转交动态子项；未测量同任务的维护成本优势 |
| 编译成本 | 只运行正确性与预算检查；本轮没有 Rust 编译性能比较 |
| 至少一项可重复净突破 | Store 字段是已运行的语言组合增量，尚无产品级净突破结论 |

这一片补齐 ZUI 持有动态子项状态的一个前置。动态共享元素窗口、回调字段、
一般混合捕获和外部事件关闭协议由其他切片继续实施；本片不宣称完成 ZUI
或完整语言。`B0 Doing`、完整语言 `NotSatisfied`、产品 `NotRun` 保持。
