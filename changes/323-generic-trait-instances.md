# 323：有限具体 trait 实参与普通 bound 消费

依据 `_zil_design.md` §18.1 与 316，接通 `trait Read<R>{fn read(self:&Self)->R;}`、具体本地 Self 的 `impl Read<i32>`，以及普通函数 inline/where 的 `T:Read<i32>` 消费。关联类型不先行：本片直接扩展现有静态调用，不引入投影或公共 IR。

实例保存真实 trait 声明身份、完整模块名、全部有序规范化实参及实际 Self；透明别名归一，同布局不同名义与 phantom 参数不合并。名义 bound 使用结构化实例引用，与保留 Copy/StoreElement 能力分开。Copy/Transfer/Share/Send/Sync/Drop/Fn 族的原排除不放宽，空 trait/impl 不造结构、清理或线程权限。

首片 trait binder 为普通类型，无 bound 或直接 Copy 前提；实际实参限已有有限普通 Copy 类型。开放参数 `Read<U>`、supertrait、关联项、default/泛型方法、泛型/条件 impl 暂 Unsupported，不丢弃前提。未用 trait 签名按真实 binder 检查，未用 impl 仍核完整方法集、前提、receiver 与代入后签名；泛型正文只消费已有声明 bound，最终静态代码继续经过 Types、Analyze 和独立 Verify。

同 Self 的不同 Read 实参是不同合法 impl 头，各有确定独立方法目标；唯一 generic bound 选择真实目标。直接方法调用遇到多个候选在调用处报歧义，不把声明当作重叠。相同完整实例的重复 impl 拒绝；不新增 specialization、安全失败回溯或外来 Self 权限。

所有索引、类型键/比较、替换、源调用改写和队列增长先收费并累计到请求；命中继续收费，默认限额不增。实例计数在已计费的插入时累计，后续读取不重新扫描全部实现。新 trait 的 names→alias→engine、模块 normalize/restore、lower/static conformance 和最终 publish 传递实际 work/node 余额；无新 trait 入口沿既有兼容路径。字段/方法碰撞和声明/impl 成员查找在每次访问前收费。新 trait 路径不擦除 Reference lifetime 或 CoreFunction.life；与 322 的公共字段及入口由 root 串行整合。未触及新实例及下述 alias 签名正规化缺陷的既有路径保持原费用。

限定验证复用已有 trait 测试目标，覆盖原探针、跨模块、多个实参/同布局/phantom、同 Self 实例、unused坏定义、前提/签名/可见性、N/N−1/fresh/冷暖和真实 O0/O2。正式全量、公共规范/schema及集成由 root 管理；未运行项目不登记通过。

## 实现与有限实测

名义 bound 现在使用结构化声明/参数 key。普通 Copy 与 StoreElement 保留独立枚举项，不把用户 trait 名字当作结构资格。泛型正文在符号检查时记录实际 bound 选定的成员位置；具体实例改写到该 trait 实例的静态目标。同 Self 三份同布局/phantom 实参得到三个不同 Core 方法目标，LLVM 中仍是普通 `ptr` 接收者及 `{ i32 }` 等既有值表示，没有运行时字典、额外环境或隐式分配。

`Read<Box<i32>>` 的预期方法签名由真实 trait 声明、完整实参和 Self 产生，再经过普通类型专门化。已有私有 Products 请求对象保存这份声明派生签名，并绑定 trait key、Self、impl/member 与原声明成员 span；静态 conformance 不用实际 impl 签名回填预期。`Box<B>`/`Box<i64>` 冒充 `Box<A>` 返回得到 `E-TRAIT-SIGNATURE`。源码-derived Core/Typed、Analyze 与独立 Verify 仍逐个检查实际调用和权限。

life 公共迁移来自集成方 `8e46a4615`，已合入本片；方法重命名保留其余 CoreFunction 字段，签名替换保留 Reference lifetime。本片没有增加 trait 公共 IR、Wire 标签或运行时机制；不据公共字段存在宣称 life 实现完成。

保留的历史失败及迁移：

- 原 `trait-type-argument.zil` 源字节已通过普通静态原生调用。同 Self 的 `Read<i32>`/`Read<i64>` 声明共同合法，inline/where 各自选中目标；直接调用在多候选处报歧义。
- 原 147 未用 `trait Read<T>` 被登记 Unsupported，现原样晋升；关联项/default 等原负控保留。
- 原 231 的合法 `Read<i32>` where 应用及 alias 方法签名曾被登记拒绝。通用 alias resolver 现在完整遍历 trait 的词法签名，原 where/inline 两形式均晋升原生 O0/O2；错误 impl 的 alias 签名仍拒绝。未提供实际 trait 参数的原源保留，现报明确 arity 错误。其余旧负控与源字节不改。
- 新增检查覆盖未用 trait 的未知类型/重复 binder、嵌套 Copy obligation、未用 impl 的方法集/实例签名、同布局名义及 phantom 实参、别名归一重叠、名字/可见性和结构能力排除。实际实例不能补证定义期缺失的 bound。

环境先 source `/workspace/.zil-env/env.sh`，使用 `CARGO_TARGET_DIR=/workspace/.zil-env/target323 CARGO_BUILD_JOBS=1`。合入公共 life 后执行：

```text
cargo test -p zil-driver --test impl162_trait_bounds --test impl147_static_traits --test impl233_modules --test impl231_where_traits --test review231_independent -- --nocapture
cargo test -p zil-expand --lib trait_request_ledger -- --nocapture
cargo clippy -p zil-driver --test impl162_trait_bounds --test impl147_static_traits --test impl233_modules --test impl231_where_traits -- -D warnings
cargo clippy -p zil-expand --lib --tests -- -D warnings
```

driver 五目标结果分别为 22/0、12/0、9/0、14/0、8/0，共 65 项，无忽略/过滤；新增两个 ledger unit 控制 2/0（限定筛选，另 3 项未运行）。两组 Clippy 通过，修改文件 rustfmt 与 diff 空白检查通过。162 的新原生输入及 231 的两份 alias 形式使用实际 clang/LLVM 23.1.4、Linux x86_64 O0/O2，每次完整编译均有一次独立 Verify，机器程序退出 0。原生机器证据不记为 LLVM API、原生 Windows 或同语义 Rust 性能结果。

独审发现并保留的预算回修：`d626c5ad4`/`de33ad2ef` 的原 3797/656/5 只覆盖了局部上限，alias→engine 与下游默认 allowance 的重置使它不能证明累计计费。`a0adf510e` 将本片新 trait 入口全部接到同一剩余链，并补上新扫描逐访问费用；原结果不登记为最终累计通过。

修后包含同 Self 两个实际 trait 参数和两个 bound 调用的完整请求，work N=3797、nodes N=734、instances N=5；各自 N 成功、N−1 ResourceLimit，cold/warm/fresh 一致且查询退出后无活跃请求。另用真实源独立测量阶段费用：单源 alias+专门化 work=1235/nodes=365，项目 normalize+专门化 work=1489/nodes=326。各自精确额度可完成专门化，但不能再完成下游 public Expand；单源还断言完整费用包含真实 alias 与 engine 的费用之和，直接防止只取最大子 pass。旧 233 的无 trait、Drop、静态 trait work 硬边界 242/1568/1432 保持。新测试的重复重放不累计覆盖成绩。

| Rust 四目标 | 本片实证与限制 |
| --- | --- |
| 同类安全 | 完整名义/参数/phantom 身份、符号 bound、未用定义及实际调用经正反控与独立 Verify；完整语言和并发安全未完成。 |
| 运行与内存 | 普通静态目标与原值布局，clang O0/O2 实际执行；没有新增堆分配/责任协议，旧 resource 清理控制通过；未做 Rust 运行/RSS配对或新分配失败注入。 |
| 表达与维护 | 普通库可直接声明并消费 `Read<R>`，同 Self 多实例、泛型记录实参及跨模块组合已有原生结果；完整框架维护成本未测。 |
| 编译成本 | 新实例与冷暖累计额度有精确边界，默认额度保持；未证明编译提速或相对 Rust 成本优势。 |
| 至少一项突破 | 原拒绝的自然 trait 实参源及多实例普通 bound 调用推进到实际原生；产品四目标合取未建立。 |

不运行正式全量、不 push。开放 `Read<U>`、泛型/条件 impl、关联项、supertrait、默认/泛型方法、动态 trait 与跨线程能力仍未完成；完整 1.4、ZUI、B0 和既有 product-gates 不因此完成。独立审核绑定最终提交，由集成方记录。
