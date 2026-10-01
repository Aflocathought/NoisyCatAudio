# Noisy Cat Audio 文档索引

当前产品为 **猫振器 / Specatral Resonator**，采用 Rust 实现的 CLAP 频谱共鸣效果器。项目说明、功能和基本用法从 [根目录 README](../README.md) 开始。未来 Specatral Delay 目前只是系列计划，尚未实现。

## 使用、设计与共享组件

| 内容 | 入口 |
| --- | --- |
| 功能、用途、参数、路由与内测风险 | [产品说明](../README.md) |
| 产品范围与开发阶段 | [项目大纲](PROJECT_OUTLINE.md) |
| 模块、DSP、线程与生命周期 | [实现设计](IMPLEMENTATION.md) |
| 频谱共鸣算法推导 | [DSP 推导](DSP_Derivation.md) |
| 跨插件共享偏好及接入协议 | [audio-plugin-settings](../crates/audio-plugin-settings/README.md) |
| 图标源码、预览与导出 | [图标说明](../assets/icon/README.md) |
| Windows 安装、更新、卸载与构建 | [安装器说明](../installer/windows/README.md) |
| macOS 云端编译、测试与实验包 | [Mac 构建说明](MACOS_BUILD.md) |

## 授权、名称与贡献

- 自有代码、文档及图标生成工具：[GPL-3.0-only](../LICENSE)，适用范围见 [版权声明](../COPYRIGHT.md)。第三方内容保留原许可。
- 名称非独占共享及指定静态标志的 CC0 授权：[名称与标志声明](NAMING_AND_LICENSE.md)。项目称呼不表示成立公司或取得注册商标。
- 来源记录、中立表述、AI 参与说明及验证边界：[贡献说明](../CONTRIBUTING.md)。
- 使用插件不要求将自己的音频作品开源，具体边界见 [作品权属说明](../README.md#许可证与作品权属)。

## 当前版本与验证记录

| 记录 | 范围 |
| --- | --- |
| [0.15.0 全局偏好](validation/GLOBAL_PREFERENCES_2026-09-29.md) | 配置文件、跨进程协作、工程状态兼容、界面验证 |
| [0.14.1 频谱图层](validation/SPECTRUM_LAYERS_2026-09-29.md) | DRY/WET 显隐、冻结历史恢复及显示状态保存 |
| [Windows 安装生命周期](validation/WINDOWS_INSTALLER_2026-09-29.md) | 隔离安装、升级、修复、卸载和占用检测 |
| [输出保护检查](validation/OUTPUT_SAFETY_2026-10-01.md) | 当前没有最终峰值限幅；离线峰值及检查边界 |
| [发布许可审计](validation/RELEASE_LICENSE_AUDIT_2026-09-30.md) | 依赖、字体、运行库及发布包声明的未完成项 |
| [本轮源码整理与验证](validation/REPOSITORY_REVIEW_2026-10-02.md) | 命名、仓库链接、分批提交及本轮检查结果 |
| [macOS 云端编译与测试](validation/MACOS_CI_2026-10-02.md) | ARM / Intel 原生构建、单元测试、CLAP 校验及实验包 |

更早的算法、性能和界面记录位于 [validation](validation/)，按文件名中的日期及文内版本阅读。历史记录中的测试数、制品哈希和性能数据只对应当时版本；中性的“宿主”表述不表示所有音频宿主均已验证。

当前源码版本为 0.15.0 开发版。本轮整理不等于发布新二进制；`target/` 中的本机构建、截图、日志和审计归档不纳入 Git，也不保证其他克隆中存在。源代码检查、模拟宿主、安装器和真实音频工程验收分别记录，不互相替代。
