## 编程原则

- Zero-up refactor, breaking changes. NEVER write defensive, fallback, or patch code. Disregard backward compatibility and unreasonable legacy structures.
- Runtime truth, no fake compile passes. NEVER optimize for build-green status by adding patches, fake flags, placeholder transitions, or exception wrappers.
- Event-sourced facts, no simulation. Store state must describe facts, not hopes.
- Root-cause fixing, no symptom-patching

## 技术栈

- Rust + Windows crate
- 前端位于 `frontend/`
- 目标平台为 Windows、MacOS

## Rust Skills

本项目通过 `.rust-skills` Git submodule 使用
[`actionbook/rust-skills`](https://github.com/actionbook/rust-skills)。所有 Rust 问题、设计、实现、
调试和代码审查任务都必须使用该 Skill 系统。

### Rust skill 流程

1. 完整阅读 `.rust-skills/AGENTS.md`
2. 使用 `.rust-skills/skills/rust-router/SKILL.md` 路由问题
3. 根据路由结果任务类型，继续完整阅读对应的 `.rust-skills/skills/*/SKILL.md`
4. 遵循相关 Skill 的实现、检查和测试要求


## 实现与验证

### 调试

- 修改项目工程代码后必须执行与风险相称的真实语法检查、测试
- 测试必须验证运行时事实，不得使用占位实现、伪造状态或仅为通过编译而添加的开关。

### 构建
- 仅当用户明确表示构建时，才进行构建。

### 发布

### 前端检查

npm --prefix ./frontend run build

### 打包Release

cargo tauri build

### 版本

- 调试通过后更新 `CHANGELOG.md`。
- 修改发布行为时同步维护版本号。

### 发布
- 通过tag触发workflow远程编译release
- prelease通过tag中-识别

## 项目文档

- Windows API 笔输相关文档介绍（不是 Rust crate 规范）：`doc/winapi/index.md`
- Rust for Windows API：<https://microsoft.github.io/windows-docs-rs/doc/windows/>
