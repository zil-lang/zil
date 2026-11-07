# 大数组后端来源与当前边界

`large-array-a18.zil` 原样保留262144字节嵌套数组，预期退出0。历史O0崩溃及O2结果仍保存在 [原始精简复现](https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/recovery-large-array.json)，不覆盖历史。

324候选 `5825cd582ac6b419d185b59486c7b7fc79f1bedb` 已通过恢复后的显式API feature复验：LLVM/Clang23.1.4，clang/API ×O0/O2四组均backend匹配、无fallback、原生退出0。普通大Copy值以独立栈快照及有界内存复制降低；不缩输入、不改请求优化等级。限定结果、此前默认feature跳过API的误报与更正见 [324变更记录](../../../changes/324-large-aggregate-lowering.md)。既有aggregates目标已覆盖，正式组合全量仍由集成方裁决。

目录名保留为历史路径。333已实际复现并修补`large-array-a18-by-value.zil`，连同
原a18、返回及函数指针源形成有限大型Array/Tuple/Record内部调用/返回ABI；
完整signature/fnptr载体依赖使用`.lc1`版本隔离，原版本传播阻断保留。
共同B为`14832d5a35630b7002c79e4df47b0cbb007570b6`，精确作者/非作者机器及
N/N−1/fresh见`changes/333-large-value-abi.md`和`https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/review333-by332.json`。
一般大型Enum/ErrorUnion按值可靠性仍未验收，大型closure接口/环境与特殊helper
亦保留其明确边界；不把有限333扩大为所有大Copy/foreign ABI完成。小栈失败原样保留。
