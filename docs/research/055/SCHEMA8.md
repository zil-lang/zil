# 055 的 schema8 衔接：独立功能补充

集成补充：以下为原候选的适配合同。当前已完成实现、15项功能测试及非作者精确复核，结果与完整输出见[SCHEMA8-INTEGRATION.md](SCHEMA8-INTEGRATION.md)；本文件不改写schema7性能身份。

本文件是集成说明；055 正式性能身份仍为 `3d4375a18ea4ab9c4e800461befea1d7f3c69f42` 的 draft10/schema7 生产依赖。schema8 适配由集成方在主线加 055 的临时树完成，不改写该冻结、原始样本或性能结论。实际功能通过的身份与范围仅由上述集成收据给出。

## 需要处理的实际 IR 变化

schema8 新增 `Type::ErrorUnion { error, result }`，其当前 E1 两臂要求 Copy；新增 `ScopeEnter`、`ScopeExit`、`MakeErrorUnion`、`ErrorTag`、`ReadErrorUnion`。没有新增 Terminator 变体。`?` 错误返回仍由实际分支、投影、清理、`ScopeExit { kind: Error }` 和普通 `Return` 构成。冻结 CONTRACT 中前瞻性的“专门 error-return”不应解释成不存在的独立终结指令。

| 位置 | 最小语义适配 |
| --- | --- |
| `graph::operands` | 三种 error 值指令读取其 `value`；ScopeEnter/ScopeExit 无 SSA 操作数 |
| `oracle::before` 的独立穷尽匹配 | 自己读取上述 `value`，scope 标记对本轮 may-liveness 无 gen/kill；不调用候选访问器 |
| `graph::event` / 独立 mandatory-event 审计 | scope 与 error 指令必须作为观察事件保留；标签保留 scope ID、CleanupExitKind、is_error、value ID |
| `graph::potential_failure` | 对 `ReadErrorUnion` 加入保守投影失败 sink |
| `oracle::audit_extraction` 的独立失败分类 | 独立加入同一 ReadErrorUnion 投影，不重用候选分类器 |
| 类型活跃槽判断 | 当前 Copy-only ErrorUnion 不新增 Desc/借入闭包槽；若未来允许资源臂，必须另定完整语义 |
| 预算编码大小 | 新指令只有固定字段，现有固定字段保守计费可覆盖；不增加未计费的大向量扫描 |

现有 `graph::event` 默认保留未被显式列为透明的指令，但集成仍须用真实输出确认这些新节点全部保留，不能把“编译穷尽匹配通过”当作错误出口正确性的证明。完整事件标签从原 IR 产生并被 raw 审计核对，错误臂、scope 类别和清理顺序都不能投影掉。

研究抽取拒绝的是返回 **Desc 或捕获 Desc 的闭包**的签名；Managed 返回本身受支持。真实 `error_cleanup` 中有资源工厂，不能误把所有资源返回函数都跳过再宣布全函数通过。

## 用真实多模块项目产生补充

`examples/mainline/error_cleanup` 是四个真实文件：`main.zil`、`io.zil`、`token.zil`、`failure.zil`。应通过公共的程序级 Parse/Expand/Types 入口建立实际模块/导入关系，不删除 import 或把源码拼接后冒称原项目。现有055单文件 `typed(name,text)` 不能直接承担这个任务。

可在研究工具增加明确限于这四文件的功能入口：逐文件创建 `SourceUnit` 并解析真实 import；构造对应 `CoreImport` 后调用 `expand_program_with_limits`、`check_program`，取其 `TypedProgram.module`。随后保持同样的生产 Analyze/Verify 前提，并对**每个非外部函数**执行 raw 审计、独立 oracle、flat/cut/block 全行相等和完整事件关系核对。该入口不进入生产 driver，不扩大正式性能语料。

应记录实际源码与 schema8 依赖/适配源码哈希、TypedIR 指纹、函数总数、ScopeExit(Error) 数、错误操作/投影及终结数，并保留完整输出。特别覆盖资源/捕获清理、调用参数或 callee 待求值时的 `?`、部分构造、闭包内部错误与循环中的错误出口。对错误 tag、is_error、scope exit kind、错误臂边和清理次序做篡改，检查器必须拒绝；某些篡改可能不改变 may-liveness，正好验证两层保证的区别。

补充只给**功能一致性收据**，不重用 schema7 的耗时、预算边界身份或样本量。适配完成后的规范/研究兼容文档可以更新，但旧 freeze 继续按原 manifest 验证；需要重做历史计时应检出历史提交，而不是在 schema8 上绕过源码哈希检查。
