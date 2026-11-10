# 普通 POD 哈希容器

`std.pod_hash_map` 与 `std.pod_hash_set` 用普通库源码实现固定容量插入、按需增长和完整重建。增长先在独立 owner 中完成分配与 rehash，成功后才替换原容器的字段，使准备阶段的失败保留原容器。

这两个模块走通常的导入、类型、Analyze、Verify 与代码生成路径。标准库模块名不授予额外权限；每次请求仍依据实际源码、类型、来源与合同完成独立核验。Store 的移动、借用和字段替换规则见[普通 Store 所有权](ordinary-store-ownership.md)。

## 元素与回调合同

Map 的键和值都要求 `Copy + StoreElement`，Set 的键也如此。`PodHashSet<K>` 封装 `PodHashMap<K, bool>`，共享探查和重建机制。这一边界不包括任意拥有资源的键、值，也没有逐元素资源析构合同。

查找和修改显式接收 `equals: Fn(K, K) -> bool` 与 `hash: Fn(K) -> i64`。调用方应提供一致的键关系：相等键具有相同哈希；相等判断与哈希在键存储期间保持稳定。`Fn` 约束调用能力，不能证明数学上的等价关系、纯函数性质或哈希稳定性。

库不保证比较与哈希回调的调用次数。碰撞、已有键替换以及重建都会影响次数；不能依赖“每次插入恰好调用一次回调”。准备失败保留容器，并不承诺撤销回调已经产生的外部效果。

## 存储与探查

Map 拥有两份 Store：紧密排列的键值项 `entries`，以及桶索引 `buckets`。桶标记 `0` 表示空，`-1` 表示删除留下的墓碑，正数表示对应的紧密项索引加一。碰撞采用线性探查并调用 `equals`，最多遍历一整圈桶。

起始桶通过 `hash(key) % bucket_count` 计算；负余数再加桶数。该实现不对哈希取绝对值，因此不会因最小负整数取反溢出。桶数为 `2 * capacity + 1`，在构造前检查容量范围。

`capacity` 是允许容纳的不同键数量，不是分配器实际预留的字节数，也不是可供用户直接索引的桶数。合法范围为 `0..=4611686018427387903`，即 `0..=2^62−1`；零容量仍有一个空桶，可以随后增长。

## API

以下 `Error` 为 `pod_hash_map.Error`。所有快照值都按 Copy 返回。

| Map 操作 | 结果与行为 |
| --- | --- |
| `with_capacity(n)` | 返回 `Error!PodHashMap<K,V>`；建立两个 Store，预留所需容量并清零桶 |
| `len(map)` / `capacity(map)` | 当前不同键数量 / 逻辑容量 |
| `get(map, key, equals, hash)` | 命中返回 `Some(value)`，否则 `None` |
| `at(map, index)` | 紧密项的 `Option<Pair<K,V>>` 快照；负数和越界返回 `None` |
| `insert(map, key, value, equals, hash)` | 固定容量插入；替换返回 `Some(old_value)`，新增返回 `None` |
| `insert_growing(map, key, value, equals, hash)` | 返回语义同 `insert`；新增键需要空间时先增长 |
| `reserve(map, n, equals, hash)` | 将容量至少提升到绝对目标 `n`，返回 `Error!void` |
| `remove(map, key, equals, hash)` | 删除并返回 `Error!Option<V>` |
| `clear(map)` | 返回原项数，清空内容和墓碑，保留既有分配与逻辑容量 |

| Set 操作 | 结果与行为 |
| --- | --- |
| `with_capacity(n)` | 返回 `Error!PodHashSet<K>` |
| `len(set)` / `capacity(set)` | 当前不同键数量 / 逻辑容量 |
| `contains(set, key, equals, hash)` | 是否存在相等键 |
| `key_at(set, index)` | 紧密位置的 `Option<K>` 快照 |
| `insert` / `insert_growing` | 返回 `Error!bool`；新增为 `true`，已有键为 `false` |
| `reserve(set, n, equals, hash)` | 绝对容量目标，行为同 Map |
| `remove(set, key, equals, hash)` | 返回 `Error!bool`，表示是否删除了已有键 |
| `clear(set)` | 返回原键数，保留分配与容量 |

固定容量 `insert` 不自动增长。已有相等键的替换即使在满容量时也可进行；实现替换整条键值项，因此键的代表值也更新为此次传入的键。Set 沿用这一行为，不能依赖首次插入的键对象一直保留。

`at` 和 `key_at` 暴露的是当前紧密槽位的快照，不是稳定游标或顺序保证。删除会将最后一项移入空洞，并修正指向它的桶；已有位置可能改变。

## 增长的准备与提交

`reserve` 的参数是绝对目标容量。它先检查合法范围；目标不超过当前容量时直接成功，不缩容、不重建。需要增大时，依次执行：

1. 构造独立的目标 Map，完整分配 entries 与 buckets，并清零桶。
2. 读取原 entries 的 Copy 快照，逐项插入目标 Map，重新计算探查位置。
3. 准备全部完成后，将目标 entries 与 buckets 的 owner 依次转交给原 Map。
4. 最后更新逻辑容量，完成提交。

准备阶段没有修改原 entries、桶或容量。准备失败时，临时 owner 按普通清理规则释放，原内容与分配责任仍属于原 Map。提交中的字段替换继续受普通 Store 的来源、借用和退休规则约束；这不是允许任意 owning 结构绕过核验的通用事务接口。

`insert_growing` 在满容量时先查找键。已有键直接替换，不增长；只有新增键才选择新容量：从零增长到一，此后通常翻倍，接近上限时取合法最大值，避免算术溢出。完成 `reserve` 后再执行固定容量插入。

传入的键和值都是 Copy 值，因此增长准备失败不会消耗调用方的 owning 值。这里没有承诺“返回一个被移动的资源 owner”；那是其他 owning 容器 API 需要单独定义的合同。

## 删除、清空与错误

删除先定位目标，再取出最后一项；若目标不在末尾，用最后一项填补空洞，同时修正其桶索引。被删除键的桶保留墓碑，使碰撞链中后面的键仍可查到。`clear` 则清空紧密项并把所有桶置零，移除墓碑，保留容量以便复用。

错误分为 `Capacity`、`Allocation` 和 `Internal`。`Capacity` 表示容量参数或固定容量限制；`Allocation` 表示准备 Store 分配失败；`Internal` 标识已建立的内部库存或探查约束不符合预期。不能把任意 `Internal` 都解释成全局回滚：例如删除路径可能已经完成一部分原定修改。完整准备后才提交的保留保证，针对 `reserve` 的准备阶段。

回调错误传播、并发修改、共享内部可变性和一般 owning 键值不在这些接口的合同内。容器结构变化仍须遵守现有借用规则，不能持有旧分配的有效借用或描述符而跳过最后使用点检查。

## 核验边界与回归控制

库通过实际可变输入和普通 Package 字段完成替换。跨调用、公共回调、捕获和返回来源必须由两核分别还原；同型地址不等于同一实际 endpoint，静态类型资格也不能授予路径上的动态写权限。替换后的旧描述符、借用和代际事实须按实际来源失效。

回归控制保留完整源码输入，分别检查碰撞与负哈希、已有键替换不增长、增长后所有旧项可达、删除填洞、墓碑复用、清空后复用，以及逐个实际分配点的失败。失败控制应同时观察旧内容、容量和重试结果，native 还要核对真实分配/释放及退出状态；仅得到“编译成功”不能替代这些语义检查。

源码入口为 [Map](../lib/std/pod_hash_map.zil) 与 [Set](../lib/std/pod_hash_set.zil)。[Source/native 控制](../tests/language-readiness/terminal/library-and-ui/growing_hash_collections.rs)使用独立输入检查真实对象生成与分配责任：默认覆盖 Clang，开启 `llvm-api` 时增加 LLVM API 路径；[回调控制](../tests/language-readiness/terminal/library-and-ui/growing_callable_effects.rs)约束跨调用效果。这些测试是接口维护入口，不扩大上述语言边界。
