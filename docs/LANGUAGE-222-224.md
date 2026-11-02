# 施工8：有限源级生成、Product Store 与共享借入

本轮从 GitHub `main` 的 `a8e8831a9e28ac2ec83be6eac740a8f04a00af7d` 接续施工7，实施222–224。本文记录有限实现、返修、独立审查、组合与正式验收；每层结果保留其实际身份。B0为Doing，181为施工；完整语言1.4与完整ZUI保持NotSatisfied，Rust产品门和原生Windows为NotRun。

## 新增的实际能力

| 切片 | 已贯通的行为 | 保留的边界 |
| --- | --- | --- |
| 222：源级 `#for` | 声明级普通函数与语句级展开；同型 Unit/bool/u8/u16/i32/i64 显式数组和同整数类型半开区间；经独立 CT 核验与 VM 求值，逐轮卫生标识、原配置快照、依赖和累计预算；普通类型中数组长度等既有标量槽可使用迭代值 | 不支持一般 Type/Syntax 生成、`#emit` 或 struct/enum/trait/impl/type alias/package 声明生成；生成结果继续经过普通语义和安全核，普通 `defer` lowering 仍 Unsupported |
| 223：完整 Product Store | 有限完整嵌套 Product 可作为动态 Store 元素，真实内联布局、push/pop/reserve、失败载荷返还与递归声明序清理；普通 `T:StoreElement` helper 传递完整元素身份 | Product 内 Store、递归 Store、树内 callback、拥有数组、任意 owning Sum、拥有捕获 `Store<Product>`、Product 独占元素窗口仍未开放；只支持精确 `Product!Unit` / `Unit!Product` 协议壳 |
| 224：共享 `&Product` | 普通参数、局部地址、直接 `&root.child`、同步转借及嵌套 Copy/有限 Copy 用户资源头读取；真实根来源、完整责任、活跃 loan 和 NLL；零存储普通头不继承 Store 的非零元素限制 | 活跃共享借入期间禁止相交 move/take/drop/replace；不开放可变嵌套 Product、借入返回/逃逸或 `&Product` 闭包捕获（含现有立即同步调用形状），共享地址不制造可变 Desc |

没有新增隐式 Box、RC、锁或全局表。Verify 继续从原始指令独立重建资格、来源和责任，不调用 Types/Analyze 的求证函数。schema/Wire 及默认限额、`validation/product-gates.json` 未改。

生成大括号不插入运行期作用域；return、break/continue和资源清理沿已支持普通程序的语义执行。defer只保留生成遍历，普通Core lowering仍为Unsupported，独审未报告defer机器执行通过。同型range相等或降序为空，零次迭代仍要求模板可解析；ConfigReady保持封闭快照，生成声明不能反馈后续CT查询。

## 独立审查实际发现并修复的问题

1. **222 的类型内部标量槽遗漏替换。** 原实现替换普通表达式，却遗漏参数、返回、局部、闭包参数和泛型类型实参中的数组长度。修复统一遍历这些既有 TypeRef 槽；原失败输入保存在 `examples/mainline/ct_for222/review_repair/`。审查还发现同名局部值会使展开后的 record 构造器丢失原类型绑定；普通单源 API 的手写对照成功，CLI 工程入口的既有差异另行记录。修复566b保持名义构造器的类型绑定；同一手写/生成API输入均成功，原五份类型槽输入和独立清理程序8次O0/O2执行通过。入口差异与原Fail分别保留。
2. **223 扩大了普通可变地址资格。** 非作者把合法未用 `&mut Leaf` 输入一致改为嵌套 Product，保持真实字段索引、SSA 与 Hold，并对变异后模块重算指纹，原 raw/Wire 被错误接受。返修分离普通地址资格与 Store/精确 Unit 协议资格，223 自身即能拒绝该越界，不依赖 224 的守卫补救。
3. **224 的直接子包地址没有机器实现。** 原字节 `&root.child` 通过 Verify 后被 LLVM 后端旧资格拒绝。返修仅为已核的不可变字段地址发射真实父布局上的 GEP，不复制拥有值、不增加权限；同一原输入及直接资源字段近邻已分别执行 O0/O2。

原失败、跨工作树旧缓存污染和修后重放各自保留身份。旧缓存污染记作 InvalidEvidence，不计代码失败或通过；作者、独审和集成的重叠执行不累加成新覆盖量。

## 正式 GitHub 验收与历史尝试

| 验收身份 | 实际提交 | GitHub run |
| --- | --- | --- |
| PR 完整验收 | `4d3df5891aafebdd8337337180a7e2559daec361`（实际 checkout） | [run 41088376020](https://github.com/zil-lang/zil/actions/runs/41088376020) |
| main 合并后完整验收 | `ccae8c60c96a2c174b137b2fab084e9e8a97a02f`（实际 merge） | [run 41106864272](https://github.com/zil-lang/zil/actions/runs/41106864272) |

最终 main 默认 workspace **2861 passed**，reference workspace **2826 passed**；正式通过还要求其余必需 workflow 步骤完成并成功、原始证据上传完整，不能只看默认 workspace 一项绿色。

PR9 最初合入 `39e52ef784d41f5744b909a9e6702ab16c1b1227` 后，[main run 41101248493](https://github.com/zil-lang/zil/actions/runs/41101248493) 保持 **Fail**：默认2860/1/0，reference2826/0/0。唯一失败发生在 `review223_machine` 向子进程stdin写入时的 `BrokenPipe`，早于退出码/stdout/stderr的采集；该次消费者输出没有被验证为通过。原 `entry_error.zil` 不读取stdin，修复 `e38ce27fda432e7a55b4a203f4fd047756d9512e` 将无输入样例改用空stdin且不写入；需要模式字节的样例仍严格检查写入。相对PR候选，生产源码、规范和源fixtures保持原字节，退出/输出/分配清理oracle不变。上表最后main是非作者复核该测试驱动修复后的新合并身份，没有把原Fail改名为Pass；PR与最后main的完整文件树不同，差异限于该测试驱动、223派工和修复证据，见 `evidence/integration/222-224/closeout/POSTMERGE-REPAIR-BINDING.json`。

| 历史尝试 | 必须保留的状态 |
| --- | --- |
| run `41082643919` | Fail。默认与 reference 各在已执行部分出现 64 通过、1 失败、0 忽略，均未完成整仓；原 201 诊断预期失败及后续限定修复分别保留。 |
| run `41084362447` | 默认完整 2858 通过、3 失败、0 忽略；reference 为 Incomplete，后续门为 NotRun。reference 未收尾的原因尚未证实。 |
| 候选 `91609072b0687d944fc25489b5536e8f7a7e2106` 的临时运行 | Superseded；由进一步分离归档阶段的补丁替代，不作为最终 Pass 或 Fail。 |
| 候选 `e3e231b493c4d64f04d01beaf0c394a1f148dbb5` / run `41088078934` | Superseded；由补齐目录大小写冲突检查的后继候选替代，不作为最终 Pass 或 Fail。 |

历史失败与限定返修收据分别见 `evidence/integration/222-224/ci201-repair/`、`ci192-214-repair/`；归档修补见 `ci-native-phases/`、`ci-native-paths/`。容量修补为默认/参考阶段分别生成完整哈希与权限清单，上传独立native原始artifact成功且复核清单一致后释放对应产物；隐藏文件纳入，符号链接与大小写冲突拒绝。测试范围、默认限额、门及45分钟超时未削弱。

## 本地集成与证据入口

本地 clean `a57516d138f85c8f352bec5d440c5c22af259b14` 的 11 组受影响回归为 **132 passed / 0 failed / 0 ignored**。最终共同实现 `4d7bbb762eb34987ffd05c9ab23a6ad6bc6fae41` 再跑5组受影响CT/组合回归，**19/0/0**；这两轮重叠，不相加成新覆盖。三名非作者按各自范围对该共同提交裁决Pass，精确回执为 `evidence/reviews/222-224/REVIEW-222.json`、`REVIEW-223.json`、`REVIEW-224.json`。格式及计划一致性自检另行通过。规范输入注册表随后按当前正文重新生成，未改生产源码。精确命令、完整输出、原始产物和首次组合失败见 `evidence/integration/222-224/local/RESULT.json`。

新组合程序在已有 Product 类型上，用 `#for` 展开元素构造和 Store 操作，经共享子视图和普通函数读取；最后一次使用结束后可扩容、弹出，O0/O2 均按预期输出 `BA`。直接子包共享读取后移动完整值输出 `A`。保持视图活跃时的 reserve/push/pop/consume 四条近邻均由 Verify 以 `E-BORROW-CONFLICT` 拒绝。

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

本轮新增机器证据属于Linux x86_64的相应受测路径；原生Windows为NotRun。历史Win64/Wine结果保留原身份，不计作本轮原生Windows验收。

## Rust 四目标与下一范围

| 目标 | 本轮实证 | 尚未证明 |
| --- | --- | --- |
| Rust 同类安全 | 有限生成、完整元素资格、独立 raw/Wire、真实借入来源及机器失败清理；三项非作者发现按原输入闭合 | 完整语言安全、一般逃逸、挂起、取消及并发 |
| 运行与内存同级 | 递归内联布局、真实 LLVM API O0/O2、分配失败后责任平衡；223 作者消费者 74 次/38 次实际失败均 live0 | 同语义 Rust 运行时间、RSS 或尾延迟；334B 仅是该消费者的请求 payload 峰值 |
| 表达与维护更简单 | 生成结果可进入普通 Product Store，完整拥有组件能经普通函数同步共享读取 | 完整 ZUI、开放异构、长期回调关闭/注销/重入和实际维护成本比较 |
| 编译成本更低 | 新形状有精确 N/N−1/fresh 控制，具名旧预算与默认限额保持 | 本线没有相对 Rust 编译性能结论；编译专项仍在独立研究分支 |
| 实质进展 | 三项原来不贯通的通用语言能力现可组合执行 | 四目标产品合取及可重复产品净突破 |

后续继续完整 Type/Syntax 与声明生成、拥有容器的更深组合、一般 trait/关联项/life/HRTB/lending/dyn、长期外部生命周期及重入/注销/关闭，以及挂起/取消/并发。已有有限片通过不移除这些终态义务，也不把它们全部反推成每个同步手写ZUI起步程序的前置条件。

B0的公共schema.v1和公共接口交付不等于已有内部schema/Wire27；181的有限历史记录也不等于完整1.4 Done。历史handoff缺项保持原身份：218–221 closeout只在metadata范围为Pass，其完整历史handoff为Fail、1138项历史错误/0项新增错误；这些是旧收据计数，不是本轮新检查。稀疏checkout未展开历史证据可能额外报缺文件，不能重标历史成绩。相对Rust的运行、内存、维护和编译成本对照仍未建立，四目标合取与可重复产品净突破未成立。
