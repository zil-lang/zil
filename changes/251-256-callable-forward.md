# 同一回调实参原值返回

继续用户指定的下一语言主线。以已完整本地验收的拥有Event/Reply和路径修复候选aebc4a600为代码基线；PR18远端归档重跑独立进行，新阶段不得抢先合入main。此枝先冻结公共合同/表示，不把可编译公共接口或局部测试计为新语言实现。待PR18完整证据核回后顺序集成，失败只阻塞真正受影响工作。

目标是普通自由函数的唯一按值F→裸F，返回本次已准备实参的同一有效实例。覆盖Fn/FnMut/FnOnce、旧有限Copy与同步Address调用签名、本轮完整拥有Event/Reply，以及完整普通拥有伴参；通过真实跨模块原源、独立CFG来源、声明模式、实际责任和O0/O2逐分配点清理验收。具体Copy实参复制而非一律移动；符号F bound不授Copy，结果恢复调用者原声明来源，不能统一降格或按实际Closure类型升级。

公共IR新增CallableReturnSource::{Fresh,Parameter(u32)}与CallableCleanup::Returned。索引为实际函数参数位置；source是未核关系，不能用ClosureId/返回类型相同代替实例证明。原Fresh工厂资格不变。schema/Wire/typed指纹迁移31，返回合同指纹3，角色合同指纹4；旧Wire字节和失败证据保持并明确拒绝，具名新版本反例另存。新增字段与遍历、队列/状态增长均预付现有额度；Fresh返回合同多一来源tag，Parameter多tag与索引，格式长度变化如实记录，不提高默认限额或免除必要工作。

251 root持规范/公共IR和机械Fresh迁移；252前端、253 Analyze、254独立Verify、255 Lower/LLVM、256真实消费者按不相交写集并行。所有实现以同一冻结提交为输入，只root发布公共接口和集成。Verify不复用Analyze/Types求证；非作者审核及组合验收后方可合入。本批不包含一般F容器/多F、借入逃逸、opaque新签名工厂、CT、外部长期保留或完整ZUI承诺。

## 已有错误联合的分支上下文修复

冻结消费者698d280c的 `tests/impl256/frozen-prep/project/main.zil::union_data` 在明确 `Meta!Packet` 期望下，以 `if` 分别产生合法 Meta 和 Packet。现有Expand提前剥掉Union后，把then的Meta错误用作else枚举Packet的期望，产生E-GENERIC-ARGUMENT。252修复保留原Union上下文：仅在两分支名义不同且原Union存在完整匹配侧时，为枚举构造恢复该侧的完整类型提示；真实槽、名义身份/类型实参、活动tag及最终Union资格仍由各阶段核验。同名义及非Union旧路径保持，未匹配者不获得额外资格，不改变冻结源/oracle，不开放E!F或新capture。实际类型复制沿已有预算预付；记录原源拒绝、修复后贯通及旧Copy控制精确N，如确需改变旧费用另行登记。

## schema31来源字段的实际收费

新返回关系在Analyze输入元数据中按Fresh来源tag一个单位、Parameter来源tag加参数索引两个单位计入。既有preflight与fingerprint各实际遍历一次，故每份Fresh返回合同新增2个Analyze单位、每份Parameter新增4个单位；这是新增公开字段的真实费用，旧Fresh证明逻辑不变，默认限额不涨。各具名旧工厂控制的总N按其实际合同数测量并记录N−1/fresh，不把该格式变化报告为全体旧N不变。此处为逻辑元数据单位，Wire另按实际tag一字节与参数索引四字节计长度，不能把两种单位混用；其他内核按自身实际扫描预付并分别报告。

## 草案负例范围纠正

原冻结 `mut_declared_actual_fn_shared_call.zil` 的 outer 按值接收FnMut并返回i32，超出本片裸F返回且落在既有Unsupported helper profile；不能为了草案预期SafeReject改诊断类别或悄悄扩大准入。原源、原预期与实际Unsupported历史Fail均保留，不计本片语义拒绝通过。以同一声明FnMut、outer直接返回F的合法范围正反对照承担模式保持义务：`let current=keep(f)`后共享调用应拒绝，`var current`后合法调用再返必须真实通过且保持后态。该调整纠正草案的前置假设，不改变原native oracle；按值FnMut普通数据结果helper仍记B0未完成能力，不因本片完成而删除。

新合法范围的let/var对照已实测：var在O0/O2输出123M并正确销毁；let命中了真实缺可写位置，但Expand把该明确非法调用误分类为Unsupported。252仅对已准入的Parameter返回profile将此诊断分类修为SafeReject并保留原失败；不放宽旧FnMut数据结果header，不增加扫描或改变native预期。
