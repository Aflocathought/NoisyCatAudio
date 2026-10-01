# 0.14.0 Low / High Mute

Low / Mid 与 Mid / High 控件下方各增加一个 Mute 按钮，选中时显示 Muted。默认均关闭，可随工程/预设保存和自动化；分频频率保持原值。宿主参数 ID 为 `mute_low`、`mute_high`，名称为 Low Mute、High Mute；CLAP Remote Controls 新增 Routing 页。

## 信号行为

两个按钮分别关闭现有低频与高频的干声贡献。它们不关闭输入，不改变中频共鸣的激励、状态、衰减或 Post Unison；Wet only 和额外 Wet 输出保留。Middle dry/wet 小于 100% 时，同时静音外侧两段仍会保留中频干声。

继续使用现有软分频斜率，Mute 不是按某个 Hz 硬切频谱。令低分频高通权重为 `A`、现有中频权重为 `M`，预计算低频 `L=1-A`、高频 `H=A-M`。静音/淡变时的干声权重为：

`D = M × (1 − MidMix) + L × LowGain + H × HighGain`

两个 Gain 均为 1 时，保留原始 `1 − M × MidMix` 计算路径，避免改变旧工程的数值输出。两侧均静音且 MidMix=100% 时，干声权重严格为零。

系数在 hop 更新中约 20 ms 线性淡变，再经过原 Hann 重叠合成；听到的变化还受窗口时长/已有延迟影响，不是采样瞬间硬切。首次激活或 reset 后直接使用当前保存的静音目标，避免先输出一帧未静音内容。不触发宿主重启。

新增外侧权重表只在构造引擎时分配，4096 点时约 16 KiB；随分频点更新原位改写。复用原输入 FFT 和打包的干/湿 IFFT，不增加 FFT、声部或音频延迟。左右声道保持独立。隐藏按钮不会改变状态，旧工程缺失两个字段时显式恢复 false。

## 验证

- Workspace：50 DSP + 45 插件 = 95 项通过，日志 `target/band-mute-tests.log`。仅测试代码的 Clippy 循环写法修正后，3 项静音 DSP 测试再次通过，`target/band-mute-dsp-final.log`。
- 四档 FFT 的实际立体声低/中/高频多音渲染，覆盖 44.1/48/96 kHz：对应外侧频段衰减，中频和另一外侧保持；1024/96 kHz 仍按短窗固有分辨率检查，不假设砖墙分频。两侧均静音、100% wet、无共鸣时从首帧开始干声为零。
- 动态切换保留共鸣器的内部状态，Spectral/Post 下释放后的湿声尾音保留；比较湿声采用 2e-6 容差，因为干/湿共用复数 IFFT 会有舍入误差。DC 切换的相邻采样变化小于 0.005，reset 后首个合成 hop 应用保存的 Mute。
- 实际插件回调的五阶段自动化验证低/高频独立输出，多种块长逐样本一致，包含 hop 的回调未分配或释放堆内存。
- GUI 回归点击两个按钮并验证各自的 begin/set/end 手势，确认分频频率不变。旧状态迁移默认关闭、显式保存 true 保留。
- GPU 预览通过，RTX 4060 Laptop / Vulkan；默认布局和 980×700 下最小/最大分频、按钮开/关状态已目视检查。图片 `target/band-mute-preview/`，日志 `target/band-mute-ui.log`。
- Clippy 全目标/功能 `-D warnings`、格式、diff 空白检查通过。release 仍有 vendored nice-plug 原有两个 unused-variable 提示。
- 最终制品 CLAP validator：36 passed / 0 failed / 0 warnings / 8 skipped，`target/band-mute-validator.log`。
- 导出 CLAP 的四种 Mute 组合保存/加载检查通过，缺失字段恢复 false，激活不额外重启。关闭共鸣后，两侧 Mute 同开得到精确零输出。结果 `target/band-mute-preview/state-abi.json`，日志 `target/band-mute-state.log`。
- 模拟宿主真实窗口显示/隐藏/销毁、数值输入到 300 Hz、9 条预期空格消息传回宿主通过，`target/band-mute-keyboard.log`。
- 默认关闭 Mute 的 12 组模拟 Bounce 通过：Internal/MIDI × Spectral/Post × 48/44.1/96 kHz，包含尾音排空。浮点音频和 PCM WAV 的 SHA256 均与 0.13.0 的相同配置一致，结果 `target/band-mute-bounce/`，对比 `target/band-mute-preview/legacy-comparison.json`。

本次未操作真实音频宿主。源码、GPU 和模拟宿主证据不代替音频宿主的实际工程、听感或 Bounce 验收。

## 制品

`target/artifacts/0.14.0/my_spectral_resonator.clap`，13,776,384 字节。

SHA256：`85EC2DEBDE289563A11EDAA554F5DA7BD50E8DB1F246713E47D4DC189F42B163`。

稳定路径 `target/bundled/my_spectral_resonator.clap`，发布和旧版备份信息见 `target/band-mute-preview/publication.json`。需重新加载已有实例才能载入新版本。
