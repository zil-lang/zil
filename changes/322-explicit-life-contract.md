# 322：显式 life 与真实返回来源上界

本片从已备份基线 fe4cb19ae 接续，不继承未恢复的 301–312 成果。依据
ZIL-ORIGIN、ZIL-CONTRACT、ZIL-MODULE、ZIL-TRAIT 及 316 的真实探针，贯通
普通 Direct 函数的一层共享 Copy/slice 地址输入与结果，以及既有具体 inherent
Self 接收者。life 只约束来源，不授予 Copy、同一性、apart、独占或延寿。

公共合同由 root 实施：CoreFunction.life 保存 CoreLifeContract 的函数内 binder
名字、有序真实参数 life 索引和可选结果 life；最终参数序包含 receiver 0。
TypedFunction.reference_return_bound 保存显式结果允许的升序唯一参数索引集合，
并由 ScalarFunction 对外绑定声明上界。Types 从 Core 声明生成该上界；Analyze
与 Verify 各自从最终 CFG/调用固定点重建真实来源，逐 Return 核其为声明集合的
子集。候选 Hold 不授信。接口继续发布独立核出的实际来源，上界与实际来源分别
进入签名/合同/缓存身份，实际来源变化继续触发既有失效链。

没有 life 的旧函数、返回位置省略 life 的函数继续正文来源推导，保持原多输入
select 接受性。显式返回 life 必须有非空实际共享地址参数集合；无标注输入不被
擅自归入某个 binder。两个不同 binder 的来源即使某次调用 actual alias，也不能
在定义期互换。同 binder 多输入表示允许来源并集，不证明对象相同。

life-only 声明不产生按 life 名称的机器单态化。原 AST 名字须保留到 Core 合同
构造后才擦除；所有其它类型位置不得借 type_ref 静默丢弃标注。初片不开放混合
普通 T/F 泛型、fn pointer/HRTB、where life、别名 life binder、借入存储、可变
life、Product/资源头借入返回、外部 ABI 或异步。Self 正规化保留原 life/receiver
对应关系；CT source 重建、driver CT query 克隆和预费必须保留合同。

schema/Wire33 与公共 None 构造迁移由 root 管理，无新运行时布局、分配或权限位。
只做已有 cross_function_borrows 目标内源/raw/Wire、跨模块、CT、O0/O2 与
N/N−1/fresh 限测；正式全量、Windows、Rust 产品比较未运行不得记为完成。
