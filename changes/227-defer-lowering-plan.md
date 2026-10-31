# 227 普通defer lowering：设计前置任务

父任务B0；当前Planned，不计实现。语法已有defer Block，但规范未完整钉死声明时名称绑定、退出时值读取、与隐式资源清理的相对次序、条件注册、return表达式准备、?错误、break/continue及嵌套defer的闭合规则。

先冻结上述规则与最近反例，再决定Core cleanup action或等价显式CFG方案。不能简单把AST复制到函数尾：那会漏提前/错误退出、改变shadowing或重复清理。须与安全核、CT及budget遍历一起施工。该任务与225/226分离，不把语法已解析记为可运行；没有独立审核前不合并生产实现。
