# 201预检：callback Store的责任、窗口与捕获依赖

**结论：现有公共值转交表示可以作为201的候选基础，但尚未由callback Store的实际Analyze/Verify或机器运行证明。** 三份原始源码探针已经执行；两份callback Store停于Expand资格门，含Store字段Package被闭包捕获停于Types的保留存储门。本预检没有修改生产、规范或默认额度，不是201准入或安全验收。

依据主树 `f27357e097174f19567e25d0f078214a9c11b97c` 的 [201提案](201-proposal.md)。源码审计和实际CLI均绑定200组合 `5c2fbc50135d6ca12ee0481153b954194ad39b9d`；设计摘要 `13ea0962a2bf73893a0b03eac133456216856f3748152392c925e95028059c7d`、规范集合 `4402e04fd45551cbdd8dff92c4ac51af8d8ddd6f87296c17ce3690f55359bff3`、schema25。200的正式验收由其独立流程完成，本预检不扩大200范围。

## 三份实际输入与到达门

输入先冻结为 `94105de0136abab5777b1cc2cd2e7a3447f18f7e`。复用前端已构建的clean 5c CLI，SHA-256 `4eaf5291d867f0ebc64390f5ab3850ea6726570216ab688ae971c44e457a2fc9`，构建收据和执行前后文件摘要均核对。没有重建target。

| 输入 | 实际结果 | 到达门及未到达范围 |
|---|---|---|
| `00_original02_store_of_callback_state.zil` | Expand SafeReject，`E-GENERIC-ARGUMENT`，span58..59，exit1 | StoreElement的非manages F字段不满足有限Copy资格；Analyze/Verify/native均未到达 |
| `01_split_pair_two_elements.zil` | Expand SafeReject，同代码，span136..137，exit1 | 仍是Item的F字段资格；两个元素、pop及callback调用尚未到达 |
| `02_package_store_capture.zil` | Types Unsupported，`E-UNSUPPORTED`，span394..397，exit3 | `Package capture with Store fields requires a retained storage protocol`；Analyze/Verify/native未到达 |

三次 `verification_runs` 均为0。原02保留626字节，SHA-256 `23faee6a4575e6c522fa8ffed71d929dfd8463241197c7760cbe670f11f65fa8`，与历史 `bbdb967d38e574a11d5a96518ed2213039dc6bd3:evidence/preflight200/cases/02_store_of_callback_state.zil` 及200夹具逐字相同。

第二份输入使用200已知的单个双字段 `Pair<i32,F>` 参数，函数内部将两个实际callback字段分别转入单字段 `Item<i32,F>`，reserve2、push两次、pop两次后各调用。两个初始环境仍来自同一工厂的独立调用。它没有引入两个State参数的helper，也没有为通过前门而删除拥有ByteBuffer。当前只确认原探针真实停在哪个门；没有声称后续构造、错误返回或pop路径已经实现。

第三份输入的Store元素不含callback，是现有普通Item。它将完整Store作为Bag的Managed字段，再让返回闭包拥有Bag，专门识别捕获存储的独立门。该Unsupported不证明依赖环、窗口寿命或析构正确。

## Analyze的已有责任基础

[`zil-ir/src/lib.rs`](../crates/zil-ir/src/lib.rs) 已表示完整 `OwnedStore { element }`、完整Package字段及ClosureId。OwnedStoreNew携带实际元素类型；Reserve、Push、Pop、Borrow/Mut均携带真实store SSA。Push的结果是 `Element!Unit`：操作消费完整输入，满容量时由error载荷继续拥有该元素；Pop返回 `Unit!Element`，非空时完整责任进入success载荷。不能把失败理解为原local仍可再次使用。

[`zil-analyze/src/resources.rs`](../crates/zil-analyze/src/resources.rs) 的实际transfer已具备以下形状：

- OwnedStorePush调用 `consume` 消费实际SSA，再产生拥有错误联合责任；OwnedStorePop产生拥有结果责任。Make/TakeErrorUnion、DropErrorUnion继续承担各分支的原有责任。
- MakePackage逐一核实际字段类型和装配值并消费各自责任；同ClosureId不会使不同SSA合并。完整Package转入Store时可以复用这个外壳责任，但新增callback字段的自含资格必须另外核。
- StoreNew产生Created来源；Store操作通过 `dynamic_store::access` 将Reserve/Push/Pop/独占借入标为写访问，继承实际description的资源来源，而不是按变量名授信。
- Store本身是一个拥有前缀的Managed资源，Analyze没有按runtime容量分配静态元素槽。这一点应保持；每个实际元素的回调清理由已核Store运行时/后端规则承担，不能从“完整Package被消费”推出后端已经逐callback析构。

这些是5c源码事实，本次没有穿透前门运行callback Store来验证它们的组合。201必须补成功/失败push、空/非空pop、错误载荷丢弃、多个不同环境、零/部分调用和逐分配故障，而不是只核一个Store类型声明。

## 新元素必须在Analyze独立准入

当前 [`budget.rs::local_value`](../crates/zil-analyze/src/budget.rs) 对Managed/Desc OwnedStore先按carrier接受；结构preflight会遍历OwnedStore.element以计数、约束深度，但并不等于检查这个元素是否符合新的callback State规则。`type_work`把ClosureId作为原子处理，也不替代实际捕获图检查。

当前 [`callable_fields.rs::check`](../crates/zil-analyze/src/callable_fields.rs) 只对函数locals、values、return的**顶层Package**直接callback字段调用self_contained。只出现 `Managed(OwnedStore(Package(...Closure...)))` 的模块，不能假定会通过别处某个顶层Package触发同样核验。仅持有Store、未push/pop的函数同样必须有资格检查；资源定义不能成为漏查入口。

因此201需要一个按实际Type遍历的、先计费的Store元素准入：覆盖函数真实类型、resource definition、OwnedStoreNew元数据及错误载荷中出现的element；核ClosureId存在、1/2个直接同类型字段、完整名义Package及现行自含捕获边界。不读取Types的分类结果，也不通过制造空helper合同绕过。具体写集建议为 `dynamic_store.rs`、`budget.rs` 和 `callable_fields.rs`，是否需要资源transfer变更由实际raw攻击决定，不预先重写安全核。

这一检查不能给旧无callback Store或旧单输入State批量追加无界重复遍历。新增Closure图工作与实例重复、类型/名义名称比较、worklist增长都应先计费；旧额度不提高，原预算控制需保留精确身份。

## Store窗口不是拥有callback槽

[`holds.rs::infer`](../crates/zil-analyze/src/holds.rs) 已从DescribeLocal/DescribePackageField产生真实父资源root，OwnedStoreBorrow/Mut继承该description来源，AddressField和ReborrowAddress继续传递来源。借入callback返回lending地址时，CallClosureBorrowed的结果又继承callee来源。因此，公共Hold可以继续表达“这个callback/结果依赖这一个实际Store”这一关系。

独立Verify分别重建来源和loan：[`borrows.rs`](../crates/zil-verify/src/borrows.rs) 的Store借入使用真实store SSA；[`loans.rs`](../crates/zil-verify/src/borrows/loans.rs) 对借入、projection、reborrow及CallClosureBorrowed保留来源/父loan；[`buffer_effects.rs`](../crates/zil-verify/src/borrows/buffer_effects.rs) 从实际调用参数替换结构写入和apart义务。不同索引目前不提供一个新的可持久元素身份，不能仅因索引或ClosureId不同就推导两个独占窗口apart。保守的同Store结构失效规则应保留。

但当前前端和独立类型核还存在真实的**位置准入缺口**：

1. [`packages.rs::package_callable_callee`](../crates/zil-types/src/packages.rs) 的199直接调用只识别名称所指的真实本地 `Type::Package`；`store_at`产生的 `Address<Package>`不满足这一路径。
2. [`addresses.rs::address_place`](../crates/zil-types/src/addresses.rs) 明确限制Store元素窗口只投影有限Copy字段；Closure字段不会被当作普通Copy读取。
3. Verify的[`AddressField`](../crates/zil-verify/src/lib.rs)先核整个Store元素，然后调用普通 `check_type(field)`，不能只放宽StoreElement就默认Closure字段地址获得准入。

如果201必须支持就地 `view.callback()`，现有 `AddressField → ReborrowAddress → CallClosureBorrowed` 可以作为表示候选，但必须补前端真实位置生成、独立字段许可和来源/loan的纵向证据。Fn只能共享调用，FnMut要有实际独占元素窗口；FnOnce不得从借入窗口盗取拥有责任，只有合法pop后的完整本地Package可走原TakePackageField。callee必须先于args建立，活跃lending结果的最后使用必须阻止同Store的reserve/push/pop/give/drop或冲突重入。

本次三输入没有试图绕过前门进入这些窗口动作，所以上述是源码审计和必须验证的边界，**不是新窗口功能已通过的声明**。完整值的push/pop与借入就地调用可共用现有公共类型，后者的语义准入不能省略。

## 捕获图：保留现有禁门，不混淆两类循环

[`zil-types/src/closures.rs`](../crates/zil-types/src/closures.rs) 与 [`zil-verify/src/closures.rs`](../crates/zil-verify/src/closures.rs) 都明确拒绝直接Managed/Desc Store捕获，以及含Store字段的Package捕获。后者的Types入口已由第三探针实际到达。Analyze用于callback State字段的self_contained也排除直接Store，以及含Managed Store的Package；但其一般retained-return分类允许Package类别，不能把这个字段专用检查描述为所有闭包的通用Store禁门。

对201最小同质callback Store，建议保持上述捕获范围。允许“Store拥有一个自含callback”不意味着同时允许“这个callback又拥有同类Store”。现有允许的直接Managed ByteBuffer、有限普通资源、无Store的有限Package和直接子Closure仍按各自规则独立核验。

Verify `closures::check_layouts` 的DAG边来自**直接Closure捕获字段**；它没有把Package内部或OwnedStore.element里的ClosureId加入同一个环境图。`type_layout_work`会对OwnedStore元素的有限Type树计费，但ClosureId仍是原子。类型实例扫描同样不能假定自动跨Managed/Desc Store展开每个callback环境。

须区分：`C0 → C0` 的直接内联环境循环使机器布局不有限；`C0 → Managed Store<Element<Closure C0>>`经过固定大小的间接Store头，不应误称为同一种无限内联布局，但它引入当前未准入的拥有/清理及类型依赖循环。保留捕获禁门可以使201不顺带承诺这套递归协议。未来若开放，须明确新的图遍历边、循环裁决、展开与析构预算、runtime有限责任及raw/Wire反例，不能依赖当前seen集合停止遍历就宣称循环安全。

## 公共表示判断及建议顺序

当前判断为 **“可能够用，尚待实际纵切”**：完整值路径已有Package/Closure/OwnedStore与错误联合，源类型/ClosureId和每个SSA都进入现有Wire；窗口候选也有字段projection与借入调用指令。尚未发现必须新增公开事实才能表达完整push/pop的证据，但没有callback Store的真实Wire正例，不能先宣布schema必不变化。

建议先以原02和单Pair拆两元素输入贯通完整责任与实际机器清理；同时保留不带callback的旧Store控制。再把201要求的窗口调用绑定到同一候选，核父失效、借入FnOnce拒绝、同Store别名和lending最后使用。任何父类型/元素ClosureId、缺字段、重复SSA、错误载荷或sealed Store定义变异，均由raw/Wire独立重建，不能只记录Expand拒绝。

源码审计显示最小Analyze工作不只是删除一个条件，尤其需要把新元素的资格检查放到实际Store类型出现位置。机器侧仍须解决真实Closure布局和逐字段清理，不能由本文的候选表示判断取代。

## 证据与四目标

原输入、小型运行摘要在 `evidence/preflight201/analyze/`；完整实际命令、stdout/stderr、已产生阶段文件及CLI构建收据另存 `201-analyze-preflight-evidence.zip`，不含执行产物。只执行三次CLI check，没有Rust测试、LLVM/native、Wine、原生Windows或性能采样。

| 目标 | 本预检的实际结论 |
|---|---|
| Rust同类安全 | 只识别当前资格门和必须补的独立核边界；三次前门拒绝不构成callback Store安全证明 |
| 运行与内存同级 | 未执行机器程序或同语义Rust时间/RSS比较 |
| 表达与维护更简单 | 已保留单Pair拆多元素的原始使用意图；尚未贯通，维护成本未测 |
| 编译成本更低 | 没有性能比较或新预算测量；与用户独立编译专项不混算 |
| 至少一项突破 | 形成可实施的具体边界和原探针，不声称产品净突破 |

201尚未实施，完整语言 NotSatisfied、产品 NotRun；原额度、产品门和200验收范围不变。
