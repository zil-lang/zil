# 074 package字段操作与组件更新闭环

2026-10-24（Asia/Shanghai）；生产基线draft14 0c093f5e9fad117b8913d912020e48e9040d581a。依据_zil_design §§2.4、3、7、8、15.2，在生产代码修改前冻结本片合同。

## 为什么

完整package目前只能锁步移动与清理，组件不能安全更换一个拥有资源或移出后恢复。本片把状态细化到直接Managed字段，保留完整SSA/函数ABI边界，形成组件资源更新的源至机器闭环。

## 冻结语义

- 当前Package仍至少一个直接Managed<Record>、其余有限Copy；无整体用户Drop、无嵌套拥有包/环境/Desc/Address字段。不开放含整体用户析构的拆分；该能力仍由已核decompose后续合同承担。
- Package局部区分外壳活动与每个Managed字段的责任状态。Store/参数入口建立完整字段；普通SSA/参数/返回/whole Take始终要求全部Managed字段在所有路径LIVE。部分形状只留在局部，不能跨函数或偷偷变成完整值。
- `TakePackageField { local, index } -> Managed`（tag54）消费一个必须LIVE的直接拥有字段，结果fresh为INPUT，保留包壳及其它字段；像whole移动一样允许let绑定，但不得伪造create返回。
- `DropPackageField { local, index }`（tag55）是显式字段清理动作，条件清理实际仍LIVE的目标字段；在调用用户teardown前将该字段责任移入清理激活并清标志，保持外壳活动。它不改变兄弟字段或外包的初始化次序，不扫描可达图。
- `StorePackageField { local, index, value }`（tag56）要求外壳确定活动、local可写及值类型精确匹配。Managed字段在所有路径均无LIVE责任才可写，消费一个Managed SSA且建立字段LIVE；Copy字段只复制匹配Copy值。该原语不能隐式覆盖遗留旧责任。
- 源`give p.field`使用Take原语；`p.field = RHS`首先定位静态local字段一次，再完成RHS准备。Managed RHS由真实临时责任守护，准备失败不提前清理旧字段；准备成功后显式DropPackageField清理旧字段，再StorePackageField提交新责任。此段无可恢复失败/挂起，现阶段用户Drop若panic仍为已声明abort语义；RHS已发生副作用不回滚。live/moved汇合由条件旧清理处理，不人为限制成只能确定LIVE或确定EMPTY。Copy字段直接Store，普通可写规则保留。
- 允许RHS先移出同字段后补回；RHS若移走/销毁整个外壳，提交必须拒绝。`let`允许移出，不允许补入/替换。无新关键字、隐式clone、Box、RC、锁或世代机制。
- Copy字段在部分活动外壳中仍可读；Desc投影只要求对应Managed字段LIVE。既有精确(local,index)根参与逆向真实liveness：旧Desc及其alias/capture未来仍用时，目标字段Take/Drop/Store拒绝；兄弟字段Desc可以保留。whole移动/清理仍覆盖所有字段根；循环回边不能忽略旧实例使用。
- DropPackage按声明顺序清理实际剩余LIVE字段，再终止外壳责任；全移空仍需结束壳存储。原局部逆成功初始化清理次序、Scope、SSA责任守恒继续检查。字段槽不加入真实局部初始化次序矩阵；新增槽/扫描真实收费，无package函数不额外全模块扫描。
- schema/wire12、Typed fingerprint V12同批发布；现有tags不改，新tags54–56。完整Package机器值仍为原inline struct，局部字段责任flag不进入跨函数ABI。CT不能执行资源/package动作；原始输入遍历和预算仍覆盖全部新操作。

## 最近反例与验收

重复字段移出、缺字段whole转交、活旧Desc/捕获跨替换、不可写填入、LIVE字段直接Store、重复拥有SSA、错资源类型/索引、StorageDead遗债、未初始化外壳Store、分支/循环未恢复、伪装create，以及RHS失败/晚实参失败的精确清理。正例包括兄弟字段描述继续使用、部分包退出、条件take后替换、循环take/refill、全部补全后跨模块返回与消费。

072负责源码/Types/Analyze，073独立Verify，074公共表示、CT边界、LLVM/native应用与总集成。先公共接口后实现；正式全门由root在独立全新target执行，作者限定测试及非作者审核分别绑定精确提交。图071完全隔离，不作为语言合入前置。产品门不变，本阶段不是一般容器/异步/完整ZUI。
