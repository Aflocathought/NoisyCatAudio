# Noisy Cat Audio shared preferences

供同一品牌的多个音频插件复用的用户配置模块。`com.aflocat.audio` 是命名空间，不要求注册公司，也不是 Rust 特殊目录。

- Windows：`%APPDATA%/com.aflocat.audio/settings.json`
- macOS：`~/Library/Application Support/com.aflocat.audio/settings.json`
- Linux：`$XDG_CONFIG_HOME/com.aflocat.audio/settings.json`，未设置则使用 `~/.config/`。
- 测试或便携部署可设置绝对路径 `AUDIO_PLUGIN_SETTINGS_ROOT`，命名空间仍追加在其下。

```json
{
  "schema_version": 1,
  "global": {
    "maximum_ui_fps": 60,
    "language": "system"
  },
  "plugins": {}
}
```

`maximum_ui_fps` 支持 30、60、90、120；`language` 当前可选 `system`、`en`、`zh-CN`，未知语言代码也保留以供未来版本使用。语言字段本身不会自动翻译界面。

## 插件接入

在编辑器打开时调用 `Preferences::open(settings_path("com.aflocat.audio")?)`，保留返回的 `Arc`。UI 使用 `snapshot()` 读取缓存，调用 `set(Edit::global("maximum_ui_fps", json!(90)))` 修改全局项。插件专属偏好使用 `Edit::plugin("插件稳定 ID", "字段名", value)`，不要写入别的插件的子对象。音色参数、FFT 选项等工程状态不属于此文件。

同一动态库内、同一路径的编辑器共用一个后台线程。磁盘读取和写入均在该线程，UI 只读写内存；不允许从音频回调调用本模块。最后一个编辑器关闭时停止并 join 线程，避免 DLL 卸载后仍执行插件代码。关闭时最多额外重试 500 ms 的锁争用；磁盘 I/O 本身的耗时由系统决定。出现权限错误、坏文件或长期占锁时，未写入的修改可能无法保存，界面应展示 `snapshot().error`，而不能假装保存成功。

## 多插件协作规则

1. 所有写入方都必须使用同一协议：在稳定的 `settings.lock` 文件上取得系统排他锁，锁文件永不主动删除。
2. 锁内重新读最新 JSON，仅合并本次改动的键。未知字段与其它插件的数据保持原样；不同字段的并发修改可同时保留，同一字段由最后成功写入的值生效。
3. 同目录创建唯一临时文件，写完并 flush/sync 后 rename 替换 JSON。读取方看到完整旧版或完整新版；进程崩溃时系统释放文件锁，旧的 JSON 不会被半截内容覆盖。
4. 其它进程的更新每 500 ms 读取一次，不在每帧访问磁盘。待保存键覆盖刷新值，写入成功后才确认；保存进行中再次修改同一键不会被较早的完成结果抹去。
5. `schema_version` 不兼容、文件格式损坏或超过 1 MiB 时拒绝覆盖。相同版本的未知键保留。未来需要升级结构时应先实现迁移协议。

以上约定针对本机正常支持锁与原子替换的文件系统。手工修改文件或其它不遵守锁协议的程序不属于协调范围。

## 检查

`cargo run -p audio-plugin-settings --example preferences --offline -- --init` 可初始化共享文件，保留现有值。不加 `--init` 时只读取；文件缺失则显示默认值。

测试写入 `target/settings-tests/`，覆盖进程间竞争、读者完整性、持锁进程崩溃、客户端刷新、保存后重开、旧字段兼容和损坏文件保护。不会修改真实用户配置。
