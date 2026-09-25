# Spectral Resonator

面向 Bitwig Studio 的 Rust 频谱共鸣效果器。

当前处于 M2 整数频点共鸣研究原型阶段。此前的 M0 增益插件已由用户在 Bitwig 中确认可加载、可调 Output Gain；M1 透明 STFT 在 Bitwig 以 44.1 kHz 报告约 92.9 ms 延迟，与 4096 样本一致。0.3.0 构建增加可听见的共鸣湿声，源码检查、release 打包和 CLAP 验证器已通过，M2 本身仍待 Bitwig 试听。

## 从哪里开始

- [项目大纲](docs/PROJECT_OUTLINE.md)：了解产品目标、首版范围、技术选择和里程碑。
- [具体实现设计](docs/IMPLEMENTATION.md)：了解工程结构、STFT、共鸣状态、精确音高重建、实时约束和测试方法。
- [M1 验证记录](docs/validation/M1_2026-09-26.md)：查看构建、离线重建和 CLAP 验证结果及其边界。
- [M2 验证记录](docs/validation/M2_2026-09-26.md)：查看整数频点共鸣的源码检查和 CLAP 验证结果。

已确定采用 Rust，优先交付 Windows x86-64 CLAP 插件并在 Bitwig 中验证。DSP 使用 CPU；界面先保持简单，后续使用 egui 与 wgpu 实现 GPU 可视化。VST3 是后续兼容目标。

M0 的 Bitwig 加载与增益调节已有用户反馈，工程保存恢复和多实例仍待宿主验证。M1 的离线透明重建、左右声道独立性和 4096 样本延迟已通过源码测试；用户在 Bitwig 看到约 92.9 ms 延迟，符合 44.1 kHz 下的预期。M2 增加逐声道整数频点湿声和临时研究控制，精确音高与三频段 crossover 仍属于后续工作。

## 文档状态

| 项目 | 状态 |
| --- | --- |
| 更新日期 | 2026-09-26 |
| 项目目录 | `D:\Programming\Project\spectral-resonator` |
| Rust / Cargo | 本机已核实为 1.92.0 |
| Workspace 与 Cargo.lock | 已建立；nice-plug 固定为 0.4.2，RustFFT 固定为 6.4.1 |
| CLAP 插件源码 | 0.3.0；1→1、2→2；固定 N=4096、H=512；M2 整数频点共鸣；Output Gain 保持原参数 ID |
| 本机验证 | 格式、5 项源码测试、Clippy、release 构建通过；CLAP 验证器 33 通过、0 失败、11 跳过 |
| Bitwig 加载与 Output Gain | 用户确认此前的 M0 构建可加载、可调音量；Bitwig 版本和测试配置尚未记录 |
| M1 的 Bitwig 延迟 | 用户看到约 92.9 ms；与 44.1 kHz 下的 4096 样本一致；立体声与 CPU 尚未记录 |
| M2 的 Bitwig 试听、立体声、CPU | 尚未验证 |

当前插件用 `Root Note`、`Decay T60` 和临时的 `M2 Wet Level` 控制整数 FFT 频点共鸣，并把湿声加到对齐后的干声。将 `M2 Wet Level` 设为 0 可只听干声。该参数属于研究原型，不代表 M3 最终混合设计；M3 计划用两个分频点形成低／中／高三频段路由。默认报告的延迟仍为 4096 样本，在 48 kHz 下约为 85.33 ms。

## 本机编译与打包

在 Windows x86-64 上，`scripts/bundle-clap.ps1` 生成单个可加载文件 `target/bundled/my_spectral_resonator.clap`。本机 Cargo 全局配置的 USTC 镜像当前返回 404；构建时通过临时 Cargo home 使用官方 crates.io：

```powershell
$env:CARGO_HOME = Join-Path $env:TEMP 'spectral-resonator-cargo'
./scripts/bundle-clap.ps1
```

## CLAP 验证

验证器当前 Git 版本要求 Rust 1.95；插件项目仍使用 `rust-toolchain.toml` 固定的 Rust 1.92。通常可单独安装验证器，然后验证实际打包文件：

```powershell
rustup toolchain install 1.95.0 --profile minimal
cargo +1.95.0 install --git https://github.com/free-audio/clap-validator.git --locked
clap-validator validate target/bundled/my_spectral_resonator.clap
```

本机执行 `cargo install --git` 时仍受到 USTC 镜像 404 影响，因此改从同一 Git 源码构建验证器，放在 `target/tools/bin/clap-validator.exe`。`./scripts/validate-clap.ps1` 会优先使用该本地程序，也支持 PATH 上的 `clap-validator`。Windows 的 `.clap` 路径必须指向插件文件；旧的目录式 `target/bundled/Spectral Resonator.clap` 是 M0 遗留产物。

在 Bitwig 中验收新构建时，分别输入仅左、仅右及左右反相信号，检查声道位置与电平，并用脉冲核对 4096 样本延迟。旧 M0 插件仍在宿主中使用时，避免同时扫描两个具有相同 CLAP ID 的包。
