# Windows 安装包

当前产品显示名为 **Specatral Resonator**，发布者为 **Noisy Cat Audio**。为兼容已有安装，AppId、支持目录和安装包文件名保留历史标识。下述安装流程仍有第三方许可交付及运行库条款流程待完善，详见 [发布许可审计](../../docs/validation/RELEASE_LICENSE_AUDIT_2026-09-30.md)；本轮源码整理没有重新发布安装包。

使用 Inno Setup 6.7.3+ 将已经构建的 Windows x86-64 CLAP 打包为单个 `.exe`。安装后无需 Rust、Python、.NET SDK 或 Inno Setup。当前插件版本仍为 0.15.0，安装器没有修改音频算法或 CLAP ID。

包内包含微软签名的 Visual C++ x64 可再发行组件 14.44.35211.0，因此安装过程无需联网下载。只有系统缺少兼容的 `VCRUNTIME140.dll` 时才启动微软安装程序，并可能请求管理员授权；取消／失败会阻止插件文件写入。若微软组件要求重启，需重启后再运行插件安装器。已有兼容版本直接跳过。共享运行库不随插件卸载。

## 安装、更新和卸载

双击 `SpectralResonator-<版本>-Windows-x64-Setup.exe`，可选择简体中文／英文，并选择当前用户或所有用户。默认无需管理员权限；所有用户安装需要管理员授权。

首次安装可浏览选择 **CLAP 插件目录**，文件直接放在选定目录下，不额外追加品牌子目录。默认遵守 [CLAP 官方 entry.h](https://github.com/free-audio/clap/blob/main/include/clap/entry.h) 的目录约定：

| 安装范围 | 默认插件目录 | 卸载器／图标目录 |
| --- | --- | --- |
| 当前用户 | `%LOCALAPPDATA%\Programs\Common\CLAP` | `%LOCALAPPDATA%\Programs\AflocatAudio\Spectral Resonator` |
| 所有用户 | `%COMMONPROGRAMFILES%\CLAP` | `%ProgramFiles%\AflocatAudio\Spectral Resonator` |

自选目录必须是可写的本地绝对目录，不能是盘符根目录。选择受保护目录时应使用所有用户模式。非标准目录需在音频宿主的插件位置中添加，再重新扫描。本安装器不修改宿主设置。

新版安装包沿用稳定 AppId、插件名 `my_spectral_resonator.clap` 和原目录，以覆盖方式升级；同版本可重复安装修复。旧版安装包拒绝覆盖较新的已安装版本。更新页锁定原目录，命令行请求不同目录也会拒绝；如需迁移，先卸载再安装。当前用户与所有用户之间的已有安装会互相拦截，需先卸载再切换范围（检查当前登录用户及机器级安装，无法枚举其他账户的私有安装）。

安装／更新／卸载之前请关闭加载此插件的音频宿主。安装器通过独占文件打开检测 DLL 占用；占用时提示关闭宿主后重试，不主动结束宿主，不安排重启替换。

从 Windows **设置 → 应用 → 已安装的应用 → Specatral Resonator → 卸载** 移除，也可运行支持目录中的 `unins000.exe`。卸载只删除安装器记录的插件文件、图标、卸载器及本产品注册信息。同目录的其他插件、工程和 `%APPDATA%\com.aflocat.audio\settings.json` 均保留；该配置供同系列插件共享。

之前手动复制的插件无法通过安装记录自动发现：同名文件位于本次目标目录时，会询问是否替换并纳入卸载管理；静默安装默认拒绝。位于其他扫描目录的手动副本请自行移除，避免重复 ID。旧 `.clap` 文件不是安装包，因此本版不能凭空推断它之前存在哪里。

安装包目前未作发布者代码签名；Windows 可能显示未知发布者或 SmartScreen 提示。编译器下载的官方签名验证不代表本项目安装包已签名。

## 构建

需要 PowerShell、项目 Rust 工具链和 Inno Setup。获取固定版本的便携编译器及微软运行库分发包（下载需网络，只写入 `target/installer-tools`，不注册编译器到系统，也不在开发机执行运行库安装）：

```powershell
./scripts/setup-installer-tools.ps1
./scripts/build-windows-installer.ps1 -Offline
```

如果已经安装 Inno Setup，可用 `-IsccPath 'C:\...\ISCC.exe'` 指定；仍需先准备经过哈希校验的运行库分发包。默认先调用 `bundle-clap.ps1 -ArtifactOnly` 构建，再打包；不会写入可能正被音频宿主占用的 `target/bundled`。版本从插件 `Cargo.toml` 读取。

已有经过验证、对应当前版本的 `target/artifacts/<版本>/my_spectral_resonator.clap` 时，可加 `-SkipBuild` 只打安装包；调用者需确认该文件确实是所需的构建。输出为 `target/installers/<版本>/SpectralResonator-<版本>-Windows-x64-Setup.exe`，旁边 `.exe.json` 保存插件、安装包和编译器哈希。图标从 `assets/icon` 的已有 PNG 在构建时生成 ICO，不改写图标源文件。

普通静默安装示例：

```powershell
& '.\SpectralResonator-0.15.0-Windows-x64-Setup.exe' /CURRENTUSER /VERYSILENT /SUPPRESSMSGBOXES /NORESTART '/CLAPDIR=D:\Audio Plugins\CLAP' '/LOG=D:\setup.log'
```

使用 `/ALLUSERS` 选择机器级安装；更新时省略 `/CLAPDIR` 自动沿用原目录。`/DIR` 是 Inno 的支持文件目录参数，不是插件目录；正常向导不开放它。

## 回归验证

生命周期脚本需要 PowerShell 7（用 .NET 的 NativeLibrary 加载真实 DLL 来模拟占用）。

```powershell
./scripts/test-windows-installer.ps1
```

使用真实 0.14.1 和当前版本的不同 CLAP 二进制；可用 `-PreviousVersion` / `-PreviousPluginPath` 指定另一个旧版本。测试产品有随机 AppId 和独立注册项，插件及支持文件被编译时限制在 `target/installer-tests/<id>`。测试会在 HKCU 创建后删除独立安装／卸载记录，需要该权限，但不会操作真实音频宿主或正式插件安装。

覆盖中文与空格路径、旧版安装、已加载 DLL 下更新／卸载失败且保留文件、升级目录记忆、拒绝迁移、同版本修复、拒绝降级、正常卸载、同目录其他文件保留、共享设置不变、同名手动文件保护、重新安装与再次卸载。另有仅测试包支持的缺运行库模拟分支，验证前置检查可以阻止半安装；测试包硬性禁止安装系统运行库。记录留在测试目录中。测试成功不等同于真实音频宿主扫描、Bounce、干净系统上运行库安装或 UAC 全流程验收。

`probe-windows-installer.py <隔离测试包路径>` 使用 Python + Pillow 捕获该测试安装器自身的目录、摘要和完成页，检查中文扫描提示，并卸载该测试产品。只接受 `target/installer-tests/<id>` 下的安装包，不接触宿主窗口。

## 第三方来源

编译器：[Inno Setup 6.7.3](https://github.com/jrsoftware/issrc/releases/tag/is-6_7_3)，官方发布 SHA-256 固定在工具安装脚本中，并验证 Authenticode 签名。

运行库：[微软 Visual C++ 可再发行组件说明](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)。构建脚本固定官方 CDN 文件的 SHA-256，工具准备脚本同时验证 Microsoft Corporation 签名。运行库按其自身微软许可分发，由微软安装程序管理系统级升级和卸载。

`ChineseSimplified.isl` 原样来自同版本仓库的 [Files/Languages/Unofficial/ChineseSimplified.isl](https://github.com/jrsoftware/issrc/blob/is-6_7_3/Files/Languages/Unofficial/ChineseSimplified.isl)。保留原作者注释；许可证见 [INNO-LICENSE.txt](INNO-LICENSE.txt)。它只翻译安装向导，不改变插件中已保存的共享语言偏好。
