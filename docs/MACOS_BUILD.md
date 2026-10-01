# macOS 云端构建与测试

没有 Mac 时，可通过本仓库的 GitHub Actions **macOS build and test** 使用 GitHub 托管的 Mac。工作流分别在 `macos-15`（Apple Silicon / arm64）和 `macos-15-intel`（x86_64）上原生编译并运行测试，不使用 Rosetta 来代替另一种架构的执行。

2026-10-02 已完成两个架构的实际云端编译和自动测试，结果、测试包及 Intel 非正规数性能警告见 [验证记录](validation/MACOS_CI_2026-10-02.md)。

## 范围

- 使用 `rust-toolchain.toml` 固定的 Rust 和 `Cargo.lock` 构建当前 CLAP 插件。
- 运行 workspace 的库测试；原有需要 GPU 的忽略测试不会被当作通过。
- 生成含 `Contents/Info.plist` 和 Mach-O 可执行文件的 `.clap` bundle，检查架构、导出入口和 ad-hoc 签名。
- 解压实际生成的 ZIP 后运行上游 CLAP validator 和项目现有的音频生命周期、状态保存、尾音及离线渲染探针。
- 保存环境、提交 SHA、测试日志、ZIP 哈希和对应源码归档。

这些结果证明相应云端环境中的编译、自动测试及插件接口行为，不代表真实音频宿主、交互界面、Retina 缩放、设备实时音频或真实工程 Bounce 已验收。没有 AU 导出，因此这些 CLAP 包不适用于 Logic Pro。VST3 和 Universal 2 不在本次范围。

## 执行方式

工作流在相关源码推送到 `main`、`codex/macos-*` 分支或针对 `main` 的 PR 时执行，也提供 `workflow_dispatch` 手动入口。首次新增工作流尚未进入默认分支时，可通过推送匹配的测试分支触发。

在 GitHub 仓库的 Actions 中打开执行记录。两项架构任务均通过后，可下载：

- `macOS-arm64-test-package`：Apple Silicon 测试包与源码归档。
- `macOS-x86_64-test-package`：Intel 测试包与源码归档。
- `macOS-*-validation`：对应的环境和测试证据，失败时也尽可能保留。

工作流只使用标准托管 runner，权限为 `contents: read`，不需要 Apple 证书、私钥或仓库写权限，不创建 Release。制品保留 14 天，需长期保留时应下载归档。

## 有 Mac 时本地复现

需要 macOS 15 或更新版本、Xcode Command Line Tools、rustup、Python 3.11+：

```sh
bash scripts/bundle-clap-macos.sh
cargo test --workspace --lib --release --locked
```

依赖已缓存时可传 `--offline`。脚本固定 `MACOSX_DEPLOYMENT_TARGET=15.0`，当前仅声明 macOS 15+ 测试包；没有验证更老系统。产物位于：

```text
target/artifacts/<version>/macOS-arm64/SpecatralResonator-<version>-macOS-arm64-test.zip
target/artifacts/<version>/macOS-x86_64/SpecatralResonator-<version>-macOS-x86_64-test.zip
```

脚本只构建当前机器的原生架构，不修改用户插件目录。ZIP 内的 `.clap` 是目录 bundle，不能按 Windows 的 DLL 文件处理。之后进行人工宿主测试时，可将完整 bundle 放入 `~/Library/Audio/Plug-Ins/CLAP/`，避免同一插件 ID 的多份副本同时被扫描。

测试包仅做 ad-hoc 签名，没有 Developer ID 签名或公证；下载后 Gatekeeper 仍可能拦截。这不能作为普通用户开箱即用的正式安装包。正式发布还需完成签名、公证、第三方许可材料及真实宿主验收，参见 [发布许可审计](validation/RELEASE_LICENSE_AUDIT_2026-09-30.md)。
