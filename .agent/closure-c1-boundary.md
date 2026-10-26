# 当前闭包与地址边界

状态：draft12，schema/wire9。当前事实以docs/IMPLEMENTATION-STATUS.md、051–062工单与最终收据为准；本记录不覆盖语言规范。

已有Copy聚合环境、默认共享/独占捕获、move Copy快照及地址转交；本地资源默认捕获Desc，move Managed或正文give在创建时转交载体。拥有环境按正文作用分别为Fn/FnMut/FnOnce。Desc读取当前目标，目标写不等同环境写；持有资源也不自动等于FnOnce。未调用环境清理，消费时已移出字段不重复清理，剩余字段正常返回后清理；环境局部按逆成功构造序，捕获字段按固定首次出现序。

独立Analyze与Verify分别重建来源、责任、Desc存活、转交及间接写效果。LLVM使用inline环境、字段完成标记和局部live flag，拥有/FnMut/FnOnce入口传实际环境地址；FnOnce临时分配固定在入口块，无隐式Box/RC。

一层共享/独占地址、重借、参数与返回、投影仍保守持有整对象根。只读move &mut仍保守FnMut；资源Desc不冒充独占Address。环境逃逸/参数/返回、嵌套环境、一般拥有聚合、一般trait/dyn/opaque与unwind/取消尚未完成。有限Copy E!T/postfix-?和显式词法storage scope已贯通；结果SSA仍由全函数责任检查约束，拥有错误载荷与一般拥有聚合部分初始化未开放。

CT纯函数CFG与有限tuple/record/array结果重新进入未核Core；CT enum/错误联合/函数指针/地址和外部IO仍关闭；纯CT scope标记照常计真实指令访问。公共类型/有限泛型及标量C ABI已贯通，不重派未开工。质量工具现有session/scalar-native/resource-closure-native，后者只是有限顺序生命周期。B0 Doing，完整语言NotSatisfied，产品门NotRun；问号/局部move/@及命名候选不变。

有限完整package作为普通非Copy Value已接通，字段Desc可进入已有局部闭包。Package整体及拥有环境作为包字段/闭包捕获仍未开放。准备临时在失败时逆序清理；完整包字段按声明序清理，不混同一般部分完成对象。
