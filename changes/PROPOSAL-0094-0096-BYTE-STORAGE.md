# 094–096：泛型组合与显式拥有字节存储（draft20）

2026-10-25（Asia/Shanghai）。用户继续授权语言施工与设计优化同步，ZIL为主体；既有兼容性不是目标，但安全与四目标门不放宽。本提案先于接受性、schema和ABI实现发布。

## 为什么选择这一片

draft19只有固定数组/既存切片的借入窗口，泛型正文仍拒绝范围、可变重借与赋值；这些限制阻碍通用容器算法。仅把malloc返回的Copy c_handle包装成resource不能授予ZIL地址、初始化或拥有资格。新增封闭的字节存储原语，把增长策略与算法留给普通ZIL函数，避免把完整Vec/String API写进编译器。

用户提供的设计线Boundary Lab 0.6.0已取回原包，SHA256为c22948a96d9f9de4a615dcf3314f393a44a3066b2701b6a50fa5b3e15dc6572a。研究提示必须区分Write、Invalidate、Publish和实际清理责任，并保留危险执行前缀；本文据此明确动态存储的发布/失败/窗口边界，不据有限模型宣布更换默认拥有路线。

## 泛型正文

沿现有符号bound检查补充数组/切片索引、范围、长度、可变重借和对合法已初始化place的赋值。绑定可写性与通过&mut写目标分开；let绑定的&mut可修改目标但不能重绑。未约束T不因此成为Copy、数值类型或任意字段提供者。具体实例仍完整经过Types、Analyze和独立Verify，不按某个成功具体实例反推通用规则。旧Unsupported断言逐项迁移为真正正例及最近反例，不移除整组测试。

## 明确的首片源接口

`ByteBuffer`是内建、无公开字段的资源类型：普通参数/值观察为Desc，`give`和create/give结果沿既有Managed规则。不能由用户同名声明、record literal、外部token、整数或原始指针制造。

| 原语 | 类型与行为 |
|---|---|
| `bytes_new()` | 新建Managed<ByteBuffer>，空头，无分配，无可恢复失败 |
| `bytes_reserve(b, capacity)` | Desc<ByteBuffer> × i64 → bool；请求绝对容量；负数返回false；不超过当前容量返回true；分配失败保持头、内容、长度、容量和责任 |
| `bytes_push(b, value)` | Desc<ByteBuffer> × u8 → bool；不隐式分配；已有容量不足返回false且不改变存储；成功先写一个字节再提交len |
| `bytes_len(b)` | Desc<ByteBuffer> → i64；当前已初始化前缀长度 |
| `bytes_capacity(b)` | Desc<ByteBuffer> → i64；当前分配容量 |
| `bytes_view(b)` | Desc<ByteBuffer> → &[u8]；绑定真实存储来源的新共享窗口 |
| `bytes_view_mut(b)` | Desc<ByteBuffer> → &mut [u8]；绑定真实存储来源的新独占窗口 |

原语裸名保留，用户成员名不受影响。参数从左至右恰好求值一次；失败不回滚此前已发生的用户副作用。reserve和push都是对该存储的可写操作，即使本次运行恰好不扩容/失败，静态核验也不能据运行结果跳过窗口冲突。普通ZIL库可以先reserve再push、选择容量增长策略并用bool或已有Copy错误联合传播失败。

## 语义与表示

始终维持0≤len≤cap≤i64::MAX；仅[0,len)已初始化。载体内联为{ptr,i64 len,i64 cap}，Desc指向该实际头；既有唯一责任/live flag仍在载体所在的未释放存储，不进入已释放payload。空头为{null,0,0}，空窗口允许长度0但不能索引。首片元素固定u8，分配大小就是cap、对齐1；不把本片结果外推到ZST、过对齐或拥有元素。

reserve只在准备成功后发布完整新头；使用不调用用户代码的目标分配原语，失败不释放旧块。成功保留已初始化前缀；新分配的额外容量不提前标为初始化。push提交字节与len之间没有可恢复失败、回调或挂起。自动清理先结算唯一责任，再释放payload；不得free后写入payload内记录。Trap仍沿既有abort语义，不承诺unwind清理。

Linux x64使用显式生成的C allocator调用；Win64使用显式目标分配ABI（由本片后端选择并记录，不能默认增加手写C宿主）。原生证据分目标报告，未跑目标不记Pass。没有默认RC、运行时世代、全局ZIL对象表或隐式装箱。

## 独立核义务

ByteBuffer Desc必须从实际载体/合法参数产生。借片来源、真实清理责任和Desc别名映射由Verify内部重建，绝不调用Types/Analyze求证。所有新operand参与结构、SSA、支配、活性、配额和fingerprint遍历。

活动共享窗口排除reserve/push/give/drop及相交写；活动独占窗口还排除冲突读与其它窗口。显式重借保留父子暂停；窗口结束后方可恢复相关资格。普通helper及两个同源Desc参数不能绕过限制。Verify独立重建跨参数窗口/写入所需apart并在实际调用方实例化；优先按真实活性与顺序推导，若某个模式仅有保守摘要则显式记录其安全误拒/Unsupported边界，不把不同形式参数名字当不交。give参数拥有本函数实际载体，不能作为可逃逸的借入Parameter根。旧窗口不能因同址重分配或后来建立新窗口重新合法。允许从Desc参数返回合法借片，调用处替换为实际来源；拒绝返回局部载体裸片。

第一片暂不开放ByteBuffer位于package、闭包capture、聚合/error payload、CT、FFI参数/返回、自定义allocator或异步边界。支持普通Desc参数、Managed give参数、普通create/give工厂及借片返回是本片通用组合义务。裸ByteBuffer不是普通Copy值，MakeManaged/ReadDesc/WriteDesc不能构造或修改它的内部头。

## 公共schema17

Type::ByteBuffer为原始封闭payload标记，type tag24，仅合法地出现在Managed/Desc及已核资源定义中；它本身不授信。资源定义使用唯一名字`@builtin:ByteBuffer`、payload ByteBuffer、teardown None；用户teardown不能替换此内建释放协议。

新Typed指令：ByteBufferNew(63)；ByteBufferReserve{buffer,capacity}(64)；ByteBufferPush{buffer,value}(65)；ByteBufferLen{buffer}(66)；ByteBufferCapacity{buffer}(67)；ByteBufferBorrow{buffer,mutable}(68)。前者结果Managed<ByteBuffer>，reserve/push结果bool，元数据结果i64，borrow结果对应mutability的Address<Slice<u8>>。布尔编码canonical，所有字段完整绑定fingerprint；wire只接受17，Typed域V17。Core继续以未核Call/Named表达原语，不能在Expand授予安全许可。

## 最近反例和准入

必须包含任意头伪造/复制、len越界/未初始化读、活片跨同源helper扩容、借片后give/drop、局部片逃逸、失败reserve保持旧字节、失败push不推进len、相同地址不能恢复旧片、参数求值与失败顺序、正常/提前/普通错误返回准确清理。原生O0/O2使用真实ZIL程序和同合同Rust oracle；分配失败通过明确链接的测试分配器故障注入或真实拒绝路径实证，不把不可重复的内存耗尽当唯一测试。

094负责源面/Types/Analyze/CT与泛型；095负责独立Verify/raw-wire近邻；096由root持公共schema/Lower/LLVM及原生消费者。每片精确提交经非作者审核，正式全量由root统一跑。093冻结schema16实验不迁移或重标；本片生产schema17与研究结论分列。B0、完整语言、产品门保持原状态，不能以完成本片替代终态。
