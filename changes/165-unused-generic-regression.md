# 165：迁移未实例化泛型的旧拒绝断言

父任务为 B0；派工为 `7f14d1de646e8455a4a096f47958ed33f6a6ef32`。仅修改
`crates/zil-expand/tests/generics.rs` 及本单证据，不改变生产源码、规范、预算或公共接口。

164 的首轮完整 workspace 记录了 1381 项通过、1 项失败。失败的旧测试把两个不同边界
都要求为 `E-UNSUPPORTED`。先用生产源码一致的 712a960 CLI 实测得到：

| 未实例化声明 | 当前真实结果 |
| --- | --- |
| `#[inline] fn id<T>(x:T)->T=>x` | Expand / Unsupported / `E-UNSUPPORTED`，`unexecuted generic declaration attribute` |
| `fn id<T: Copy>(x:T)->T=>x`，没有声明 `Copy` trait | Expand / SafeReject / `E-GENERIC-ARGUMENT`，`generic bound Copy is not a declared nominal trait` |
| 声明 `Read` trait，并在未实例化的 `fn get<T: Read>` 正文中调用 `x.read()` | 源码检查成功，独立 Verify 执行一次 |

`inline` 是已注册的 codegen 属性；此处拒绝的是尚未执行的泛型声明属性，不将它称为
未知属性。162 已引入具名 trait 约束的定义检查，未声明约束因此具有准确的名义约束错误，
旧断言不再反映现有语言行为。

迁移把上述边界拆开：保留未执行属性的精确拒绝；对未声明的 `Copy`、其他未知名称及只有
同名 struct 的约束核对单一诊断的阶段、分类、错误码、消息和约束位置；对已声明合法 trait
的未调用泛型正文要求真实 Expand 成功、无 Unsupported、保留普通入口并验证 source map。
不把任意错误作为通过，不删除负例，也不扩大语言接受范围。

限定验收运行 zil-expand 的 generics、attributes 及 impl094_generic_places 测试，
相关测试目标的 Clippy 和格式检查。原 workspace 失败收据保留；最终完整 workspace 由
集成方重跑，非作者审核锁定的交付提交。B0、完整语言和产品状态不因本测试迁移升级。
