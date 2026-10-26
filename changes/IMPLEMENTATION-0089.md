# 0089：成功语法的预算资格与发现阶段复用实验

状态：研究提案；生产准入待功能、独审与完整请求成本裁决。

基线 `2883f978de79ea0e8a09a4cb218fa68687f4f568`，schema/raw wire15。
规范与公共IR不变，默认额度、错误类别、错误顺序和独立Verify不变。

## 输入审计与两个独立机制

当前 `query_key` 覆盖 FileId、ModuleId、完整路径、源字节摘要、规则/schema/包版本、全部限额、build_inputs，以及按阶段决定的target。driver仅使用默认冻结AttributeContext；公开syntax自定义context不进入driver缓存。发现器每次读取当前字节、调用parse_with_limits，再用AST解析import。068已省去同请求的第二次解析，但热请求仍真实解析。Parse键包括逐模块剩余token/node额度，前序文件增token会改变后续未改文件的键。

L仅让发现器只读复用已发布且数据库仍有效的成功Parse。查找键采用正式project Parse预算（max_source_bytes仍为初始项目总额），并非发现器已逐文件扣减的byte额度。每文件当前字节仍读取；每个import仍重新解析路径、歧义与根目录边界。当前raw超过容量时禁用发现器复用，随后按原路径清空。不在ArtifactPlan/冲突检查前发布新raw。

K仅分离Parse键的max_tokens和max_nodes；深度及所有其他原字段仍入键。syntax提供与成功结果同时产生的私有需求收据：lexer非EOF token数、parser实际nested节点计数、属性校验实际work。收据不进入ParseResult/Debug摘要，不能从磁盘反序列化。driver把收据与成功Parse键绑定；缺失或不足时回到原预算专用键与受限parser，保持原失败阶段、span、顺序。每请求仍按原tokens.len（含EOF）扣项目token/node额度。其他阶段保留全部请求限额。

成功资格只证明重放该次解析仍会成功，不授予类型、来源、借用、释放或Verify权限。现有独立Verify每成功完整请求重新执行。

## 最近反例与关闭条件

- lexer实际token需求不含EOF，项目扣费含EOF；不得混用。
- parser节点数与属性work不能由token数未经证明代替。
- N/N−1、热成功后降低额度、失败后提高额度、取消后恢复保持原诊断。
- 不同FileId/ModuleId/path、注释换行、插入模块导致ID重排必须失效。
- import删除/歧义/根外目标必须重新查验；只读缓存和输出碰撞不因热syntax成功而越过。
- raw超过capacity时，本请求不能从即将清空的旧缓存提前获益。
- K不减少发现器解析；L不解决剩余预算造成的键变化；二者必须分开测。

不扩大接受集，不改变预算；若功能不能完全等价，或完整成本未建立可推广净收益，生产候选NoGo，恢复基线源码，保留各冻结候选与全部失败。

## 预登记

见 `docs/research/089/PREREGISTRATION.md`。正式计时须由root通知独占窗口。
