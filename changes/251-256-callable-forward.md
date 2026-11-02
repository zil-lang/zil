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

## 私有回调槽按结构封闭，不以出现转交调用授予资格

254独审发现旧施工门把被覆盖局部和合流路径串接，错误判断某槽曾流入Parameter调用。复核§9.4.2及接口的persistent_slot约定后，确认“普通私有单槽必须流入keep”是额外施工限制，不是同实例返回的语言规则。原R1/R2实际已销毁A并合法转交B/C，尚未证明资源或借用漏洞；原输入、旧门预期、错误接收与两次Fail报告全部保留，不追改历史。撤销该存在性门及其相关性图，亦不改用会误拒合法分支的must条件。

本批只按结构准入已有编译器局部载体：Array<自含具体Closure;1>位于函数局部/SSA，元素签名须满足已核闭合拥有数据规则。载体仅通过构造、局部存取、元素0取出/补入/借用/销毁和既有清理操作流动；载体自身不得进入普通/外部调用实参、返回、环境捕获、Package/State/Store或其他聚合。公开参数与结果继续原容器门；不开放一般F数组或隐藏的拥有签名工厂。元素本身按既有具体F规则移动、调用及转交。实际活性、借用、责任与失败清理由独立资源核逐CFG核验；Parameter函数每个返回的真实同实例与声明模式仍由独立provenance核验，不能删减。

新验收把原R1/R2的完整程序作为结构封闭、责任正确的当前正控，另测合法混合分支/循环、无转交函数的私有局部，以及载体逃逸、重复take、旧借用和错误清理反例，均需raw/Wire与源码对照。旧机器oracle和源面目标不变，新增/删除遍历按实际费用记录；不通过修改Unsupported类别、提高默认额度或丢弃历史Fail达成通过。

## 闭包正文中的普通拥有回调局部

结构门独审的真实源码在外层闭包正文创建一个自含Managed环境、Event→Reply签名的内层具体回调，Types在创建处以“closure value requires a finite Copy type”拒绝。§9.3.1的具体拥有签名及§9.4.2的合法局部移动没有“只能在普通函数正文”限制；这是前端组合缺口，不是Verify拒绝，也不改写为非目标。252沿真实局部类型/实例、捕获与剩余责任修复该位置，包含同实例局部移出再补入，不把拥有闭包伪装为Copy，不开放外部借入或新签名工厂。原源与原Unsupported保留，实际生成仍须独立两核、原生正常/未调用清理及最近重复移动/借用反例；新增遍历按实际预算收费，旧无此路径控制单列。
