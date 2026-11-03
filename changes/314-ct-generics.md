# 314：Type/Syntax 与普通 Copy 泛型

依据 `_zil_design.md` §16.1、§16.7、§18 的普通 Copy 与 where Copy 规则，本片在现有 CT artifact 源归因器内接通普通自由函数的有序类型参数及具体纯 struct 实例。源入口仍用普通调用、`::<...>`、构造及 Type/Syntax 结果；不修改 313 的 Expand/driver 入口或公共 schema。

定义期按每个真实 binder 的独立不透明 Copy 类型检查未调用 CT 泛型正文。该内部检查不产生可执行 artifact，不把某个具体 i32 代入来推定泛型能力；Copy 不授予字段、算术、比较、callable 或运行期资源权限。内联 Copy 与当前函数直接 binder 的 `where T:Copy` 幂等归并；其它 bound、const/lifetime、泛型 enum 暂仍明确拒绝。实际实例再次核查普通 Copy 结构；Type/Syntax 本身以及含它们的记录不能充当普通 Copy 类型实参。

函数实例键包含真实声明及完整有序规范化类型实参；记录实例键包含定义模块、原声明和全部实参，包含 phantom 参数。同布局不同名义类型保留独立身份，透明别名归一。CT raw 沿用既有独立函数/记录 schema ID；definition 保留原词法声明以维持反射可见性，源码重新归因和现有独立核继续认证实际 artifact。

新增约束扫描、绑定、类型复制/比较、实例查找、符号定义检查及队列增长先从同一请求余额收费。缓存命中仍扣比较工作；定义检查消耗保留，内部检查产物丢弃；不增默认额度。无泛型旧路径保留原费用；必要的类型正确性补检不另收额外预算。

限定验证使用已有 `impl260_compiler` 及 Type/Syntax 相关目标，保留 pending 两原源，覆盖推导/显式实参、完整身份、未调用反例、跨 binder、资源/地址拒绝、独立 raw/Wire/VM 和 N/N−1/fresh。正式全量、原生 O0/O2 和产品四目标由集成/消费者单独验收，本片未运行时不登记通过。

## 本轮实现与限定实测

以现存 299 源为起点重新实现，未恢复/复用旧 302/303。普通 struct Copy bound 的运行期消费者由 320 独立交付；本片不修改 Expand 或 driver lib 入口，也不删原泛型记录声明规避运行期检查。

内部不透明 binder 检查覆盖 CT-only 未调用定义，也覆盖从 CT 调用图才发现的普通纯值泛型 helper；即便调用时具体实参为 i32，`T:Copy` 的 `x+1` 仍被拒绝。不同 binder 的占位类型不同；匿名期望类型构造、公开字段、比较、转换和 callable 不从 Copy 获得能力。实际代码仍经独立 CT 核。检查产物与具体实例分开，查询/反射权限保持真实声明的词法上下文。

预审发现并保留的回修包括：匿名 `let x:T={}` 原可越界；记录的数组类型实参原被过严拒绝；别名的 RHS 原可捕获调用方 binder；`Type`/`Syntax` 同名 binder 原会被拼写分类剪掉。最近反例和实际成功控制均在 `impl260_compiler`。外层含 Type 字段的普通 struct 仍有既有运行期 Unsupported，未冒充成功控制；其类型 binder 遮蔽用真实 raw domain 分类控制。

限定命令（环境先 source `/workspace/.zil-env/env.sh`，`CARGO_TARGET_DIR=/workspace/.zil-env/target314 CARGO_BUILD_JOBS=1`）：

```text
cargo test -p zil-driver --test impl260_compiler --test impl260_member_owner --test impl261_lock --test ct_unchanged_view -- --nocapture
cargo clippy -p zil-driver --test impl260_compiler -- -D warnings
```

结果：35/0、3/0、7/0、4/0，均无忽略或过滤；Clippy 通过。原 pending `ct-type-generic-function.zil`、`ct-type-generic-record.zil` 原字节到普通运行期 Verify；多参数 Config 与 Syntax<Expr> 也到 Verify。真实源码的 generic raw→编码→解码→同源 authority→独立 Verify→VM 返回 7；替换 schema 或实际函数类型被源认证拒绝。

内部源归因边界为 work N=994、nodes N=267、temporary bytes N=1753、instances N=4、depth N=6；各自 N 成功、N−1 ResourceLimit、fresh 重放完全相同。两次不同实际类型查询的完整请求 work N=924，cold/warm/fresh N 成功、N−1 ResourceLimit。旧 alias work N=40/104、routing N=45 保持。新检查消耗累计保留，逻辑工作不因命中或内部检查产物丢弃而返还。

| Rust 四目标 | 本片实证与限制 |
| --- | --- |
| 同类安全 | 符号 Copy 能力、真实词法 binder、完整实例隔离和独立 CT 核均有源正反控；完整语言与异步安全未完成。 |
| 运行与内存 | 生成结果进入现有运行期表示，无新增 RC/锁/堆盒；本片未运行原生、未测同语义 Rust 性能/RSS。 |
| 表达与维护 | 原两份自然源码与多参数 Type/Syntax 组合贯通 Verify；完整框架维护成本未测。 |
| 编译成本 | 新旧精确预算及两 query 冷暖累计控制通过，默认限额保持；未测编译提速或 Rust 成本优势。 |
| 至少一项突破 | 原合法 Type/Syntax × 普通泛型源组合从拒绝推进到实际 CT VM 与普通 Verify；产品四目标合取未建立。 |

不运行正式全量或 push。通用 const/lifetime/trait/callable 泛型、泛型 alias/enum 与 CT 运行期地址/资源仍未开放；完整 1.4、ZUI、B0 和既有 product-gates 不因此完成。原生 O0/O2/失败清理由集成与消费者独立记录，不计入本片成绩。
