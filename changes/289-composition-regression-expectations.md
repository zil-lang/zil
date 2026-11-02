# 289 组合能力与诊断回归

基线c1db412f完整默认为3956通过、15失败，原失败保留。本片只改既有
功能测试；真正的不同字段误拒由288修复，旧Types费用由291单独恢复。

七个捕获冲突源码仍精确SafeReject/E-BORROW-CONFLICT，诊断从Verify
提前到Analyze；只迁移实际受影响调用，其他借入及lending反例保留原阶段。
不接受任意阶段或任意错误。

两个普通工厂原源现在可把自己的实际局部闭包借给child helper，
即使与外层F同ClosureId也不冒充F；原字节执行分别返回99/100，
而外层F返回4。循环raw把实际F换成同型局部值仍违反公开Flow，
Analyze由旧Unsupported变为SafeReject；独立Verify raw/Wire仍拒。

不捕获角色的泛型defer原源按普通本地闭包合同晋升并执行O0/O2；
捕获supplied F的相邻原源继续Types Unsupported，未借defer隐藏角色。

限定七个实际执行目标82/0；原scalar_local_promotion受llvm-api feature
门控，首次零执行不计通过，显式启用后完整目标7/0。最终整仓门仍由集成
候选运行，不将本片限定检查改称全量。
