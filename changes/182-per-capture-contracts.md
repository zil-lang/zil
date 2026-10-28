# 182：有限混合捕获的逐来源、逐效果合同

## 原因与规范

设计 1.4 §3.3–3.4、§4、§5.3、§9.7 要求捕获使用真实来源与实际访问效果。现有实现遇到模块中任意资源头窗口便统一拒绝 Desc/Address 混合捕获；没有资源头时同一安全程序可接受。Desc-only callback 及 helper 内部 callback 又被统一视为写，不能表达已经证明只读的来源。直接删掉保护条件会丢失当前调用、独占权限和隐藏写的检查。

本片针对已经具备完整机器路径的有限资源 Desc、Copy 字段 Address 和具体同步 callback，建立逐捕获合同。它是语言基础能力，不实现 ZUI 产品，不修改默认预算或正式性能门。

## 接受行为

- 捕获保持下标、真实来源与独立的地址贷款；不因其它函数或模块出现不相干资源头而改变准入。
- 从真实可达 CFG 重建 Desc 的实际读、写及失效上界，按捕获下标和显式参数实例化。仅有 Fn 不说明纯，未知效果不能当作空集合。
- 共享地址与同源真正只读 Desc 可以组合；同源 Desc 写与共享或独占窗口冲突，来源独立的混合读写可接受。
- 当前调用把捕获与本次实参纳入同一个窗口检查，不能仅看调用后的活性。环境位置的借用不授予捕获对象写权限。
- 普通 helper、两层转发及 Desc-only helper 内创建并调用 callback 均传播实际效果。保留公开 helper 能力上界，不把上界冒充发生的写，也不读取未经核验的前端证明。
- Hold、Call 与 Cleanup 使用同一真实来源；最后一次调用后释放普通借用窗口。FnMut 重借和 FnOnce 剩余清理沿现有贷款及责任规则，不增加隐藏保活、运行时借用表或 RC。

## 仍明确拒绝

未贯通的 callback 显式 Address 参数、helper 的 Address 附加参数、直接资源头/Store 元素窗口捕获、借用 ByteBuffer/Slice 捕获、借用闭包逃逸与异步恢复不随本片放开。它们不是空效果；原前置诊断继续生效。强读与 Store/ByteBuffer 元数据读取的既有权限规则不改变。

## 实施与公开接口

Types 已能产生本片所需具体环境和调用指令。Analyze 与 Verify 分别保持/重建按捕获下标的来源和效果；Verify 不调用 Analyze 或 Types 求证。公开 Hold 来源可继续使用既有保守投影，内部精确捕获行不作为外部授信。

目前不需要新增指令、Wire 字段、运行时表示或公共 schema；保持 schema 22。`CallableParameterContract.writes` 仍是原能力上界。如实现发现必须对外发布精确 effects，将先向集成者提交生产者、消费者和迁移需求，不在本分支修改公共 schema。

预计生产写集：`zil-analyze/src/holds.rs`、`resources.rs` 及其效果依赖子模块；`zil-verify/src/borrows.rs`、`borrows/{roots,loans,buffer_effects}.rs` 和新的私有捕获事实子模块、`resources.rs`；必要的双方 helper 合同消费位置。仅当保留现有前置准入需要时修改 Types/Verify 的 closure/helper 分类。Lower/LLVM 沿用既有真实路径，不计划改机器 ABI。

## 验收

保留调查用原始 probe：无 head 的安全混合组合、加入无关 head 的同一组合、只读/写 Desc 与共享 head、两层 helper 内只读 callback、未支持的显式 Address 参数。新增同源混合只读正例、同源写最近反例、不同来源写正例、多个 Desc 捕获的单独读写、分支和循环传播、FnMut 重借、最后调用的当前窗口、helper 隐藏写、正常/错误/FnOnce 清理及源码跨模块消费者。

独立 raw/Wire 变异不得通过改变捕获索引、来源、调用入口或候选 Hold 将真实写降为读、抹掉当前使用或制造不交。O0/O2 从实际 ZIL 源生成并执行，核对输出、退出与清理；不以手填 IR 或测试数量代替终态结论。旧 171 混合捕获期待只按逐例授权迁移，原输入保留。

## 施工身份

- main：`aba4ef7`；180 源码及证据 `495893c7ca91f662a9c03e5ef69503ade4dc7e9e`，受测源码 `74254d8da12004519ca342b1c0602a5a90831c44`。
- 179 依赖候选 `aeab717` 已在准备分支合流，仍由集成者完成交叉独审与合入；本片准备基线 `4e45dd4` 不代表主线已接受这些依赖。
- design SHA256：`fe11dddb8fd64aebd773e90dea5ceaa545da3e41e280fd939806c4dc0ea212e2`；normative SHA256：`1cf7aadb8800c31436f158ee3cdf78bcea330ed2bfafc29112b8d11211655a6e`。
- 施工/测试使用独立工作树 `wt182` 和 target `targets178/author182`，离线 locked、并行 2；正式全量、公共接口、工单状态与合入由集成者持有。
