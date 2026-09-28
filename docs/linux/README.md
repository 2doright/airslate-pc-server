# Linux

## 构建依赖（pkg-config 需可见）

- webkit2gtk-4.1、libsoup-3、gtk3、librsvg、libayatana-appindicator（托盘，bundler 强制检测，仅有运行时库不够）
- Fedora: `sudo dnf install webkit2gtk4.1-devel libsoup3-devel gtk3-devel librsvg2-devel libayatana-appindicator-gtk3-devel`
- 打包 AppImage 另需 libfuse2（linuxdeploy 工具本身是 AppImage）：Fedora 装 `fuse-libs`，Ubuntu 24.04 装 `libfuse2t64`；无 FUSE 环境无法产出 AppImage，deb/rpm 不受影响

## 运行权限

- 注入输入需要 `/dev/uinput` 访问权；deb/rpm 必须安装 `assets/linux/70-airslate.rules`，通过 `TAG+="uaccess"` 将虚拟设备授权给当前 active local desktop session，不得要求用户永久加入 `input` 组
- uinput 规则必须保留 `OPTIONS+="static_node=uinput"`：systemd-tmpfiles 会在启动阶段处理该静态节点，访问节点时可触发内核按 `char-major-10-223` 自动加载 uinput；安装脚本仍需 `modprobe uinput` 让首次安装立即生效
- 有线模式需要 USB 设备节点访问权：同一规则只匹配 `DEVTYPE=usb_device` 的华为 HDC（`idVendor=12d1`）与 AOA 配件（`idVendor=18d1`），使用 `TAG+="uaccess"` 授权当前桌面会话；否则 nusb 打开设备报 `permission denied (errno 13)`，扫描循环无法进入 Bulk 会话
- ModemManager 会探测华为厂商接口并触发 xhci 周期 reset，须在同一规则中标记 `ENV{ID_MM_DEVICE_IGNORE}="1"`；否则平板每数秒重枚举，无法稳定连接
- uaccess 标签规则文件名必须按词法顺序位于 systemd 的 `73-seat-late.rules` 之前；当前使用 `70-airslate.rules`。AppImage 无法安装系统规则，需保留一次性配置步骤

## 启动

- NVIDIA 专有驱动 + Wayland 下 webview 创建即崩（`Gdk-Message: Error 71`），程序在 `main.rs` 启动时自动探测（Wayland 会话且 `/proc/driver/nvidia/version` 存在）并设置 `WEBKIT_DISABLE_DMABUF_RENDERER=1`；Intel/AMD、X11 会话不受影响
- 关窗口仅最小化到托盘（笔输注入不中断）；重开 UI 点托盘图标左键或菜单“打开界面”，彻底退出用菜单“退出程序”
