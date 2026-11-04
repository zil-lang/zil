# 标准库及产品消费者终态实施

先把已有真实UTF8验证/事务编辑实现复用为普通ZIL源库，增加拥有String的安全封装、明确错误/分配失败、只读视图/移动与标量消费接口；随后接泛型容器/迭代、真实布局/输入/渲染消费者。此计划不等于完整标准库或ZUI完成。

stdlib模块按明确版本随编译器分发，std.string/std.utf8/std.text_edit是实际普通源码，不具有编译器权限。导入后逐字节计费、正常parse/Expand/Types/Analyze/Verify并进入源码/缓存身份；无磁盘同名劫持、无隐藏宿主实现或跳过核验。手工ProjectSources也必须提供真实源和图。现有std.meta/std.syntax特殊产物协议保持。

最近反例包括非法UTF8/过长序列/代理区、标量与字节边界混淆、分配失败改写旧值、活跃view时增长/移动、私有存储伪造、模块/缓存源码失配。产品场景仍按既有冻结门执行，原生Windows缺真实主机不以Wine或未运行工作流替代。

std.store复用显式容量Store原语，新增有序remove：先完整准备suffix容量，失败前原输入保持；真正移出一个T并将suffix按原序恢复，所有拥有值按既有责任处理。不引入隐式扩容。实际带Drop消费者、活跃view反例与逐分配点失败验证后才接受。

std.iter接同步Copy slice callback for_each/fold/any/all/position/find，依真实FnMut调用顺序和may合同；position/find返回卫生Option，空输入/提前退出/环境写入与相交回调反例必须执行。此源库段不替代一般Iterator关联类型或lending iterator终态。

UTF8/String接续：普通库补充Unicode标量编码、值域与输出容量在写入前核验；非法标量或短输出保持原字节。String显式reserve、Unicode标量push/truncate及拥有substring复用真实资源头权限，保持UTF8不变量和失败前不改原值；输出字符串独立拥有，不延长输入借用。完整get/borrowed substring错误载荷及lending仍须真实语言消费者支持，不能以拥有副本冒充返回借用。

Ordinary Store insertion/truncation: insert preserves existing element order and
requires caller-provided spare capacity; it never grows the input implicitly.
An invalid index, insufficient capacity, or suffix preparation failure returns
the actual incoming T without changing any input content or capacity. It first
prepares the whole suffix, then transfers owners using the existing Store
operations with proved capacity. truncate drops the removed suffix in reverse
index order; invalid bounds leave the input unchanged. Actual owning payloads,
all preparation failure points, and active-view counterexamples are required.

UI library continuation: ordinary `std.ui_layout` supplies validated integer
pixel rectangles, intersection/hit testing and bounded row/column tracks. Track
min/max sizes and equal flexible growth are explicit; validation and sizing
finish before any output rectangle changes. Space exhaustion/invalid input
returns an ordinary error without mutation. No implicit allocation, native
handle or compiler permission is introduced. `std.ui_raster` will consume the
same clipped coordinates for an explicit caller-owned RGBA framebuffer; input
composition will reuse transactional UTF8/String editing. These library kernels
are prerequisites, not evidence of a native window, IME service or full product.

The layout/raster candidate executes both backends at O0/O2 (8 native builds/runs).
Raster uses premultiplied RGBA and validates the entire affected destination
before source-over blending; row padding and untouched pixels are preserved.
The ordinary input composition candidate retains preedit separately, uses scalar
indices/revision checks, and commits through String's transactional replacement.
Its original consumer currently exposes the missing mutable nested Package-field
borrow (`string.clear(&mut session.text)`);342 is implementing that actual language
path. This UI source is preserved without flattening its nested String ownership.
No native window/OS IME or complete ZUI acceptance is claimed by these kernels.
