# 179 作者限定交付

冻结源码：`204fea481d8770bf53f2e81324830b4f375d954e`。这只是作者限定通过，待独立审核与集成方验收。

## 实际变化

真实 Package 的具体自包含闭包字段从整父壳访问足迹细化到 `local + field index`。Analyze 和 Verify 分别重建，公开 Hold 仍保留父壳 Local，父壳存活、同字段冲突和整体移动不放宽。Copy 字段更新因此能与 Store 窗口及另一个 callback 字段并存；Store/ByteBuffer 既有精确字段来源保持。未知用户资源 Drop 仍按整父壳保守处理。

## 实际限定结果

- 最终干净提交：8 个 Rust 测试组，95 通过、0 失败、0 忽略。新增179组10项，内含16次 O0/O2 原生执行。
- 四模块消费者：O0/O2 共18次原生执行，其中8次真实 realloc 失败注入。每次分配探针均为 allocated == frees、live == 0；失败出口和清理字节顺序均核对。
- raw/Wire：合法候选两路通过；删除父壳 Hold、非法字段索引、延长第一个独占窗口穿过第二窗口均拒绝，攻击者重算指纹。
- scoped rustfmt 与 Clippy -D warnings 通过。完整命令、源码身份、stdout/stderr 和退出码见 summary 引用。

## 失败与边界

原始基线误拒、错误阶段断言、示例 pub manages 解析失败、getchar 缺 unsafe，以及宿主 StableCrateId/rlib 缓存碰撞原件均保留，未改标 Pass。最终改用独立空 target，最终五条命令均记录干净树。旧未导入 state.zil 曾在工具调用间重现，先前 dirty 记录照原样保留；最终消费者源码集合和编译器 source_files 均恰为4模块。

此片仅开放已核自包含 callback 的实际字段精度。一般借入或混合捕获仍保持既有 Unsupported；未知用户 Drop 的保守整壳足迹继续存在。未跑正式全量，未宣称完整语言、产品或跨平台窗口生命周期完成：B0 Doing、FullLanguage NotSatisfied、Product NotRun。

## 紧凑证据

本目录不保存原生执行文件、LLVM/阶段展开或缓存。candidate.bin 为不可信 verifier 输入夹具，可用于独立重放，非执行产物。所有实际失败和新179正反例保留源码、收据与观察；邻近组保留完整测试运行输出。manifest.json 逐文件绑定摘要。
