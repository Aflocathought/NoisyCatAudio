# 0.15.0 全局偏好

2026-10-02 更新：当前显示品牌为 Noisy Cat Audio，产品名为 Specatral Resonator；共享命名空间保持兼容。仓库地址已经配置，设置按钮现可在用户确认网址后打开项目仓库。以下测试和制品哈希仍对应 2026-09-29 的历史构建，不代表改名后的二进制已经重新验证。

共享命名空间为 `com.aflocat.audio`，Windows 路径为 `%APPDATA%/com.aflocat.audio/settings.json`。真实用户文件已用同一存储模块初始化，默认 `maximum_ui_fps=60`、`language=system`；已有字段不会被初始化覆盖。其它平台路径、schema、接入方式和错误边界见 [模块说明](../../crates/audio-plugin-settings/README.md)。

## 使用与兼容性

- Settings 中的最大 UI 帧率对同品牌、接入此模块的插件全局生效。旧工程中的 `ui_max_fps` 不再覆盖用户偏好，也不再写回工程。FFT、DRY/WET 图层显示、FPS 调试叠加等仍随实例/工程保存。
- 语言选择保存 `system` / `en` / `zh-CN`。按用户要求，本版只保存共享偏好，暂不翻译界面。
- 当前品牌字段为 Noisy Cat Audio，CLAP 插件 ID 仍为 `org.spectral-resonator.dev`，避免已有工程找不到插件。
- 原验证时仓库按钮因缺少 URL 而禁用；2026-10-02 已通过 Cargo `repository` 配置正式 HTTPS 地址。点击仍需明确确认目标地址才发送打开浏览器指令。

## 存储与线程

新增独立 crate `audio-plugin-settings`，可被其它插件复用。写入在 `settings.lock` 系统排他锁内重新读取最新文件，只合并变动键，保留未知字段与其它插件的数据；同目录临时文件 flush/sync 后 rename 替换。不同键可并发保留，同键最后成功写入生效。格式损坏、不支持的 schema 或过大文件均拒绝覆盖。

同 DLL 同路径共享一个后台线程；其它进程的更改每 500 ms 读取一次。UI 每帧仅访问内存，音频回调不打开文件、加配置锁或启动线程。最后一个客户端释放时 join 线程，退出前对未保存修改做有限重试。权限故障或持续占锁不能保证保存，错误在界面显示，已有文件不被破坏。

## 验证

- 共享存储 4 项测试通过，1 项是由父测试启动的独立进程辅助入口。4 个进程共提交 120 批更新，保留所有插件键与最终共享值；并发读取未遇到坏 JSON。持锁进程被强制终止后，系统锁释放，客户端待保存更改成功写入。
- 客户端复用、快速连续修改、外部修改同步、关闭前保存、重新打开加载、未知字段保留、损坏/未来版本文件不覆盖均通过。最终日志：`target/global-settings-store.log`；测试数据在 `target/settings-tests/`，不会触碰真实用户配置。
- 插件原有 45 项测试通过；新增仓库确认测试最初因测试未消费 egui 纹理增量失败，修正测试清理后单独复测通过。该测试确认未确认不打开、确认才发送准确 URL、Escape 取消不打开。日志：`target/global-settings-tests.log`、`target/global-settings-confirmation.log`。
- GPU 预览通过（RTX 4060 Laptop / Vulkan），设置布局已检查；图像 `target/global-settings-preview/settings.png`，日志 `target/global-settings-ui.log`。
- 两个相关 crate 的全部目标 Clippy `-D warnings`、格式与 diff 空白检查通过。Release 仅有 nice-plug 原有的两个 unused-variable 警告。
- 最终制品 CLAP validator：36 passed / 0 failed / 0 warnings / 8 skipped，日志 `target/global-settings-validator.log`。
- 最终制品的模拟宿主完成 3 轮窗口创建/调整尺寸/销毁，音频线程处理 2194 块。数值输入到 300 Hz、空格键转发通过。故意在旧工程写入 30 FPS、独立全局配置写入 120 FPS，三轮导出的实际上限均为 120；本轮测得约 117.8–118.7 FPS，不能等同于音频宿主性能保证。日志 `target/global-settings-native.log` 和 `target/global-settings-preview/native-profile.jsonl`。该脚本使用临时配置目录，真实用户配置仍为 60 FPS。

本轮未在真实音频宿主工程内实测；Windows 多进程与文件替换已经实测，macOS / Linux 仅实现了路径分支，没有平台运行证据。

## 制品

`target/artifacts/0.15.0/my_spectral_resonator.clap`，13,941,248 字节。

SHA256：`5D7C77F40E1BAF0FB6703CFD4B7C51A4696963A0C53D106FEA96C1178537C260`。

常用路径为 `target/bundled/my_spectral_resonator.clap`；发布和 0.14.1 备份信息见 `target/global-settings-preview/publication.json`。需重新加载已有实例。
