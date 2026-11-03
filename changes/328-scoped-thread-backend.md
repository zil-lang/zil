# 328：结构化线程的真实 pthread 后端

实现325已冻结的 `WithScopedThread { worker, argument, body }`：只在
`x86_64-unknown-linux-gnu` 使用实际 pthread_create/pthread_join，结果顺序为
`(created, worker_result, body_result)`。沿现有机器模型同时交给LLVM API及文本
参考消费者，不以测试C代码承担生产线程行为。

每个调用位置生成私有helper及pthread start入口。helper栈上固定context保存
已初始化i32参数和独立i32 worker结果；pthread_t按本目标的64位unsigned long
单独保存。child只读参数、调用已核静态worker并写自己的结果。父线程独立消费
body并清剩余环境，成功join之后才读child结果；create/join提供真实POSIX同步。
create失败只清理已准备body环境，返回false/0/0，不调用body或同步fallback。
join失败调用abort并unreachable，不能返回释放仍可能被访问的context。

body沿既有inline环境、按指针/按值调用ABI和closure drop helper；不复制到child。
线程helper引用和静态worker边纳入CGU及layout缓存输入；pthread链接仅新增能力
出现时启用。公共schema/Wire、资格证明及CT关闭由root/325负责，本单不制造权限。

限定验证复用backend lib测试模块：机器IR结构及LLVM核验、真实OS线程执行、
成功/创建失败/加入失败和拥有环境清理、API/reference O0/O2及分区依赖。
真实源消费者待325组合后执行；手造已核前IR模型只按模型证据报告。

本单不自审后端，不宣称Suspend/SABI、Mutex、取消、一般Transfer/Share或产品
四目标完成。原生Windows、Rust运行/内存/维护/编译成本比较未测。

## 作者限定执行与交付

生产checkpoint为`7a1b191e8a6689080a36302fa4a9f03f9e8d264b`；目标/ABI诊断及
分区模型补充为`45e406cd8d7c7b3f5008625db7c1675f0d10d323`，包含325消费者
`98def1064a5da5c665298cb39ac93ab9daab1d7c`。以下均在环境恢复后重新实际执行，
不把丢失的旧/tmp日志当作新执行，也不冒充非作者审核或root组合验收。

加载`/workspace/.zil-env/env.sh`，固定`CARGO_TARGET_DIR=/workspace/.zil-env/target328`
及`CARGO_BUILD_JOBS=1`：

- `cargo test -p zil-driver --features llvm-api --test generic_regressions thread325 -- --nocapture`：
  3/0。真实main分别以Clang/API、O0/O2执行成功/业务错误/create失败/join失败，
  共16次；另父body真实借用控制4次。stdout分别`PECBDJA`、`PECBDJA`、`PECDA`、
  `PECBDX`，join失败为SIGABRT；多模块API O2实际分区且无fallback。
  source/raw/Wire及Analyze N=6871、Verify N=10587的N/N−1/fresh同次检查通过。
- `cargo test -p zil-backend-llvm --features llvm-api --lib scoped_thread -- --nocapture`：3/0。
  原生机器模型Clang/API×O0/O2×三个模式，共12次；这是模型级控制，不当作ZIL源
  接受证据。另核private helper/start共置、worker跨文件依赖及单一定义、目标与
  foreign ABI冲突关闭。`--no-default-features`同过滤3/0，重复参考结果不相加。
- 两种feature配置的`cargo clippy -p zil-backend-llvm ... --lib --tests -- -D warnings`
  均通过；本写集六个Rust文件的`rustfmt --edition 2024 --check`通过。

本次本地日志及SHA256：

| /tmp/zil-328-前缀后的文件 | SHA256 |
| --- | --- |
| source-final.log | 3c719f282adcab586b3b54b6cff16d566311b40f0e3db5c56b636a858b5029a7 |
| model-final.log | 59ef7f2a4c76b569d1d0b4ba455c7de8e6c1c92dde541a7575e943bf34490c25 |
| model-reference-final.log | 95779403d341e628c9c13a0f86d228314cd3b6b996daf136c9e711a723538cad |
| clippy-final.log | a11a1de8037a032bda7e33f88e31dc898d0129cfe94956add639cd289d2ce985 |
| clippy-reference-final.log | e5de9e6aa17fdd5ae05ec69d5e3826ea389ad7975d24d7363dfe7e9ad84333c1 |

第一次模型构建被尚未合入的Verify新variant消费者阻塞；合入325语义后再执行，
未在后端伪造Verify放行。两次环境不可用分别中断了工作，源码checkpoint均保留。
root随后组合324发现其新增FunctionEmitter.copy_values字段需要补模型构造；
该集成适配由root处理，本单独立树的通过不代表后续组合已通过。

本单作者负责机器实现，不自审328。独审由root/324实施者另行绑定精确提交。
实证推进为真实OS线程及创建失败清理，不再停留于同步FFI名字或SC模型；适配器
context无新增heap盒，pthread自身栈/创建成本及Rust性能/内存/维护比较未测，
完整安全/语言/SABI/Mutex与产品条件不升级。
