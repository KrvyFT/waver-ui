# waver-ui

egui 三栏 patch 工作区：模块库、画布、检查器。只依赖 [`waver-core`](https://github.com/KrvyFT/waver-core)，**不**依赖 `waver-dsp`。

本仓库是 [waver](https://github.com/KrvyFT/waver) workspace 的一部分，在伞仓中位于 `crates/waver-ui`（git submodule）。

## 仓库

- GitHub：https://github.com/KrvyFT/waver-ui
- 默认分支：`main`
- License：MIT OR Apache-2.0
- 依赖：`waver-core`、`eframe`、`rtrb`

## 在伞仓里开发（推荐）

```bash
git clone --recurse-submodules https://github.com/KrvyFT/waver.git
cd waver/crates/waver-ui
git add -A && git commit -m "…" && git push
cd ../..
./scripts/repos.sh sync
git commit -m "chore: bump waver-ui" && git push
```

在伞仓根目录：`cargo test -p waver-ui` / `cargo run -p waver`。

## 单独 clone

```bash
git clone https://github.com/KrvyFT/waver-ui.git
```

可独立推送；构建请走伞仓。见 [doc/repos.md](https://github.com/KrvyFT/waver/blob/master/doc/repos.md)。

## 内容概要

| 项 | 说明 |
|----|------|
| `WaverApp` | 三栏布局与模块库（读 `MODULE_CATALOG`） |
| `PatchState` | `Graph`、布局、`recompile` → `SwapSchedule` |
| `editor/` | 节点、线缆、旋钮、VCO 专用控件 |
| `setup_fonts` / `setup_theme` | 启动时由二进制调用 |

UI 约定见 [doc/ui.md](https://github.com/KrvyFT/waver/blob/master/doc/ui.md)。
