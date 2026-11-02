# MergeFunctions 后续计量工具

作者：semantic_incremental。仅新增工具；未修改生产或既有冻结工具。以下控制不是性能结论。

## 运行质量

stage6_merge_runtime.py 逐字保留 stage6_runtime.py 的生成器（AST身份已核对），仍是6族：512/2048算术、512/2048分支、512长函数体、8192算术。输入来自运行时getchar，5个seed、默认每次20000轮、6个正反交错区组。完整4字节结果与Python独立算术oracle比较，要求退出0/空stderr。

默认运行6个当前编译器arm：off、off_closed、after、after_closed、before_and_after、before_and_after_closed。显式选择任意重复 --arm 参数后，只运行指定集合；baseline、baseline_closed和rust可选。CLI参数分别为 --function-merging off/after/before-and-after，封闭组另加 --closed-executable。不启用 --incremental-o2，避免混入另一个候选。

每个产物仅编译一次，必须当前Verify=1，记录完整编译命令/报告、源/编译器/工具SHA、可执行文件SHA与大小及nm全局定义符号。nm结果明确是全局定义符号表，并不等同动态导出表。构建时间只作诊断记录；6区组测的是同一产物运行时间，不能冒称6区组编译基准。失败构建、超时、错误输出与跳过的运行均保留，不能只看runner退出码。

示例：

    python3 tools/research/incremental-compilation/stage6_merge_runtime.py \
      --zil /absolute/path/to/new/zil --out results/merge-runtime

增加Rust时明确列出所有希望测量的arm：

    python3 tools/research/incremental-compilation/stage6_merge_runtime.py \
      --zil /absolute/path/to/new/zil --rustc /absolute/path/to/real/rustc \
      --arm off --arm off_closed --arm after --arm after_closed \
      --arm before_and_after --arm before_and_after_closed --arm rust \
      --out results/merge-runtime-rust

## 14套真实配对源码

stage6_real_pair_bench.py 从 real_pair_bench.py 复制，保留原14族、15输入、app的Rust --check调用约定，以及完整退出码/stdout/stderr逐项等价控制。默认6个AB/BA区组，每arm完整执行cold→unchanged序列，Rust独立O0 oracle另存，不并入正式计时。实际源码逐字复制与SHA绑定。

--zil-arg 可重复追加明确参数，--semantic-cache为每个独立序列创建自己的可信语义目录；对象缓存/Rust增量目录也按序列隔离。新选项、报告disk_cache和phase事件、可执行文件SHA/大小均进入原始记录。若使用MergeFunctions选项，应显式 --mode O2；不同模式/封闭选项各自冻结窗口。

    python3 tools/research/incremental-compilation/stage6_real_pair_bench.py \
      --zil /absolute/path/to/new/zil --rustc /absolute/path/to/real/rustc \
      --mode O2 --semantic-cache --zil-arg=--function-merging --zil-arg=after \
      --out results/real-after

## 作者功能控制

控制目录为 /workspace/.zil-env/research/stage6-merge-tools，tool-controls.json绑定工具与实际二进制SHA。

- 两个工具均完成Python语法检查。
- /bin/false runtime控制保留1失败构建及5无效运行；real控制保留4无效请求，两个汇总比值均为null。
- 既有529生产二进制（SHA 312bfb11f2babf3abb5c88a4268b3ea8d0241ba7f0f6ccd4c562babaf3ecb8cc）的runtime baseline控制：512算术、1区组、3轮，1有效构建/5正确完整结果，nm与exe身份均有效。
- 该既有二进制的byte_io/O2真实配对控制：1区组4请求全部有效，60次原生观察及15次oracle；ZIL语义缓存cold missing→unchanged hit，每次Verify=1。
- 上述小控制不用新MergeFunctions实现，也不构成其运行质量或性能证据；正式新候选尚待固定源码后测量。

另只读核验 stage6-semantic/shape-sequences-functional：230请求中220有效（ZIL105、Rust115），10拒绝分别是body-64x16/types-64x16各5次原预算失败。105成功ZIL全部Verify1；21族的cold/early_edit/late_edit均missing，unchanged/restore各21次hit；全部220原生结果完整4字节、退出0且stderr为空。源身份和冷/恢复fingerprint一致性均核对，收据为shape-functional-audit.json。此窗口只作功能控制，不作计时结论。
