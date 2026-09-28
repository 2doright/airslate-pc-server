## 编程原则
- Zero-up refactor, breaking changes. NEVER write defensive, fallback, or patch code. Disregard backward compatibility and unreasonable legacy structures.
- Runtime truth, no fake compile passes. NEVER optimize for build-green status by adding patches, fake flags, placeholder transitions, or exception wrappers.
- Event-sourced facts, no simulation. Store state must describe facts, not hopes.
- Root-cause fixing, no symptom-patching

## 技术栈
- Rust + Tauri v2，目标平台 Windows、macOS、Linux（Linux 覆盖 GNOME/KDE，X11 与 Wayland）。Windows crate 仅服务于保留的 Windows 后端，不得新增 Windows 专属依赖
- 前端目录: `frontend/`
- 平台后端按 `src/native_input/<platform>/` 与 `src/workspace/<platform>` 组织，新增平台按同构结构扩展；笔输与快捷键注入按平台实现：Windows=Pointer/Pen API、macOS=CoreGraphics 事件、Linux=uinput（`/dev/uinput`）：笔=绝对坐标+压感+倾斜+笔按键，快捷键=EV_KEY/EV_REL 合成
- USB 传输使用 nusb（跨平台）；Windows 的 WinUSB 辅助进程仅 Windows 后端保留

## Rust Skills
本项目通过 `.rust-skills` Git submodule 使用
[`actionbook/rust-skills`](https://github.com/actionbook/rust-skills)。所有 Rust 问题、实现和代码审查任务都必须使用该 Skill路由问题，遵循相关 Skill 的实现、检查和测试要求。

## 实现与验证
代码编写完成后更新`CHANGELOG.md`
随后通过PR进行远程CI验证(除非用户表示要本地进行)
### 本地CI调试
- 修改项目工程代码后必须执行真实语法检查、测试
- 仅当用户明确表示本地build时，才进行build
- 涉及 uinput 的改动必须真机验证：先启动本程序创建虚拟笔设备，**再打开**绘图应用（Krita 不会动态发现后出现的设备），上报压感并确认绘图应用收到压力
- 涉及有线 USB 的改动必须插华为平板真机验证：确认 udev 放行后能进入 Bulk 会话，无平板时不得声称验证通过

## 命令
前端检查:npm --prefix ./frontend run build
编译:cargo tauri build
语法检查:cargo check
测试:cargo test
### Linux 构建依赖（pkg-config 需可见）
- webkit2gtk-4.1、libsoup-3、gtk3、librsvg、libayatana-appindicator（托盘，bundler 强制检测，仅有运行时库不够）
- Fedora: `sudo dnf install webkit2gtk4.1-devel libsoup3-devel gtk3-devel librsvg2-devel libayatana-appindicator-gtk3-devel`
- 打包 AppImage 另需 libfuse2（linuxdeploy 工具本身是 AppImage）：Fedora 装 `fuse-libs`，Ubuntu 24.04 装 `libfuse2t64`；无 FUSE 环境无法产出 AppImage，deb/rpm 不受影响
### Linux 运行权限
- 注入输入需要 `/dev/uinput` 访问权；deb/rpm 必须安装 `assets/linux/70-airslate.rules`，通过 `TAG+="uaccess"` 将虚拟设备授权给当前 active local desktop session，不得要求用户永久加入 `input` 组
- uinput 规则必须保留 `OPTIONS+="static_node=uinput"`：systemd-tmpfiles 会在启动阶段处理该静态节点，访问节点时可触发内核按 `char-major-10-223` 自动加载 uinput；安装脚本仍需 `modprobe uinput` 让首次安装立即生效
- 有线模式需要 USB 设备节点访问权：同一规则只匹配 `DEVTYPE=usb_device` 的华为 HDC（`idVendor=12d1`）与 AOA 配件（`idVendor=18d1`），使用 `TAG+="uaccess"` 授权当前桌面会话；否则 nusb 打开设备报 `permission denied (errno 13)`，扫描循环无法进入 Bulk 会话
- ModemManager 会探测华为厂商接口并触发 xhci 周期 reset，须在同一规则中标记 `ENV{ID_MM_DEVICE_IGNORE}="1"`；否则平板每数秒重枚举，无法稳定连接
- uaccess 标签规则文件名必须按词法顺序位于 systemd 的 `73-seat-late.rules` 之前；当前使用 `70-airslate.rules`。AppImage 无法安装系统规则，README 必须保留一次性配置步骤
### Linux 启动
- NVIDIA 专有驱动 + Wayland 下 webview 创建即崩（`Gdk-Message: Error 71`），程序在 `main.rs` 启动时自动探测（Wayland 会话且 `/proc/driver/nvidia/version` 存在）并设置 `WEBKIT_DISABLE_DMABUF_RENDERER=1`；Intel/AMD、X11 会话不受影响
- 关窗口仅最小化到托盘（笔输注入不中断）；重开 UI 点托盘图标左键或菜单"打开界面"，彻底退出用菜单"退出程序"

## 发布
- release：`major.minor.patch`tag触发
- prelease：tag`major.minor.patch-beta.N`tag触发
- 发布时同步维护版本号。
- 打包目标按平台声明：Windows MSI 由基础 `tauri.conf.json` 定义（beta.N 映射为 major.minor.patch.N，正式版无后缀）；Linux 的 deb/rpm/AppImage 由 `tauri.linux.conf.json` 覆盖 `bundle.targets`，不得改动基础配置中其他平台的打包定义
- 发布文本：文本结构参考历史release。其中的重要更新部分是主要不同的地方，禁止从开发者角度描述，需要从用户的角度描述，禁止描述开发细节；各点内容不允许耦合，彼此应当毫不相关，文本精简凝练短小清晰。

## 相关文档
- Linux 笔输注入：Kernel `uinput` 文档（Documentation/input/uinput.rst）、libinput tablet support
- Windows API 笔输相关文档介绍：`docs/winapi/index.md`（仅 Windows 后端参考）
- Rust for Windows API：<https://microsoft.github.io/windows-docs-rs/doc/windows/>（仅 Windows 后端参考）
