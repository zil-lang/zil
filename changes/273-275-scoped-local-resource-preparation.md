# 273–275 scoped helper 的具体本地资源准备

固定基线 `fa34160bacb8ca5bd4c998467be6e6177e3a5aec`。前片266–272保持冻结验收，本片独立施工；正式合入等待前片。设计§4、§8.1、§9.1/9.1.2及§12.4.1已有具体资源创建、give、完整Package与本地body语义。本片补实现准入，不增加IR/schema/Wire、泛型权限或默认预算。

实测同一Mark/ByteBuffer→Event→具体FnOnce body：非泛型本地准备和泛型接收完整拥有参数均Verify成功；F-only scoped helper本地准备在Expand的`give mark`被旧profile门拒。独立手填raw/Wire证明现Verify已核MakeManaged/MakePackage到body的责任及正常丢弃，不能把该等级当source或native。

允许scoped helper内实际创建、归属明确、类型具体且不含符号F的本地resource/完整Package，沿原create/give/消费/失败合流追踪进入With的具体body；不由出现With或源注解直接授予拥有事实。复用既有具体资源资格与两独立安全核，未知generic数据/既有不支持的Enum、array、callable-field等边界保持。原不含本能力路径的费用保持，新增实际工作预付共享请求余额。

拥有事实来自实际具体producer，不以With存在性作为权限开关；同形普通同步callable helper同样适用。普通Name/Desc及没有create/give结果声明的调用不能仅凭静态资源类型建立owner。完整Package仍走原Value移动与实际字段准备规则；不改变其它helper返回、捕获、嵌套调用或形状准入门。新增一个不含With的普通同步helper对照，旧scalar和具体拥有参数控制的精确费用须保持；此前Unsupported的实际本地资源准备新增工作单独预付和测界。

必须通过原字节generic-local及两个既有控制的源码、默认/参考O0/O2和真实失败准备：登记成功、null未调用body、reserve失败退出均只清实际取得责任，返回后live0，调用者F仍可用。拒绝重复give、give后使用、Desc伪造owner、同owner重复capture、漏清理、消费后复用及F实际别名；独立raw保留准确plan/Flow正前缀，错误结构不能替代责任核验。

273仅Expand生产及既有测试，必要Types缺口先报root；274独立Analyze/Verify组合测试和审查，先不放宽生产；275复用foreign_scoped_callbacks功能入口及按功能命名的小源/C/oracle输入，不新建工单号Cargo target，不改旧ORACLE预期。机器与源码结果分别记录，原失败保留小文本，无永久raw归档。

正式全量9c6a64ee出现一项旧177预期失败（其余3918项通过）：`let d=R{code:65};let a=give d;return state`此前列为Unsupported，但该d来自实际资源构造，按本片合同应建立owner。保留原源字节并单列Verify成功；原描述值拒绝义务改由`let owner=R{code:65};let d=owner;let a=give d;return state`准确验证，Copy字段及callback字段两个原反例不变。独立重放确认真正Desc仍Expand/E-UNSUPPORTED且未进入Verify。本次仅迁移该测试预期，不改生产代码。
