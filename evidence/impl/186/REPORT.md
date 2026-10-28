# 186 泛型 Store 成功载荷：作者交付

受测源码 `fc96383d70ad6f8e91dff97b416b8237cfb26c73`，当前片作者限定验收 Pass；独立审核待 root 执行。工单锁定 schema22 / ZIL-LANG-1.4 与原 spec8df5/norm642f，不吸收尚返修的182或并行lending生产。

## 实际语言增量

同一 `T:StoreElement` 库可声明 `E!Store<T>` 的 `result(create/give)`，以及成功侧明确拥有的 `give x:E!Store<T>` 参数。联合本身仍是普通非Copy Value，调用、局部转交和返回不再添加give。直接Store与联合成功载荷分开记录，普通描述别名不能拥有，未实例化定义也要证明来源。

新建Store、create调用与真实局部移动、?成功提取保留创建事实；give入参和give结果不能经包装、取出、再包装、普通转发或分支混入伪装为create。Copy错误返回拥有零个成功Store，既不制造fresh，也不会因为没有成功资源被误拒。联合重复移动、give union、默认Desc升级和整联合替换仍明确拒绝。

四个生产文件均在Expand：generic_store识别有限成功形状和Copy错误；generic_check保留类别和来源；generics只在实际Store形状里传递成功期望类型；generic_enum补齐E!T内部的未解析变量遍历。公开IR、Analyze、独立Verify、LLVM、默认预算均不改。实际实例仍完整进入这些原有安全阶段。

## 实证

最终干净同源限定回归85/0/0，含本片12源组和3raw组、邻近普通泛型/178/181/183/166。新源实际原生执行22次，包含两种名义元素、宽Copy错误、晚实参失败、循环局部、普通联合转交和?清理；26个未实例化最近反例均在Expand明确拒绝。11种源码派生变异通过raw和Wire共22条路线被独立Verify拒绝。

四模块generic_result186把相同泛型工厂、适配器与两种实际内存布局接起来。Linux与Wine各54次，共108次；每平台12普通、42探针、32真实初次分配或增长失败。实际字段数据正确，Drop轨迹区分部分准备、工厂错误、调用方错误与直接丢弃，全部live为0且分配/释放平衡；Win64 bad_operations为0。四个普通/探针PE头均实际读出AMD64/PE32+，执行物不随证据交付。Wine结果不等于原生Windows认证。

上述最终运行、CLI构建、Clippy和fmt均绑定同一fc96383提交，CLI SHA见author-result.json；没有用后续源覆盖旧收据。阶段摘录明确标为excerpt，并记录完整原件摘要。

## 保留失败及边界

最初0/10包括符号enum上下文和测试分隔符问题；下一轮7/3发现嵌套Error类型的未解析变量遗漏。随后相邻36项及本片raw2项通过，但最后一个漏清变异误删了已消费临时的条件Drop，被Verify正确接受；修正夹具为真正活跃联合的Drop后通过。原失败日志、源码和候选仍保留，没有为它改核验器。

默认联合注解不凭外观获得Managed，资源错误侧、任意拥有嵌套Sum、捕获/借出逃逸、异步及完整trait求解不由此开放。完整语言NotSatisfied、产品NotRun、B0 Doing；Rust运行/内存、维护、编译成本与产品净突破仍未量化证明。

本目录仅收录源码输入、命令、诊断、观测、阶段摘录和摘要；完整LLVM、对象、可执行文件与缓存留在原始运行目录。相邻旧suite保留完整命令/计数，不重复收录其全部产物。
