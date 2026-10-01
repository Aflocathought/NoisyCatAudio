# 0.8.2：模拟 Bounce 的双线程测试与 CLAP 有限尾音修复

用户反馈 0.8.1 仍无法完成音频宿主 Bounce，要求在没有其他宿主的情况下模拟类似录制/导出的操作。本轮没有操作用户工程或终止音频宿主；扩展了实际 CLAP ABI 测试宿主，并交付 0.8.2。

## 观察、复现与结论范围

最新音频宿主 6.1 日志中，02:16:07 的 Bounce 完成了进入离线、渲染、恢复实时的完整流程。随后加载 Spectral Resonator 后，02:16:34 和 02:17:12 的操作均停在 `setRealtime(false)` / `askEngineToSkipProcessing(true)`，直到切换工程断开引擎才推进清理流程。日志本身没有显示当时加载实例的版本或具体线程栈；用户反馈上一候选修复无效，不能将它继续描述为已解决。

双线程模拟中，显式调用 stop_processing、状态读写、重置和激活都能返回，没有复现这些调用的互锁。另一个可重复的问题是：输入和输出都已经为精确零时，0.8.1 每个处理块仍返回 `CLAP_PROCESS_CONTINUE`（1），不允许依照有限尾音结束处理；插件虽然返回 `tail.get() = 10848`，但未调用宿主的 tail.changed 通知。

按 CLAP 协议，有限尾音应使用 `CLAP_PROCESS_TAIL`（3），表示宿主可以按 tail.get 决定何时停止；`CONTINUE` 则要求继续。这个差异使本次模拟宿主的“等待静音尾音结束再暂停”阶段在旧版失败，新版通过。它是明确复现的协议映射问题，仍不是对音频宿主内部等待条件的反向证明；本轮未获得修复后音频宿主 Bounce 的成功记录。

官方定义：

- [process.h](https://github.com/free-audio/clap/blob/main/include/clap/process.h)
- [tail.h](https://github.com/free-audio/clap/blob/main/include/clap/ext/tail.h)

## 0.8.2 修改

仅修改 vendored nice-plug 的 CLAP 包装：

1. 将 `ProcessStatus::Tail(_)` 映射到 `CLAP_PROCESS_TAIL`。KeepAlive 仍映射到 CONTINUE，Normal 保持 CONTINUE_IF_NOT_QUIET。
2. 查询可选的宿主 `clap.tail` 接口，在报告长度发生变化时，从音频线程调用 `changed()`。先发布新值、释放插件锁，再通知，允许宿主在通知中立即回查 tail.get。

保留 0.8.1 的模式切换 opt-out，不再把它视为足够的 Bounce 修复。DSP、尾音秒数计算、音频缓冲、音高、调制、参数和延迟均未改变。框架来源与完整本地差异见 [PATCHES.md](../../vendor/nice-plug/PATCHES.md)。

## 模拟宿主及覆盖

[probe-clap-bounce.py](../../scripts/probe-clap-bounce.py) 使用实际 CLAP 动态库，主线程与单独音频线程分工，通过 `clap.thread-check` 告知插件线程身份。主线程在音频运行时反复保存状态、查询尾音/延迟并处理 request_callback；生命周期变更按 CLAP 顺序执行，不用非法 deactivate/process 并发制造故障。

每个配置重复三轮：启动播放和录制标志、发送音符/transport 事件、停止输入、按 CLAP 返回状态等待有限尾音、显式停止音频线程、保存/加载状态、切换离线、改变采样率与块容量、重新激活、离线渲染、停止/重置、恢复实时。测试同时提供 latency/params/tail 宿主接口，并实际处理初次延迟声明产生的重启请求。

- 四个配置：Internal / MIDI × Spectral / Post。MIDI 使用 16 个独立身份音符。
- Unison=8、Harmonics=256、Granular；全局和六个节点衰减均设为 0.05 s，以缩短尾音协议测试时间。该 0.05 s 是测试参数，没有改变插件默认值。
- 实时阶段固定 48 kHz / 256 samples；三轮离线阶段分别为 48 kHz / 1024、44.1 kHz / 512、96 kHz / 2048。
- 包含播放/录制标志、非零 sample offset 的 transport 事件、左侧激励和右侧零输入、分段状态流读写。
- 每次离线渲染一秒，共 12 个立体声 WAV。保存前对原始 f32 音频计算 SHA-256；WAV 为供检查的 16-bit PCM，不作为浮点一致性的证据。
- 独立父进程提供每个测试子进程 45 秒硬超时。触发时只结束自身子进程并保留最后完成的 ABI 阶段，不影响音频宿主。

## 对照结果

| 检查 | 0.8.1 | 0.8.2 |
| --- | --- | --- |
| 静音后的 process 状态 | 持续为 1 / CONTINUE | 3 / TAIL |
| 尾音变更通知 | 四个配置均为 0 次 | 每配置 6 次，均来自音频线程 |
| 12 次静音等待 | 3 秒模拟音频内均未获得可结束条件 | 均在约 0.245 s 后可结束 |
| stop / state / reactivate 调用 | 能返回 | 能返回 |
| 四个配置的尾音契约检查 | 全部失败 | 全部通过 |
| 12 次离线原始 f32 音频 | 与新版相同 | 与旧版逐份哈希一致 |

0.245 s 是本组短衰减参数下模拟的音频时长，不是 wall-clock 性能指标，也不是任意预设的尾音长度。该配置报告 10,848 samples（48 kHz 下 0.226 s）；块量化和测试中的 transport 事件使等待略长。旧版不是测试线程卡死：测试执行完毕，但宿主根据 CONTINUE 无法结束静音等待，断言失败。二者必须区分。

证据保存在 `target/bounce-0.8.2/`：

- `old-probe/*.jsonl`、`probe/*.jsonl`：逐阶段记录、宿主通知和状态码。
- `old-probe/results.json`、`probe/results.json`：四配置失败/通过结果。
- `audio-comparison.json`：12 份跨版本原始音频哈希对照。
- `probe/*.wav`：12 份实际离线录制文件。
- `render-regression.txt`：原串行模式切换回归，两种算法与旧版音频哈希相同。

58 项源码测试通过（DSP 36、插件 22），workspace fmt / 全目标全特性 Clippy `-D warnings` 通过；vendored 上游的既有 unused/dead-code 警告仍可见。CLAP 版本副本和更新后的稳定路径验证均为 36 passed、0 failed、0 warnings、8 skipped。真实宿主内的最终确认仍待完成。

复现（使用本机可用 Python）：

```powershell
python scripts/probe-clap-bounce.py target/artifacts/0.8.1/my_spectral_resonator.clap --output target/old-bounce --require-tail-drain
# 上一行预期失败，下一行应通过。
python scripts/probe-clap-bounce.py target/artifacts/0.8.2/my_spectral_resonator.clap --output target/new-bounce --require-tail-drain
python scripts/check-clap-render.py target/artifacts/0.8.2/my_spectral_resonator.clap --require-no-restart
```

## 制品

Windows x86-64 CLAP 0.8.2，1,720,320 字节：

```text
target/artifacts/0.8.2/my_spectral_resonator.clap
target/bundled/my_spectral_resonator.clap
SHA-256: 2CD2DE4CE414C030FD3804824071DB23F6D72C3F16C6C43586CB8E1B41403FAF
```

稳定路径已原子替换并重新验证，旧 0.8.1 备份在 `target/bundled/4a755042e06a485a9dce4f43f2607078.previous`，旧哈希 `224C3667F86658F3664C0954A82FC12BC931088C479811332D90967B6E74387E`；发布信息记录于 `target/bounce-0.8.2/publication.json`。

更新文件不会自动替换音频宿主已加载的旧实例；使用时需重新加载并确认 0.8.2。若该版本仍停在相同阶段，应保留挂起现场以读取等待链，而不是继续根据同一条日志猜测更多 DSP 改动。
