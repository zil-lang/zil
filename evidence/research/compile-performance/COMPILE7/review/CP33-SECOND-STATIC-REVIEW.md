# CP33 独立第二静态复核

审查者：`/root/frontend_incremental`

精确提交：`83854ae83f7fec4fc8f9c22d0751a1fd2725671f`

审查方式：仅静态读取上述冻结提交的生产差异及相关调用链。未构建、未运行测试、未运行诊断或性能采样，未修改生产代码。本文件仅保存已经完成并向根代理报告的静审说明；保存过程没有新增验证执行或分析。

## 结论

对冻结提交的生产差异和相关调用链复核，**未发现阻断性正确性问题**。

| 边界 | 核查结论 |
|---|---|
| `ParseUsage = max(parser_nodes, attribute_work)` | 正确。Parser 和属性验证分别从零使用自己的 node 计数；成功解析所需的共同上限是两者最大值。Parser 在加一前判断 `nodes >= max_nodes`，属性验证在加一后判断 `work > max_nodes`，因此成功边界均可用 `required <= max_nodes` 表达。 |
| Token 收据 | `tokens.len()` 包含 EOF，而 lexer 的额度检查发生在生成普通 token 前。复用条件因此略保守；临界情况下会回原路径，不会错误放行。 |
| 预算身份零化 | 只零化 `max_tokens` 和 `max_nodes`。其余九项 Limits、build inputs、版本、源内容、路径、FileId 和 ModuleId 继续进入身份，另有 CP33 命名空间隔离。 |
| 项目当前额度计费 | `project_pipeline` 与 `load_sources` 均继续逐模块调用 `charge_frontend`，按当前请求扣除包含 EOF 的 token 数。缓存命中没有跳过累计额度。 |
| 当前文件加载 | 仍重新读取源文件、核算累计 source bytes、解析导入与文件路径约束；缓存没有代替这些当前检查。 |
| 额度不足与首错 | 已保留成功 Parse 的收据不满足当前额度时，选择原 quota-specific key，并执行原解析路径，保留原错误顺序和 Span。 |
| 后续 CT 等阶段身份 | 单文件路径与项目路径都从完整 `query_key` 建立后续阶段身份，没有将零化后的 Parse 身份传递给 CT、Expand、Types 等阶段。 |
| 失败与取消 | 原有 query 的失败中止、取消检查及成功后发布顺序保留；失败或取消不会发布成功 Parse。 |

这个结论限于上述生产边界；性能采用与否仍以根代理的完整成本窗口为准。本静审不声称新增执行验证或性能结论。
