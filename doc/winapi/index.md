# Windows Pointer Injection 参考

本目录整理 Windows Synthetic Pointer Injection 相关 API，供 PC 端把协议输入映射为系统触摸或笔输入时查阅。

## 核心调用链

1. 调用 [CreateSyntheticPointerDevice](./device-and-injection.md) 创建注入设备。
2. 按输入类型填充 [POINTER_TYPE_INFO](./common-pointer-types.md)。
3. 根据类型补充 [POINTER_TOUCH_INFO](./touch.md) 或 [POINTER_PEN_INFO](./pen.md) 的专属字段。
4. 调用 [InjectSyntheticPointerInput](./device-and-injection.md) 注入一帧输入。

## 输入类型速览

- 触摸输入使用 [PT_TOUCH](./input-types-and-limits.md)，支持多点，`count` 必须大于 0 且不超过 [MAX_TOUCH_COUNT](./input-types-and-limits.md)。
- 笔输入使用 [PT_PEN](./input-types-and-limits.md)，`count` 必须为 1。
- 公共坐标和状态字段位于 [POINTER_INFO](./common-pointer-types.md)。
- 触摸和笔的可选字段分别由 [TOUCH_MASK](./touch.md) 和 [PEN_MASK](./pen.md) 声明有效性。

## 文档导航

- [设备创建与注入](./device-and-injection.md)
- [输入类型与限制](./input-types-and-limits.md)
- [公共指针结构与标志](./common-pointer-types.md)
- [触摸输入结构](./touch.md)
- [笔输入结构](./pen.md)