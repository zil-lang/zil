# 089 预登记：B/K/L/KL 的真实解析工作与完整请求成本

2026-10-25（Asia/Shanghai），在实施和任何计时前登记。

强基线为 draft18 `2883f978de79ea0e8a09a4cb218fa68687f4f568`，包含生产068/080，schema15。四版本使用相同功能、相同输入、相同环境：B原生产；K仅token/node成功资格；L仅发现syntax复用；KL两者组合。每版冻结精确Git提交，计数patch另外绑定hash，计时版不含计数器。

沿用 `tests/perf/impl084/cases.json` 的原九个case、十二个case/mode单元：scalar；modules16/64/128/512；modules64-default-capacity；owned_component、helper_ui、byte_arithmetic的Verify和Build。原输入及private/public编辑全部保留，不用语言能力较少的旧compiler作为基线。单进程依序请求cold、unchanged、private_edit、public_edit，编辑写入在API计时外；当前文件读取、发现、构图、hash、资格、Verify、后端与缓存释放都属于相应完整API/进程成本。

正式窗口为每单元B/K/L/KL各一次预热、七轮保留，合计384进程/1536请求；48预热进程/192请求、336保留进程/1344请求。各轮按预定循环/反向顺序均衡变体位置，不删样本、不按显著性追加采样、不删退化规模。冷指空CompilationSession，未清OS页缓存。采样只在root授权独占窗口进行。

报告逐场景API wall、完整进程wall、父进程VmHWM（不冒充clang子进程总峰值）；配对候选/B中位比和确定种子bootstrap探索区间。没有针对显著性提前终止。全部stdout/stderr、命令、source/input/toolchain/variant hash、warmup和失败保留。

正式计时前，先跑四版本同输入功能控制：实际阶段产物、接口、诊断/状态、Verify次数、预算/取消/文件身份/import/缓存/发布与真实原生输出。query_events、cache_hit、cached_query_count可按机制变化，不能作为语义等价证据或解析次数。计数版在lexer、parser实际入口记录调用；B/K unchanged应仍实际解析，L/KL可免解析，默认容量为阴性控制。新CT编辑、token/node边界和FS反例作为功能控制，不临时增删性能样本。

生产准入以功能等价、独立审核和全成本证据综合裁决。单个规模收益不能称普遍加速；冷路径、容量阴性控制与真实消费者回归必须完整披露。图切割在本实验中体现为把“语法值依赖”和“本请求资源资格”拆成不同边界，属于架构假设，不宣称新图论定理或内存管理突破。
