# 102：拥有字节载体进入原 Roam 差分队列

## 实际改变

沿用同一个byte-buffer-native profile、监督worker、冻结快照、hunt、issue重放、
同性质缩减、坏候选拒绝和回归关闭。新生成输入明确使用
`zil-byte-buffer-native/2`、`owned-byte/1`、`byte-buffer.owned-c-model/1`；旧 `/1`
保留原Rust/C语义和输出。固定旧case的hunt scope读取实际case合同，避免标成新版。
生产编译器、语言、schema17、依赖锁、其他profile与默认/硬预算均未修改。

两个逻辑字节槽的同一动作数组分别进入普通局部、Package、保存FnMut和保存
FnOnce。除reserve/push/fill/relay/reset外，新增先准备后替换、取走后补入、
准备临时后的普通错误，以及部分消费正常/错误出口。FnMut错误后真实继续调用；
FnOnce真实消费拥有环境，不依赖尚未开放的opaque callable模式适配。

Managed局部直接赋值仍Unsupported。局部发射器使用既有give参数，把两个真实
独立拥有者经有界普通函数依次转交；新值准备成功后消费旧值，最后参数帧按
逆序清理。Package/保存环境使用真正字段take/refill/replace，并按字段序清理。
FnMut加入有限Copy调用计数，使删除所有写动作后仍是实际FnMut，不伪造模式转换。

## 分配、身份与独立参照

Rust模型维护Vec、逻辑容量和责任编号；C参考独立解释操作表，使用有界数组及
自己的分配/释放事件规则。参考C不读取Rust期望、ZIL源码或机器观察，不是通用
生产降C后端。四个实际执行仍为ZIL/C各O0/O2。

ZIL测试可执行文件链接内置malloc/realloc/free观察器。它按真实指针查找当前
活责任，成功新分配产生逻辑ID，realloc保留ID并更新指针映射，失败保留旧映射，
释放先退役ID再调用真实free。未知/重复free明确失败，结束必须零遗留；不比较
跨进程地址。clang proxy只接受本worker内置观察器的逐字节内容与hash，实际完整
argv、观察器版本/hash、源码、LLVM/二进制和输出进入既有nativeEvidence。

failAt=0不注入分配失败，随机生成保持此值。正值只控制第n次实际增长请求；
容量足够或负数不算分配尝试。计划OOM与语义变异分别标记，观察器非计划宿主
OOM为工具环境error。观察器仅是测试shim，不进入语言运行时或安全核。

## 核验和失败保留

先冻结提案，再实现。限定回归为byte_buffer六项、pipeline十项、resource_closure
四项、scalar五项；fmt和clippy all-targets -D warnings一并运行。新增原生矩阵
包括四载体、首分配/增长/替换失败、错误后恢复、部分FnOnce、最大八程序及空
FnMut，共20程序/80次原生执行；它们与随机hunt、受控闭环分别计量。

最初两次发射错误分别为必返错误分支后又生成return（unreachable Unsupported），
以及只修改环境却声明opaque FnOnce（既有模式适配Unsupported）。均为测试发射器
问题，原case、实际源码、C参考与结果保留；修为受支持的真实控制流/拥有消费，
未修改生产规则，不登记为自然编译器缺陷。首构建因工具链缺失的exit127、一次
未格式化补丁导致的fmt拒绝同样保留，不覆盖早期结果。

精确源码/工具身份、实际命令、冻结无注入hunt、计划OOM、受控缩减/关闭及完整
原件清单以evidence/impl/102/acceptance.json为准。作者不运行正式全量、不合入，
生产问题只保留并交root派修；最终提交交非作者独审。

## 剩余边界

仍是有限双槽u8存储，未覆盖直接ByteBuffer保存捕获（本片保存的是Package）、
借入捕获、环境裸slice返回、拥有错误载荷、一般元素析构、任意分配器、并发/
取消或Windows。有限差分与真实释放观察不证明全语言安全或相对Rust成本优势。
完整语言NotSatisfied、产品NotRun、B0 Doing保持。
