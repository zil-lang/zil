# 独立候选并发 screen 源码审核

结论：**Pass，可按指定小范围执行首轮并发 screen；没有发现阻断该用途的逻辑问题。** 本审核只读取最终 concurrent_screen.py，并实算其 SHA256，没有修改脚本、构建或采样。通过仅表示测量器逻辑可继续验证，不表示生产候选的性能或运行质量门通过。

精确文件：`/workspace/scratch/abf690ce880c/out/multi-edit/concurrent_screen.py`

SHA256：`79f9befbbca74b88383547ce352794bc8b79ea5f9dc391ce78a22942b4129919`

首轮范围：files=256、blocks=1、include-rust，k=16/64、c=1/2/4，顺序 ARBBRA；本审核未将其他范围作为已执行结果。

## 同总资源与独立候选

总 affinity 固定为 CPU0..3。每个 c 用 4/c 大小的连续、不重叠 CPU slices；c=1 为单候选四 CPU，c=2 为两候选各二 CPU，c=4 为四候选各一 CPU。ZIL jobs 同步设为 4/c，Rust 继承同一 slice，CGU 均为 8。没有把四候选额外分到四组四 CPU。

每个 arm-position、cell、candidate 都有独立 source、build/program、objects、semantic cache 或 Rust incremental 目录；没有共用 cache 去人为制造候选间命中。候选1..4使用不同 salt，prepare 检查四份最终源 hash 身份互异，并核对每种语言相对 base 的实际变更文件集合。c=1/2/4 分别取前1/2/4份候选，总工作量随 c 增加；跨 c 的结果要按实际批量工作解释，不能把 c4 的批次 wall 直接当作同工作量 c1 的延迟比。

盐值变更仍只作用于每 selected 文件的两个函数体。数值 oracle 独立按 seed0/65/255 执行四轮乘加模251，并按 salt 计算真实语义；不从 ZIL/Rust 输出或缓存 artifact 学习答案。当前N/k/salt范围远低于i32与u32总和边界。

每个候选先在其固定 source 路径做 final clean oracle，再改回 base、顺序 seed 自己的缓存、批量发布最终选择文件。所有候选准备完后，batch 一次启动全部最终编译；最终计时区内没有 source 写入。源码、输出、缓存、候选最终内容均互相隔离。

## A/R/B 比较和当前正确性

include-rust 使用实际 ARBBRA：A为基线二进制，B为候选二进制，R为固定 rustc；不是把A结果当Rust。A/R/B各两批、位置均关于批次中点对称。Rust固定O2、8CGU、overflow checks、panic abort、debuginfo0、正常incremental以及同一clang linker；其clean无incremental、seed/final正常incremental，输出始终按独立oracle核验。

ZIL固定LLVM API、完整O2、incremental-o2、after，jobs随每候选slice变化；每个ZIL clean/seed/final都调用已冻结helper的validate_zil，要求Success、Verify1、实际N文件、每unit当前完整preflight及LLVM前后核验/合法producer证据。prepare冻结concurrent脚本、fixture/screen与全部工具二进制哈希，run实际重算，读取Rust完整版本并要求1.99.1，ZIL receipts要求LLVM23.1.4。source identity仍需由root构建收据支撑，脚本的source_identities只是显式记录。

B的clean oracle由A编译同一候选source路径，保持同jobs/slice、相同fullO2策略且不启用object/semantic缓存。最终每个ZIL artifact必须与对应A clean oracle的完整ELF相等，material sidecar只去唯一诊断fingerprint行后全部字节相等；原生功能正确不能覆盖同码比较失败。Rust clean与incremental ELF身份也记录，但不强迫两种Rust生成模式逐字一致。三个输入向量只验证功能，不是runtime吞吐或Rust90门。

## 计时与失败证据

batch在启动前记录统一起点，先Popen全部候选再wait4(-1)逐个回收，没有poll/communicate提前吞掉usage。各Popen对象的returncode在wait4后设置，避免析构器重收；记录batch-start到reap、individual-launch到reap、launch skew、每子进程CPU与RSS。CPU为wait4各子进程及OS记账后代的和；RSS明确是最大单进程值，未伪称并发聚合峰值。

每次batch的finally都会在post-validation前保存batch-attempt.json：已回收rows/CPU、尚未完成或未启动specs、异常、强制清理rows/CPU、清理错误及清理时间。超时/启动失败只向本runner创建的独立进程组发SIGKILL，再逐个wait4；未把未启动候选计为完成。

clean、seed、final都在validate之前另写oracle-raw、seed-raw、batch-raw，stdout/stderr始终直接落文件。验证或同码断言失败仍保留原始批次数据与attempt；外层finally写completion complete=false及failure，剩余请求不会计通过。cells.json只是已完成批次的原始观测数组，没有失败后生成成功总体比例；分析时必须先检查completion。

脚本不进行原发生频率加权，也不借此认领原84请求、全模式或runtime门。blocks=1每arm只有两批，这是预先限定的有限screen；不应把少量批次推成稳健总体因果结论。实际是否能跑通、收益是否存在仍待指定窗口的数据。
