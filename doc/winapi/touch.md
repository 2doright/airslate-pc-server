# 触摸输入结构

## POINTER_TOUCH_INFO

`POINTER_TOUCH_INFO` 定义触摸输入专属字段。

### 语法

```cpp
typedef struct tagPOINTER_TOUCH_INFO {
  POINTER_INFO pointerInfo;
  TOUCH_FLAGS  touchFlags;
  TOUCH_MASK   touchMask;
  RECT         rcContact;
  RECT         rcContactRaw;
  UINT32       orientation;
  UINT32       pressure;
} POINTER_TOUCH_INFO;
```

### 成员

- `pointerInfo`
  - 公共指针头，定义见 [POINTER_INFO](./common-pointer-types.md)。
- `touchFlags`
  - 触摸标志，取值见 [TOUCH_FLAGS](./touch.md)。
- `touchMask`
  - 触摸掩码，决定哪些可选字段有效，取值见 [TOUCH_MASK](./touch.md)。
- `rcContact`
  - 预测后的接触区域屏幕坐标，单位为像素。
- `rcContactRaw`
  - 原始接触区域屏幕坐标。
- `orientation`
  - 指针方向，范围 `0` 到 `359`。
- `pressure`
  - 压力值，范围 `0` 到 `1024`，默认值为 `512`。

### 补充说明

- 如果设备不报告接触区域，`rcContact` 默认是以指针位置为中心的 `0 x 0` 矩形。
- 某些设备只报告半程方向值 `0` 到 `180`，另一些设备报告全范围 `0` 到 `359`。

## TOUCH_FLAGS

| 常量 | 值 | 说明 |
| --- | --- | --- |
| `TOUCH_FLAG_NONE` | `0x00000000` | 默认值。 |

## TOUCH_MASK

`TOUCH_MASK` 表示哪些可选字段包含有效值。

| 常量 | 值 | 说明 |
| --- | --- | --- |
| `TOUCH_MASK_NONE` | `0x00000000` | 默认值，表示没有可选字段有效。 |
| `TOUCH_MASK_CONTACTAREA` | `0x00000001` | `rcContact` 有效。 |
| `TOUCH_MASK_ORIENTATION` | `0x00000002` | `orientation` 有效。 |
| `TOUCH_MASK_PRESSURE` | `0x00000004` | `pressure` 有效。 |

## 实现提示

- 触摸 contact 的公共状态位位于 [POINTER_FLAGS](./common-pointer-types.md)。
- 只有在对应字段实际提供值时，才应在 [TOUCH_MASK](./touch.md) 中标记该字段有效。
- 触摸设备和注入数量限制见 [输入类型与限制](./input-types-and-limits.md)。
