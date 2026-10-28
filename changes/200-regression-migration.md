# 200 全仓旧诊断迁移

第一轮完整工作区在干净提交 `bcedb4db78fb1a6228b3baf54cb36a6e4dedf91b` 实际执行：2197 通过、1 失败、0 忽略，共 231 个结果组。失败收据及输出保留，不改写成通过。

唯一失败为 `impl182_capture_contracts::original_probes_keep_inputs_and_remove_the_unrelated_head_switch` 最后的旧 `helper_address_parameter.zil` 反例。它仍以 `Types / Unsupported / E-UNSUPPORTED` 拒绝，Verify 执行次数仍为 0；200 扩展了共享诊断句中的已支持形状说明，旧测试仍匹配 196 的完整旧文字。

本次只把这一字符串断言更新为当前生产发出的完整句子。原 source 的 include_str、phase、category、其余反例、攻击及所有生产代码均未修改。不将这一前门拒绝称为借用安全证明。

该文字迁移须经非作者只读核对；精确定向复验与下一轮完整工作区的命令、身份、结果另行记录，不能用 2197 加定向通过拼成一次全仓执行。
