# 施工8：有限源级生成、Product Store 与共享借入

本轮从 GitHub `main` 的 `a8e8831a9e28ac2ec83be6eac740a8f04a00af7d` 接续施工7，实施 222–224。本文当前记录已完成的有限实现、返修和本地集成；完整 GitHub 验收与合入以最终验收收据为准。B0/181、完整语言 1.4、全部 ZUI 前置及 Rust 产品门仍未完成。

## 新增的实际能力

| 切片 | 已贯通的行为 | 保留的边界 |
| --- | --- | --- |
| 222：源级 `#for` | 声明级普通函数与语句级展开；同型 Unit/bool/u8/u16/i32/i64 显式数组和同整数类型半开区间；经独立 CT 核验与 VM 求值，逐轮卫生标识、原配置快照、依赖和累计预算；普通类型中数组长度等既有标量槽可使用迭代值 | 不支持一般 Type/Syntax 生成、`#emit` 或 struct/enum/trait/impl/type alias 声明生成；生成结果继续经过普通语义和安全核 |
| 223：完整 Product Store | 有限完整嵌套 Product 可作为动态 Store 元素，真实内联布局、push/pop/reserve、失败载荷返还与递归声明序清理；普通 `T:StoreElement` helper 传递完整元素身份 | Product 内 Store、递归 Store、树内 callback、拥有数组、任意 owning Sum、Product 独占元素窗口仍未开放；只支持精确 `Product!Unit` / `Unit!Product` 协议壳 |
| 224：共享 `&Product` | 普通参数、局部地址、直接 `&root.child`、同步转借及嵌套 Copy/有限 Copy 用户资源头读取；真实根来源、完整责任、活跃 loan 和 NLL；零存储普通头不继承 Store 的非零元素限制 | 活跃共享借入期间禁止相交 move/take/drop/replace；不开放可变 Product、借入逃逸或长期捕获，也不从共享地址制造可变 Desc |

没有新增隐式 Box、RC、锁或全局表。Verify 继续从原始指令独立重建资格、来源和责任，不调用 Types/Analyze 的求证函数。schema/Wire 及默认限额、`validation/product-gates.json` 未改。

## 独立审查实际发现并修复的问题

1. **222 的类型内部标量槽遗漏替换。** 原实现替换普通表达式，却遗漏参数、返回、局部、闭包参数和泛型类型实参中的数组长度。修复统一遍历这些既有 TypeRef 槽；原失败输入保存在 `examples/mainline/ct_for222/review_repair/`。审查还发现同名局部值会使展开后的 record 构造器丢失原类型绑定；普通单源 API 的手写对照成功，CLI 工程入口的既有差异另行记录。修复566b保持名义构造器的类型绑定；同一手写/生成API输入均成功，原五份类型槽输入和独立清理程序8次O0/O2执行通过。入口差异与原Fail分别保留。
2. **223 扩大了普通可变地址资格。** 非作者把合法未用 `&mut Leaf` 输入一致改为嵌套 Product，保持真实字段索引、SSA 与 Hold，并对变异后模块重算指纹，原 raw/Wire 被错误接受。返修分离普通地址资格与 Store/精确 Unit 协议资格，223 自身即能拒绝该越界，不依赖 224 的守卫补救。
3. **224 的直接子包地址没有机器实现。** 原字节 `&root.child` 通过 Verify 后被 LLVM 后端旧资格拒绝。返修仅为已核的不可变字段地址发射真实父布局上的 GEP，不复制拥有值、不增加权限；同一原输入及直接资源字段近邻已分别执行 O0/O2。

原失败、跨工作树旧缓存污染和修后重放各自保留身份。旧缓存污染记作 InvalidEvidence，不计代码失败或通过；作者、独审和集成的重叠执行不累加成新覆盖量。

## 集成与证据入口

本地 clean `a57516d138f85c8f352bec5d440c5c22af259b14` 的 11 组受影响回归为 **132 passed / 0 failed / 0 ignored**。最终共同实现 `4d7bbb762eb34987ffd05c9ab23a6ad6bc6fae41` 再跑5组受影响CT/组合回归，**19/0/0**；这两轮重叠，不相加成新覆盖。三名非作者按各自范围对该共同提交裁决Pass，精确回执为 `evidence/reviews/222-224/REVIEW-222.json`、`REVIEW-223.json`、`REVIEW-224.json`。格式及计划一致性自检另行通过。规范输入注册表随后按当前正文重新生成，未改生产源码。精确命令、完整输出、原始产物和首次组合失败见 `evidence/integration/222-224/local/RESULT.json`。

新组合程序使用 `#for` 生成 Product，放入 Store，经共享子视图和普通函数读取；最后一次使用结束后可扩容、弹出，O0/O2 均按预期输出 `BA`。直接子包共享读取后移动完整值输出 `A`。保持视图活跃时的 reserve/push/pop/consume 四条近邻均由 Verify 以 `E-BORROW-CONFLICT` 拒绝。

旧 171/175/206 的原源码与 fixture/MANIFEST 保留，仅迁移被新合同改变的预期：合法共享读取和完整 Product Store 原资格源码晋升到 Verify 成功；原非法 canonical Store 仍被 SafeReject；活跃共享借入期间替换由前端 Unsupported 前进为 Verify 借用冲突拒绝。没有删除旧用例来获得通过。

| 证据层 | 定位 |
| --- | --- |
| 222 作者与范围 | [LANGUAGE-222.md](LANGUAGE-222.md)，`evidence/impl/222/author-repair/`、`evidence/reviews/222/` |
| 223 作者、原独审失败与返修 | [LANGUAGE-223.md](LANGUAGE-223.md)，`evidence/impl/223/author/`、`address-admission-repair/`、`review-scope-audit/` |
| 224 作者、机器遗漏与返修 | [LANGUAGE-224.md](LANGUAGE-224.md)，`evidence/impl/224/author/`、`author-repair1/`、`author-history/` |
| 当前组合与正式验收 | `evidence/integration/222-224/`，主树 `.task/impl/222.md`–`224.md` |

## 施工7原始未完项重查

最终组合CLI的真实构建提交为 `6b0d974bedfb40c93eb36e491ceb18a138718dac`，生产源与4d7相同。29份原输入及原manifest的42项哈希均未改；恰好29次check、0次native，结果由施工7的13成功/14 Unsupported/2原语法无效，前进到 **15/12/2**，旧成功零回退。

晋升的是 `product-store-element` 与 `product-error-union`；后者原源是未调用的 `fn make()->void!Root`，只证明精确Unit/Product形式到达Verify，不证明任意拥有Sum或机器清理。旧矩阵没有 `#for` 和普通共享Product探针；222/224的实际增量按本片独立源码与机器证据评估。

剩余12份有效输入仍分别缺：直接捕获借入slice、原泛型F原样返回、闭包直接资源返回、拥有Package闭包参数/返回、optional表面类型、具名lifetime泛型、Product拥有Store、含Store的外层Store元素、拥有enum、关联类型trait、合法dyn参数、合法 `syntax use`。它们保留具体拒绝阶段和原失败。`dyn-parameter` 与 `syntax-extension` 两份原Parse无效输入继续排除能力判断。完整逐项结果见 `evidence/integration/222-224/readiness/READINESS.json` 和 `SUMMARY.md`。

## Rust 四目标与下一范围

| 目标 | 本轮实证 | 尚未证明 |
| --- | --- | --- |
| Rust 同类安全 | 有限生成、完整元素资格、独立 raw/Wire、真实借入来源及机器失败清理；三项非作者发现按原输入闭合 | 完整语言安全、一般逃逸、挂起、取消及并发 |
| 运行与内存同级 | 递归内联布局、真实 LLVM API O0/O2、分配失败后责任平衡；223 作者消费者 74 次/38 次实际失败均 live0 | 同语义 Rust 运行时间、RSS 或尾延迟；334B 仅是该消费者的请求 payload 峰值 |
| 表达与维护更简单 | 生成结果可进入普通 Product Store，完整拥有组件能经普通函数同步共享读取 | 完整 ZUI、开放异构、长期回调关闭/注销/重入和实际维护成本比较 |
| 编译成本更低 | 新形状有精确 N/N−1/fresh 控制，具名旧预算与默认限额保持 | 本线没有相对 Rust 编译性能结论；编译专项仍在独立研究分支 |
| 实质进展 | 三项原来不贯通的通用语言能力现可组合执行 | 四目标产品合取及可重复产品净突破 |

后续继续完整 Type/Syntax 与声明生成、拥有容器的更深组合、一般 trait/lending、长期外部生命周期以及挂起/取消/并发。已有有限片通过不移除这些终态义务。历史 handoff 缺项保持原记录；稀疏 checkout 未展开历史证据会额外报告缺文件，不能把这种检查当作历史验收通过。
