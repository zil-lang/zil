# 323：有限具体 trait 实参与普通 bound 消费

依据 `_zil_design.md` §18.1 与 316，接通 `trait Read<R>{fn read(self:&Self)->R;}`、具体本地 Self 的 `impl Read<i32>`，以及普通函数 inline/where 的 `T:Read<i32>` 消费。关联类型不先行：本片直接扩展现有静态调用，不引入投影或公共 IR。

实例保存真实 trait 声明身份、完整模块名、全部有序规范化实参及实际 Self；透明别名归一，同布局不同名义与 phantom 参数不合并。名义 bound 使用结构化实例引用，与保留 Copy/StoreElement 能力分开。Copy/Transfer/Share/Send/Sync/Drop/Fn 族的原排除不放宽，空 trait/impl 不造结构、清理或线程权限。

首片 trait binder 为普通类型，无 bound 或直接 Copy 前提；实际实参限已有有限普通 Copy 类型。开放参数 `Read<U>`、supertrait、关联项、default/泛型方法、泛型/条件 impl 暂 Unsupported，不丢弃前提。未用 trait 签名按真实 binder 检查，未用 impl 仍核完整方法集、前提、receiver 与代入后签名；泛型正文只消费已有声明 bound，最终静态代码继续经过 Types、Analyze 和独立 Verify。

同 Self 的不同 Read 实参是不同合法 impl 头，各有确定独立方法目标；唯一 generic bound 选择真实目标。直接方法调用遇到多个候选在调用处报歧义，不把声明当作重叠。相同完整实例的重复 impl 拒绝；不新增 specialization、安全失败回溯或外来 Self 权限。

所有索引、类型键/比较、替换、源调用改写和队列增长先收费并累计到请求；命中继续收费，默认限额不增。新 trait 路径不擦除 Reference lifetime 或 CoreFunction.life；与 322 的公共字段及入口由 root 串行整合。未触及新实例的既有路径保持原费用。

限定验证复用已有 trait 测试目标，覆盖原探针、跨模块、多个实参/同布局/phantom、同 Self 实例、unused坏定义、前提/签名/可见性、N/N−1/fresh/冷暖和真实 O0/O2。正式全量、公共规范/schema及集成由 root 管理；未运行项目不登记通过。
