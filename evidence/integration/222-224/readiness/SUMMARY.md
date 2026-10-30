# 施工8：29 份原始 readiness 探针重放

使用集成方提供的现成 CLI，实际干净构建提交 `6b0d974bedfb40c93eb36e491ceb18a138718dac`，生产输入与共同候选 `4d7bbb762eb34987ffd05c9ab23a6ad6bc6fae41` 相同。CLI SHA256 为 `593da735017a4bf5db773d38ef7c5f3c139c19d75ef6d9a2785d94017b4f1d71`。没有创建 Cargo target 或执行 Cargo，没有额外源码或机器消费者。

| 原输入快照 | Success | Unsupported | 原语法无效，排除能力判断 |
|---|---:|---:|---:|
| 217 原观察 | 11 | 16 | 2 |
| 施工7 / 218–221 最终重放 | 13 | 14 | 2 |
| 本次实际重放 | **15** | **12** | **2** |

本次恰好执行 29 条 `zil check <原始源路径> --json`。相对施工7，25 份合法源的完整结果元组保持、2 份合法源晋升到 Verify、2 份原无效源保持 Parse 拒绝；旧成功项没有回退。这里的 Completed 表示观察任务完成，不是 29 项能力全部成功。

## 本轮晋升

| 原探针 | 施工7 | 本次 | 精确含义 |
|---|---|---|---|
| `product-store-element` | Types / Unsupported | Verify / Success | 原 `Root{child:Leaf;tag:i32}`，其中 Leaf 有直接 Managed 资源，可作为 Store 元素并执行源码层面的 new/push。 |
| `product-error-union` | Types / Unsupported | Verify / Success | 原 `fn make()->void!Root` 的精确 Unit/Product 返回类型通过。该函数在原源中未被调用，因此这里只是 Source→Verify 观察。 |

第二项属于本轮有限 `Unit!Product` 协议，不表示任意拥有 Sum、任意 E/T 错误载荷或机器清理已经由这个探针证明。相关独立核和机器结果仍读 223 的专属证据。

原 `ct-item-if` 与 `ct-statement-if` 已在施工7晋升，本次保持成功。原矩阵没有 `#for` 或普通 Product 共享借入探针，因此不会用这 29 项的晋升数量衡量 222、224 的全部增量。

## 保留的 12 项 Unsupported

以下是每份原源的实际失败位置和有限含义，完整诊断及原始字节哈希保存在 `RESULT.json`。

| 原探针 | 阶段 | 实际仍缺的这份表达 |
|---|---|---|
| `bytebuffer-slice-capture` | Types | 直接捕获借入 slice 到闭包。 |
| `callback-return-generic` | Expand | 原 F 原样返回；不能把已实现的有限拥有包装工厂与该签名等同。 |
| `closure-owned-error-result` | Types | 原源实际让闭包返回直接资源 R，诊断为 higher-order scalar conversion；该文件名不能用来推断所有 ErrorUnion 结果。 |
| `closure-package-param` | Types | 拥有 Package 作为闭包参数并原样返回。 |
| `optional-parameter` | Types | `i32?` 表面型尚未 lowered；Copy `Option<T>` 库控制仍成功。 |
| `generic-lifetime` | Expand | 具名 life 泛型及其来源签名的语义求解。 |
| `product-owning-store` | Types | 普通嵌套 Product 拥有 Store 的原始形状。 |
| `nested-store-element` | Types | 拥有 Store 的 Package 再成为 Store 元素。 |
| `owning-enum-option` | Types | 拥有 Leaf 的 enum variant；当前门要求有限 Copy payload。 |
| `associated-type` | Types | 有关联类型的 trait，仍超出有限本地 trait 范围。 |
| `dyn-valid-parameter` | Types | 合法 dyn 参数及其地址形状。 |
| `syntax-valid-alias` | Expand | 合法 `syntax use` 及其扩展调用入口。 |

`dyn-parameter` 和 `syntax-extension` 是此前已经识别的无效语法源，本次仍保持原 Parse 拒绝，不用于证明 dyn 或 Syntax 能力缺失；对应能力判断来自后续合法探针。

## 原输入、运行器与证据

原统一运行器是 `tools/language/217/run.py`。施工7的观察脚本在 `evidence/integration/221/readiness-final/raw.tar.gz` 内的 `replay.py`，它同时硬编码旧提交、旧结果汇总并运行额外机器控制。本次保存两份原脚本，使用相同的 29 条 check 命令，更新提供的 CLI/路径身份，分别比较原 oracle 与施工7实际结果；按本次限定范围省略额外机器消费者。

`tools/language/217/PROBES.json`、全部 29 份源码及完整 SOURCE-MANIFEST 的 42 个条目前后哈希一致。旧 RESULT、旧 raw 归档和原诊断完整保存；没有改样本、预期或阈值来匹配新结果。没有请求阶段 IR 输出，当前实际执行为 **29 check、0 native、0 Cargo**。

- `READINESS.json`：精简结果、晋升、剩余项与边界。
- `RESULT.json`：29 项原预期、施工7实际、本次实际、诊断和分类。
- `FROZEN.json`：执行前冻结的来源、原分类与 CLI 身份。
- `CLI.json`：集成方提供的原始构建回执；其中空的 `status` 字段是其干净 Git 状态，成功构建由 `exit_code: 0` 记录。
- `raw.tar.gz`：精确命令、stdout/stderr、运行脚本、环境、旧完整失败及哈希清单。

该盘点仅描述固定旧探针在当前组合上的状态。完整1.4、全部 ZUI 前置、平台验证和产品门继续保持各自未完成或未运行身份。
