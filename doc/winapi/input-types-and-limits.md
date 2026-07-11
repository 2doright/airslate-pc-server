# 输入类型与限制

## POINTER_INPUT_TYPE

`POINTER_INPUT_TYPE` 用于标识指针输入类型。

### 定义

```cpp
typedef enum tagPOINTER_INPUT_TYPE {
  PT_POINTER = 1,
  PT_TOUCH = 2,
  PT_PEN = 3,
  PT_MOUSE = 4,
  PT_TOUCHPAD = 5
};
```

### 常量

| 常量 | 值 | 说明 |
| --- | --- | --- |
| `PT_POINTER` | `1` | 泛型指针类型，不会直接作为真实输入类型使用。 |
| `PT_TOUCH` | `2` | 触摸指针类型。 |
| `PT_PEN` | `3` | 笔指针类型。 |
| `PT_MOUSE` | `4` | 鼠标指针类型。 |
| `PT_TOUCHPAD` | `5` | 触摸板指针类型。 |

## 注入相关常量

| 常量 | 值 | 说明 |
| --- | --- | --- |
| `MAX_TOUCH_COUNT` | `256` | 系统允许的最大同时触摸 contact 数量。 |
| `TOUCH_FEEDBACK_DEFAULT` | `0x1` | 默认触摸可视化。 |
| `TOUCH_FEEDBACK_INDIRECT` | `0x2` | 间接触摸可视化。 |
| `TOUCH_FEEDBACK_NONE` | `0x3` | 不显示触摸可视化。 |

## 当前实现最常用的约束

- 创建触摸设备时，`pointerType` 使用 [PT_TOUCH](./input-types-and-limits.md)，`maxCount` 不能超过 [MAX_TOUCH_COUNT](./input-types-and-limits.md)。
- 创建笔设备时，`pointerType` 使用 [PT_PEN](./input-types-and-limits.md)，`maxCount` 必须为 `1`。
- 触摸或笔的具体字段布局见 [POINTER_TOUCH_INFO](./touch.md) 和 [POINTER_PEN_INFO](./pen.md)。
