# 342 普通泛型借用与终态组合

施工基线：`77eea5bde60b1737b1a7f885ff549ed17308c3de`，派工权威：主树 `.task/impl/342.md`。

## 先行变更提案：定义期真实窗口

普通未实例化模板目前只检查可变 place 的类型资格；现有定义期 loan engine 仅在 owning callable State profile 中开启。最近反例为 `fn bad<T:Copy>(p:&mut T)->T { let a=&mut *p; let b=&mut *p; *a }`（即使从未调用，第二次独占借入也必须使旧 `a` 无法再次使用）。等价有用正例在旧 `a` 最后使用后才建立 `b`，允许原父引用在 child 最后使用后恢复。

实施拟复用现有计费的 loan identity / parent / invalidation / prepared argument / CFG join / backedge 引擎，覆盖所有普通模板；从实际参数引用模式建立独立 referent root，不从 life 名称、同尺寸或具体实例制造权限。普通局部 place、读写、移动、重借和调用窗口统一进入该引擎。type/life binders 的名字空间仍分开；有 life 时返回来源继续用真实参数索引核对声明上界，mutable result 还需真实 exclusive 来源。无 life 的模板同样不得丢弃别名错误。

随后在同一完整符号窗口证明上准入 `T:Copy + life` mutable 输入/结果。Analyze 与 Verify 仍各自重建具体 CFG 来源、返回权限和调用窗口；Core/Wire 现有来源合同不赋予证明位。新增扫描、loan、父链、根集与分支复制在扩张前累计既有请求预算，不增加默认限额。既有正例、最近反例、未调用定义、raw/Wire、实际 O0/O2 和 N/N-1/cache 依适用范围验证。

HRTB/借用 FP、借用存储/lending、显式 dyn 和一般 §5.2 状态后续按实际表示依赖继续实施；此阶段备份不代表该项终态完成。任何公共 schema/AST/Wire 变化先交由集成方发布，不能通过删除 Unsupported 或新增权限位替代证明。

## 首个实际里程碑：未使用普通模板别名核验

移除 State-only lane suppression，同时扩展来源演算：每个实际 Reference 形参由其真实 mutable 模式建立外部 referent root，root 与本地指针槽分离；普通值 local 为实际存储根。普通 local 的值读取、index 读取也参与窗口失效，不能靠缺失访问绕过已有贷款。父子链、同时实参窗口、分支版本化、循环 backedge 与预算复制复用原引擎。

实际限定命令（source `/workspace/.zil-env/env.sh`；独立 `CARGO_TARGET_DIR=/workspace/.zil-env/target342`、jobs1、debug0、incremental0、专用 TMPDIR）：`cargo test -p zil-driver --test generic_regressions --test cross_function_borrows --test impl197_mixed_state --test review184_storage --test impl200_independent --offline -- --nocapture`，先前单项失败按下列来源分账后分别完整重放：generic_regressions33/0、cross_function_borrows37/0、impl197_mixed_state14/0、review184_storage4/0、impl200_independent10/0。不是正式全量。

原 retained `mutable337/known-incomplete-unused-generic.zil` 字节不改；从未调用和有调用均在 Expand/SafeReject/E-GENERIC-ARGUMENT 拒绝。有调用路径原断点为 Verify/E-BORROW-CONFLICT，旧用例仅迁移预期阶段/诊断。首次回归运行保留该预期失败。另一次旧 exact budget 2358→2356 的暂时少费由保留 callable binder 访问收费修正，旧2358边界不变；不以调高限额处理。

331原源Expand N757→779，来自新真实参数贷款建立；其它符号/别名源本轮 N1414/275/1391，各N−1 ResourceLimit/fresh N一致。322原源99/95/714/993、337具体源143/124/1178/4818保持。现有借用目标含真实两核、Wire、原生、缓存回归；本里程碑新增2测试覆盖8份未用近邻反例与6份合法最后使用/分支/循环/独立参数正例。

Rust四目标：未用泛型别名责任增加可重放安全证据，完整语言证明仍待后续；沿既有地址ABI，没有新增隐式分配/RC/锁，未测同语义Rust运行/RSS；普通泛型无需实例化才能发现此类错误，尚无框架维护成本数据；新loan来源与复制实际累计收费，没有编译速度比较。实质增量为真实未使用普通mutable模板的既有漏洞闭合，不代表产品四目标合取或产品净突破；原product-gates/B0/完整语言状态不变。
