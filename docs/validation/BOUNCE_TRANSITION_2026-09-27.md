# 0.8.1：Bitwig Bounce 模式切换兼容修复候选

后续结论：用户反馈 0.8.1 仍无法完成 Bounce。取消模式切换重启请求不足以解决该问题；后续双线程模拟发现并修正有限尾音被映射为无限继续处理的协议问题，见 [0.8.2 记录](BOUNCE_TAIL_2026-09-27.md)。以下保留 0.8.1 当时的调查与候选验证记录。

日期：2026-09-27。用户反馈 Bitwig 的 Bounce / 弹跳 / 导出无法开始，界面仍能操作；不是普通实时录音，也未确认是插件崩溃。当前已交付 0.8.1 候选修复，尚待 Bitwig 中重新执行 Bounce 确认。

## 观察与证据边界

本机 Bitwig 日志报告版本 6.1。两次操作都停在切换离线模式的阶段：

```text
01:47:59.960 FloatDocument.setRealtime(false), wasRealtime: true
01:47:59.960 askEngineToSkipProcessing(true)
01:47:59.972 FloatDocument.isEngineReadyChanged(true->false)

01:50:34.783 FloatDocument.setRealtime(false), wasRealtime: true
01:50:34.783 askEngineToSkipProcessing(true)
```

后一次直到 01:52:55.721 才在引擎断开后的清理流程中出现 `BounceTask.doStartAfterRealtime()`；这不是正常完成 Bounce 的证据。此前退出阶段的 engine 异常退出码不能单独证明是 Spectral Resonator 崩溃。未获取挂起线程栈，因此尚未证明 Bitwig 内部的具体等待链。

读取到的两份相关插件缓存均为 0.8.0、Spectral、Unison=1。因此不能将这个现象限定为 Post 或八路 Unison 的高负载。插件 DSP 本身没有根据录制/离线标志选择不同算法。

## 修改

nice-plug 0.4.2 的 `clap.render.set()` 在插件已激活且 realtime/offline 模式变化时直接调用宿主 `request_restart()`。我们的引擎不依赖 `BufferConfig::process_mode`，这次重启握手可以省略；在宿主已经切换离线模式时发出额外重启请求是与现象相符的兼容性疑点，尚不能据此断言它就是唯一根因。

将已锁定的 nice-plug 0.4.2 源码保存到 `vendor/nice-plug`，通过 Cargo patch 引用，没有修改 Cargo 下载缓存。只修改两个上游源码文件：增加默认 true 的 `CLAP_REACTIVATE_ON_RENDER_MODE_CHANGE`，并用它控制原有重启请求。Spectral Resonator 显式设为 false。新模式仍被记录；其他插件默认保留原行为。上游来源和差异见 [PATCHES.md](../../vendor/nice-plug/PATCHES.md)。

这次变更没有修改 DSP、音色、声部数、延迟、尾音、参数 ID 或保存格式，也没有将插件强制声明为“只能实时渲染”。采样率、块容量和声道配置变化仍通过正常激活流程处理。

## 验证

新增 [check-clap-render.py](../../scripts/check-clap-render.py)，直接加载构建出来的 CLAP 动态库，通过公开 ABI 创建实例、加载/保存状态、激活、切换模式、启动/停止处理、渲染和销毁。使用 Spectral/Post 两种算法，各为 Granular、Unison=8、Harmonics=256、内部最低音 55 Hz；48 kHz 下按 1/64/256/1024 samples 混合块长处理。每种算法分别执行 Realtime → Offline → Realtime → Offline，保存状态保持一致、左输入不串入右侧。

| 实际构建 | 四轮模式设置的重启请求次数 | 跨模式音频 | 状态保存 |
| --- | --- | --- | --- |
| 0.8.0 Spectral / Post | 0、1、1、1 | 一致 | 一致 |
| 0.8.1 Spectral / Post | 0、0、0、0 | 一致 | 一致 |

测试宿主只记录重启请求，不执行额外重启，用于检查本插件可以直接切换的契约；它不模拟 Bitwig 的内部线程调度。测试为串行 ABI 调用，不是并发宿主压力证明。0.8.0 加 `--require-no-restart` 会失败，0.8.1 通过。

每次渲染 24,210 个左声道 f32 样本；0.8.0 与 0.8.1 的同算法音频哈希完全一致：

```text
Spectral: 58df66b005b5addbf00e49868260d4bc31e92cb56109f7a116003ebd17bab051
Post:     94d14bcbdaa4c20531058d9bcd3426e8bcd27c3eaf56e8f5d12d05621cb8dd72
```

58 项源码测试通过（DSP 36、插件 22）。fmt、workspace 全目标/全特性 Clippy `-D warnings`、release 构建通过；本地化后上游源码原有的 unused/dead-code 警告可见，未为消除警告扩大修改。版本副本与更新后的稳定路径 CLAP 验证均为 36 passed、0 failed、0 warnings、8 skipped。

日志保存在 `target/bounce-0.8.1/`：`tests.txt`、`clippy.txt`、`build.txt`、`old-clap-render.txt`、`new-clap-render.txt`、`artifact-validator.txt`、`stable-validator.txt`、`publication.json`。

复现命令（Python 使用本机可用解释器）：

```powershell
python scripts/check-clap-render.py target/artifacts/0.8.0/my_spectral_resonator.clap
python scripts/check-clap-render.py target/artifacts/0.8.1/my_spectral_resonator.clap --require-no-restart
./scripts/validate-clap.ps1 -PluginPath target/artifacts/0.8.1/my_spectral_resonator.clap
```

## 制品与待确认事项

Windows x86-64 CLAP 0.8.1，1,719,296 字节：

```text
target/artifacts/0.8.1/my_spectral_resonator.clap
target/bundled/my_spectral_resonator.clap
SHA-256: 224C3667F86658F3664C0954A82FC12BC931088C479811332D90967B6E74387E
```

稳定路径已原子替换并核对哈希，旧 0.8.0 保留在 `target/bundled/6c1208685eab432d81c1af8e1b0d7853.previous`，其 SHA-256 为 `B65A9727084B90C77DC29E6101E1C50F9D65BD950F2AE6D0FBF3336F9BDC538C`。未终止 Bitwig 或修改用户工程；文件更新不会自动替换宿主已经加载的旧实例。

重新加载后确认版本 0.8.1，再对原来会挂起的片段做同样的 Bounce。需要观察是否进入渲染、是否完成，以及完成后能否恢复实时播放。若仍挂起，应继续采集当时插件宿主/音频引擎的线程等待信息；当前证据不足以宣布宿主问题已经解决。
