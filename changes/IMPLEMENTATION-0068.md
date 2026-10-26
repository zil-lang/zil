# 0068 编译请求内的纵向共享与重复成本

2026-10-24（Asia/Shanghai）。冻结基线：400a082444275e1d0e2ad78c4910b9b6fc526be5；schema/wire10；产品门保持原字节。

## 为什么

真实调用链已发现：`compile_path → load_sources` 为发现 import 完整解析每个文件，随后 `project_pipeline` 再解析相同不可变 `SourceUnit`；`ProjectTyped.module` 为进入统一 `finish_typed` 深复制一次，Verify 又为取得独立拥有输入复制一次；各阶段重复计算同一输入、规则、target 的查询身份。上一轮惰性 dump 已解决未请求文本输出的格式化，不能再次把这项旧收益算入本轮。

## 本片实现合同

1. 文件加载保留同一请求内的 `Arc<ParseResult>`，经私有通道交给正常项目查询。通道只含未核语法，不含 Verified 或 Analyze 证明。仍先完成全项目 source/产物保护，再发布缓存；import 发现失败不得留下已发布 raw 查询。读取、import 解析、按唯一文件累积的字节/token/node预算和错误顺序保持。Parse-only 仍只读根文件。公开 supplied-source API 仍自行解析。
2. `finish_typed` 接收只读 `&TypedModule`，项目和单文件流水线共用。去掉项目侧中转深复制；Verify 原有独立输入复制与完整核验保留，Lower/Backend可信边界不变。
3. 私有请求身份保存内容身份、规则指纹和target指纹，逐阶段只填phase/是否依赖target。project身份保持原完整源码图指纹。改变源码、文件/模块/路径、限额、build输入或target，仍按原键失效；不把输入身份冒充语义结果指纹。

查询结果的原有完整digest、独立Verify、取消检查、stage输出字节及原子发布均保留。本片不引入全局可变通道、不跨进程传可信状态、不改变生产ABI/IR，也不声称实现完整red-green增量或消除其他阶段全部复制。

## 必须验证

- 文件路径与显式源码图得到相同阶段文件、公开接口和native行为；含目录模块、同文件别名和普通失败。
- 语法、缺文件、非法import、配额、取消、路径碰撞仍在对应阶段拒绝，失败不污染缓存或产物。
- 冷/热请求、依赖私有正文改动、target/预算/build身份变动仍产生预期查询事件；每次到Verify仍实际执行一次。
- 冻结输入和二进制，在独占窗口配对测完整API调用；明确预热、冷/热、路径读取是否计时，并保留回退。最初限定测试不用于性能宣称。

## 参考与边界

Rust 的共享编译上下文/arena/interning与Salsa不可变查询返回提供表示复用经验；本片只引入请求私有raw数据复用，没有引入其全部框架或借助宿主借用检查器核验ZIL。参考原文与更大纵向机会记录于docs/research/068。
