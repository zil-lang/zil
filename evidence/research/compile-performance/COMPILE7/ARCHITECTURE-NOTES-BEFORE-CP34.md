# zil 编译 7：架构差距与下一阶段优先级

本说明仅静态读取已冻结源码和既有 observer 证据，不包含新测试或新性能样本。结论时点：CP32、CP33 尚未纳入完整正式窗口结果；“已实现”指对应研究候选已有代码，不代表已采用或主线完成。

## 目前实际具备什么

| 层次 | 已实现 | 当前边界 |
| --- | --- | --- |
| 持久会话 | `CompilationSession` 跨请求保留私有、不可变 raw 快照；DB 能表达五种结果指纹和依赖。 | driver 的实际依赖边仍全部观察 `PrivateImplementation`；`abi` 字段在这一层填的是 target 身份。已有字段不等于已实现按签名/布局/effect 停止传播。[S1] |
| CP29 / CP32 | CP29 在完整环境相等、整个模块 Core 相等时，复用普通标量函数的 Builder 结果；CP32 进一步复用资格/权重收据、延后复制并在 64 MiB 限额内保留部分模块。 | 复用选择仍是模块粒度，完整声明、签名表、资源 effect、导入、输出组装及其它全图阶段继续执行。CP32 的这些改善已有代码，不应重复列为尚未实现的新切片。[S2][S3] |
| CP33 | 成功 Parse 的 token/node 预算从身份中分离，通过真实 usage receipt 判断当前额度是否足够；额度不足回原 parser。 | 其它限额、源字节、path、FileId、ModuleId、规则与 build inputs 仍绑定；后续阶段保持完整预算身份，当前路径查找和歧义检测继续执行。没有新增磁盘 AST 缓存。[S4] |
| 独立验证 | 每次到达 Verify 的请求重新执行验证；会话 raw 枚举不能持有 `VerifiedModule` 权限。 | 验证前仍复制整个 TypedModule 和 CandidatePlan；验证内部仍计算指纹、执行检查。其具体成本需要另开诊断，不能从 observer 间隔直接拆算。[S1][S3] |

CP33 针对的原因已经定位：项目前部文件增加 token，使后续文件的剩余 token/node 额度改变；旧 Parse 键因此使未改源失去命中。既有 discovery 本来可以借用匹配的 syntax，不能描述为“原来总是全部重新解析”。低容量清退后借用 syntax 再发布也可能计为 query Executed，查询计数不等于 lexer/parser 的实际调用次数。[S4]

## 下一步优先级

**1. 先完成现有候选的总成本裁决。** CP29 的完整窗口未建立净收益；observer 虽看到 private 编辑少做大量 Builder，但类型阶段只节省局部工作，公开环境改变又承担重建和留存成本。CP32、CP33 必须分别对同一基线判断默认容量、足够容量、冷请求和完整编辑序列的 wall/CPU/RSS，不能只比较命中数，也不能只证明比 CP29 更快。不要提前叠加候选以掩盖各自成本。[S3][S5]

**2. 单独诊断并收窄全图物化成本，继续保留 fresh Verify。** 既有 observer 在足够容量的全 query 命中请求中仍看到明显 Verify 尾部成本；源码确认其内存在两次输入深复制，且 TypedModule 指纹先完整编码到 Vec 再哈希。优先用独立诊断构建区分 clone、编码/哈希、检查本体，再选择不可变共享输入或等价的流式指纹实现；这些仍只是设计，不可据此跳过验证。其次检查无 CT 替换时的全 Core 重建，保留原 selection、作用域、hint、费用与错误顺序。已有 `verify`、`execute`、`snapshot_identity`、`contract_publish` timings 可作外层入口，新增细分只放在单独诊断源码和二进制中。[S1][S3]

**3. 推进真正按语义依赖复用的查询图。** 先从普通标量函数的窄范围闭环做起，不直接承诺全语言红绿算法。需要把稳定 owner 与当前源版本分开，并处理插入/重排造成的身份漂移：当前普通函数 ID 由全项目声明偏移构造，缓存键同时绑定源位置，直接按现有数字 ID 留存函数体并不稳定。[S1][S6]

| 依赖边界 | 下一切片应表达的事实 | 必须保留的失效条件 |
| --- | --- | --- |
| 函数体 | body 查询依赖本体和实际读取的声明/契约；只改私有实现时优先局部重算。 | 推导结果或 effect 变化必须传播；诊断源位置要更新，不能把含旧 Span 的 IR 直接当当前位置结果。 |
| 公开签名 | 消费方只依赖所用导出签名；重算后签名结果不变可停止这条边的传播。 | 参数/结果类型、传递方式、可见性、导入解析改变。当前已有 scalar signature fingerprint 可作为起点，不能把全文 hash 当语义结果。 |
| 布局 / ABI | 独立表示目标相关的真实布局依赖。 | 闭包环境、资源/聚合布局及调用 ABI 改变；仅签名表面相同不保证布局相同。 |
| effect / 调用契约 | 将资源写入、借用/调用契约和外部函数合同列为实际读取的依赖。 | body 引发 effect 变化、foreign 合同变化与递归依赖更新；不能因签名未变就默认安全复用。 |

上述细粒度调度与结果不变停止传播尚未接通现有 driver；数据库指纹种类和已发布接口是基础设施。先完成单个边界的控制：私有编辑、签名同结果重算、真实签名/布局/effect 改变、插入重排、A→B→A、失败/取消和额度边界，再谈扩大范围。[S1][S6]

## 产品和测量范围

CP31 正式 harness 在同一进程保留一个 Session，计完整 `compile_path` 到 `Stage::Verify`，包括本次源读取和 discovery；编辑快照复制与 session 初始化单列，完整子进程 wall/CPU/RSS 也保留。已有 observer 改用 `compile_project_with_observer`，把源码读取与 ProjectSources 准备放在计时外，且事件发出后还有 identity/publish 收尾，因此它只能定位结构，不能替代正式总成本。[S3][S5]

一次性 `compile_path` / `compile_project` 自由函数每次创建新 Session；当前 CLI 跨进程调用不能自动获得持久会话的 warm 收益。若产品目标是连续编辑编译，需另做显式保留 Session 的调用入口及生命周期/取消/工作区边界；若目标是一次性 CLI，需继续单独优化和计量冷路径。常驻库 API 已有，完整常驻构建服务仍是设计。[S1]

**CP33 的 Parse 复用或到 Verify 的前端收益，不代表完整 AOT 提速，也不构成超过 Rust 的证据。** 当前窗口没有计入 LLVM 发射、优化、对象文件、链接与程序运行；需另行冻结这些端到端边界、优化等级和运行结果，才能讨论与 Rust/Blaze 的相应比较。[S5]

## 可核对的已有证据

- **[S1]** CP33 `83854ae83f7fec4fc8f9c22d0751a1fd2725671f`：`zil-cp33/crates/zil-driver/src/session.rs`（`RawSnapshot`、`query`、`finish_candidate`、`parse_identity`），`lib.rs` / `project.rs` 的自由函数入口；`zil-cp33/crates/zil-db/src/lib.rs` 的 `QueryKey` / `FingerprintKind`。
- **[S2]** CP32 `3ba131efdeffe85ba5fccd1c177745d812a6ce61`：`zil-cp32/crates/zil-types/src/program_memo.rs` 的 `Request::with_capacity` / `record` / `finish`。这里只确认源码机制，不预判其正式性能。
- **[S3]** `recovery/cp29-observer-evidence/FINDINGS.md`，SHA-256 `b93b806669c3454d0f55078e3b875b433a68d2a36d79657e96486476867938f2`；独立 56 请求诊断，具体计时限制详见原报告。
- **[S4]** `zil-cp33/docs/research/compile-performance/CP33-PARSE-BUDGET-RECEIPTS.md`；`recovery/cp33-author-validation.json`；CP33 五个生产文件的额度收据和双入口键实现。本文未将限定正确性控制视为性能验收。
- **[S5]** 已冻结 `recovery/compile7_tools/compile7_session.rs` SHA-256 `3aaf249ad90b67029bacedc66523ce149c230099af27fdc22ed2a6074ee5a37f`，`compile7_session.py` SHA-256 `6ec96a3b6fdf0ade3e3ffaadff8a29c7c46d5b805941175fcf8d78e2d889a7c3`；CP29 总窗口裁决见主树 `docs/research/compile-performance/COMPILE7-PLAN.md` 的 CP32 派工段。
- **[S6]** CP33 同提交 `zil-cp33/crates/zil-types/src/program.rs`：`check_program` 的声明偏移、全图 effect/可见性工作、`ScalarInterface` 和 `interface_fingerprint`。

以上相对路径均以 `/workspace/scratch/c3607737969d` 为根；主树计划路径以该根下 `zil/` 为起点。
