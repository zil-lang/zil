# 普通拥有 Sum：阶段结果与下一主线

本批239–244已在共同源码 R `15498f14af8f11d3a3bf4b198fe69208be5913ce` 完成本地正式验收和限定独立审核。普通有限拥有 Sum 已接通源码、独立责任核、机器表示与真实失败清理。PR17两条CI仍在运行，尚未合入main；本地成绩与CI、main成绩分开记录。

环境已核对为 Rust/Cargo 1.99.1、LLVM/Clang 23.1.4，目前无需重装或升级。当前发布义务为两条PR CI完整核回、实际main合入及main自动重放；完成后按真实回执更新此处。

## 本批闭合了什么

非泛型普通Enum可以拥有多个已核资源叶或完整Product，Product可以拥有Sum，错误载荷可以携带含Store的Product。构造、按值参数和返回、消耗式match、嵌套模式、完整替换、同步defer及各类退出共同保留真实活动分支和实际拥有责任。普通helper的数据参数也可承接这些值。生成的非泛型Unit/Tuple枚举与手写枚举使用同一资格核，保留实际`manages`标记。

schema、Wire与Typed指纹为30；四条局部枚举指令提供tag观察、Copy末端读取、完整解包和条件清理。路径、实际字段、名义类型、资源定义与局部责任均需核验，不能从同名、同布局或当前Empty分支推导Copy权限。Unpack使用真实新局部初始化顺序，不继承父清理年龄；清理只访问当前活动载荷，并先清实际drop flag。机器层沿既有Enum ABI和LLVM API构建，不新增隐式Box、RC、锁或旁置权限表。

本批没有把普通拥有资格推广为capture、StoreElement、数组、FFI或任意ErrorUnion槽资格，也没有开放拥有泛型Enum、callback拥有签名或长期外部回调生命周期。直接Enum捕获等最近反例和未用定义仍需拒绝。同步callback拥有参数/结果是下一主线，不能以普通helper另带一个拥有数据参数代替。

## 最终R的实际结果

| 验证范围 | R实际结果 |
| --- | --- |
| 默认完整工作区 | 3575通过、0失败、0忽略、0过滤 |
| 参考完整工作区 | 3474通过、0失败、0忽略、0过滤 |
| LLVM text-reference | 64通过、0失败、0忽略、0过滤 |
| 质量与交付检查 | workspace严格Clippy、fmt、release CLI及迁移/证据传输/计划自测通过 |
| 239–243限定非作者审核 | 138通过、0失败；按各审核者实际区域保留身份 |
| 240前端补充非作者审核 | 61个输入通过，限Parse/Expand/Types；不计native |
| 独审中的244消费者 | 476次native；380次分配探针中252次真实失败，最终live=0 |
| root独立244消费者复核 | 100次native，其中26次真实分配失败；全部最终live=0 |
| 既有State控制 | 16次构建、128次执行通过 |

这些集合分别记账，作者、独审、整仓、PR和main的重放不相加为新增覆盖。R受测源码前后clean；规范摘要为spec `a7f77e9042b79f82368a13ae9712ed843ff2dead1edc31153d7c81c689b602e8`、norm `74d508ea769bbc8e8a83649011b7553bb2dcd7fcbe78422231921c004e58de5e`。正常路径、错误路径、替换、提前退出和实际分配失败均保留源与产物，原始产物归档逐成员核hash后才释放可重建临时文件。

主要回执位于`evidence/integration/owned-sum-20261102/formal-R/RESULT.json`、`evidence/reviews/239/integrationR/COMMON-RESULT.json`、`evidence/reviews/240/frontend-finalR/REVIEW.json`和`evidence/reviews/244/finalR/REVIEW.json`。本地最终验收与非作者证据已提交独立交付分支，正在执行CI的PR源码保持R身份。

## 原失败、返修与费用边界

首轮P完整整仓为3548通过、12失败，零忽略/过滤。部分旧Unsupported断言需要依已批准的新能力迁移，另有helper全链和raw类型资格的真实遗漏；原输入、错误及修复分别保存，没有删除旧负例来消除红灯。

后继Q `b51dc2043ae51ec42c1bec964bec0623df152431`虽已默认全仓3575/0、限定独审138/0，仍被追加合法正控证明存在误拒：generic helper内分支替换后的当前完整owner被旧Desc失效事实错误阻断。后继修复保留精确owner和实际激活/作用域事实，避免旧描述复活，同时允许合法当前owner及替换后新描述。最终R对原源和近邻重新核验，240补审的61个输入包含原54、Desc四项与owner三项。全仓绿色不能替代对合法组合及最近反例的独立推导。

五模块消费者曾分别耗尽Analyze、Verify和Codegen的默认预算。处理方式是共享结构、复用真实收敛结果，以及按完整类型和当前资源/target上下文建立清理计划；实际工作继续收费，没有上调默认额度。两项必要Copy证明被明确登记为费用例外：泛型Enum实际字段检查的Expand work/nodes由331/124变为336/126；未用generic helper的实际Enum Copy/线性证明由266/116变为296/130，Types仍为219。保留原输入、旧/新N、N−1与fresh控制，不能据此宣称全部旧输入费用不变或整体提速。

本轮还有一项发布教训：root过早为b51建立draft PR并触发全CI，后继R push按workflow并发策略自动取消Q的两条run。可取日志已封存，但CI打包因`evidence changed while archiving`拒绝，实际0 artifacts、没有transport，原CI native无法完整恢复。这些run按取消及证据恢复缺失记录，既不记Pass，也不记作测试失败；本地Q完整76282文件归档仍在。后续先完成已有独审的合法正控，再发布冻结候选，减少无效CI与临时证据丢失；沿既有流程执行，不增加新审批层。

## 下一主线：同步Event/Reply回调与dispatch

最终R的五个只读探针保留原源，未新增native：拥有Event/Reply的公共`dispatch<F:FnOnce(Event)->Reply>`、具体闭包拥有Event入参、闭包拥有Reply结果，均仍在Types Unsupported且Verify执行为0；裸`F`原样返回仍在Expand Unsupported；已有`State<F>`完整转交后进入Store的控制到Verify成功。见`evidence/planning/owned-callback-finalR/RESULT.json`。规划稿中的施工期CLI观察不能代替这份最终R回执。

下一项必达组合是：事件完整准备→具体闭包消费拥有Event→公共单F、按值FnOnce dispatch→完整拥有Reply返回→渲染与失败清理。输入消费与环境kind分开：消费参数本身不能把无消费捕获的Fn或可重复FnMut降为FnOnce；拥有返回必须先完整准备，再清局部及Once剩余环境。调用来源、活动分支、Store根、Desc窗口和先前失效均继续由两核独立重建。

这仍是待冻结、待实施的后继合同。裸F原样转交、更多公开借入回调组合、Type/Syntax声明式入口，以及外部callback注册失败、停止准入、注销排空和最终回收分别接续；不重复开发已有State/Store能力，不把它们提前记作本批完成。

## 交付维护与整体目标

R源码快照记录了91311个`evidence/`条目、1,489,515,861字节历史证据，当前非evidence源码输入4092项、26,587,437字节。依据是`evidence/integration/owned-sum-20261102/formal-R/source-inputs.json.gz`所对应的实际快照（本地原件在`/workspace/.zil-env/formal-owned-sum-R/source/source-inputs.json`）。这些量说明交付、核验和恢复已有维护负担；本轮没有测量它们与编译耗时的因果关系。后续宜优先采用已校验归档与紧凑索引，保持可恢复和精确身份，避免继续散开重复产物，也不凭体积数字宣称编译变慢。

实际状态入口仍需纠正：`evidence/final-summary.json`停在历史schema27及旧source commit，而本批生产协议已经是30。发布时应更新或明确标记该入口的历史范围，并把当前精确源码、规范及验收回执接上；报告本身不替代状态入口和主树工单更新。

实施继续采用`gpt-6.1-sol/high`，复杂责任核和非作者审核使用xhigh，按阶段不相交写集并行；root统一合同、共同候选、正式验收与发布。模型降档和并行只是施工安排，不能代替原源正控与独立责任证明。

| Rust四目标 | 本批可证与仍未完成 |
| --- | --- |
| 安全 | 有限普通拥有Sum经独立Analyze/Verify、raw/Wire、作用域及真实清理检查；完整语言安全尚未建立。 |
| 运行与内存 | LLVM API的O0/O2、活动载荷和真实失败live0有实证；没有同语义Rust运行时间/RSS配对结论。 |
| 表达与维护 | 自然可选状态、Product/Sum与错误载荷形成真实Session组合；完整框架维护优势未测。 |
| 编译成本 | 默认额度保持，必要新增检查真实收费，消费者在默认预算内完成；没有本轮Rust编译速度优势结论。 |
| 实质突破 | 有限可用组合范围扩大；产品级四目标合取及可重复净突破尚未建立。 |

完整ZIL 1.4、全部ZUI、挂起/并发和原生Windows验收仍未完成；B0 Doing、完整语言/ZUI NotSatisfied、产品NotRun保持。已有product-gates及门槛不变。
