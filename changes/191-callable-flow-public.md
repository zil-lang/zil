# 191 显式循环调用图公共基础

## 问题与边界

190只读预检确认运行时while/for helper被有限路径合同挡住。schema23的Vec<CallableTrace>无法描述未知次数重复；空列表、重复次数哨兵、Trap伪装都错误。本片只建立公开数据、完整身份与有界传输。源语义仍由190按实际CFG和当前实参独立重建；本片单独不开放Flow执行。

## 表示

schema/raw-wire24；typed domain V24、callable参数合同domain2、已验证导出domain4。返回合同domain2和环境lending来源不变。CallableTraces明确区分Finite(Vec<CallableTrace>)与Flow(CallableFlow)。Flow包含entry:u32和Vec<CallableFlowNode>，每节点有有序calls与Edges(Vec<u32>)/Exit(Return|Trap)。索引引用平坦向量，不含递归AST、FunctionId或可信证明位。JSON也明确kind:Finite/Flow。

Finite保持旧有精确排序去重路径语义；只有实际含循环的合成CFG使用Flow。Flow规范是entry0、从有序出边BFS连续编号、无孤立节点、无重复边、非空Edges，索引有效。静态嵌套helper复制已检查callee图并重定位，只有Return接续；Trap和无限路径不虚构返回。190分别实现三个阶段的CFG重建、循环状态不变式与权限核验；本片运输的结构不等于已验证语义。

## 不可信输入额度

Wire每个节点/调用/边向量在分配前经过count的总节点额度与剩余字节检查；Flow节点受max_nodes、调用受max_instructions、边受max_analysis_steps，并额外累计整个合同内调用/边防分散绕过。raw Analyze/Verify先逐集合检查和收费，后指纹/比较；功能阶段遇Flow明确Unsupported，不隐式转为空Finite。全部字段参与模块和参数合同指纹，不共用安全求解器。

## 验证计划

保留有限原输入机械迁移、旧版本拒绝；Flow往返、逐字段身份、未知tag、截断、过大/累计集合、工作额度边界和正常请求重试；raw Analyze/Verify不接受未实现Flow。验证不依赖增加默认额度，不将运输通过宣称运行时循环已完成。

## 非作者规范文字复核

/root/restore177_inventory 对0a42188只读复核发现旧interface段落仍写固定0/1/2次和导出domain2/3。已按实际Finite/Flow和domain4校正，未改生产；语义准入仍取各片真实源码与独审，不以新增结构宣称loop完成。
