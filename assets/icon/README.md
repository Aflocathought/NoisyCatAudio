# Specatral Resonator icon

## 许可范围

本目录由项目自行生成的 `spectral-resonator.svg`、`spectral-resonator-transparent.svg`、`spectral-resonator-<尺寸>.png`、`spectral-resonator-transparent-<尺寸>.png` 以及 `preview.png`，按 [CC0 1.0 Universal](LICENSE-CC0) 开放。尺寸为 32、64、128、256、512、1024。当前维护者在依法有权处分的范围内作出该声明，CC0 本身不要求署名。

`spectral-icon.js`、`generate.mjs`、`index.html` 和文档属于软件工具或文档，继续采用根目录的 [GPLv3](../../LICENSE)，不因图形采用 CC0 而改变。第三方研究样本、字体、商标或其他第三方权利不在此授权范围内。名称共享规则见 [名称与标志声明](../../docs/NAMING_AND_LICENSE.md)。

## 图形与使用

蓝色宽频轮廓承托逐层抬升、逐层变白的共振峰。默认四层共振，加一层蓝色输入频谱。颜色来自 `crates/spectral-resonator-plugin/src/editor.rs` 的 `BLUE` / `WHITE` / `INK`：`#3593FF` / `#E8F2FF` / `#080F1A`。图形是品牌化的频响示意，不代表实际测量结果。

直接打开 `index.html`，可调整层数、间距、线宽和背景，导出 SVG / PNG。网页不需要网络、构建工具或服务器。

- `spectral-resonator.svg`：深色圆角底版，适合应用图标、头像、浅色页面。
- `spectral-resonator-transparent.svg`：透明标记版，适合深色界面。
- `spectral-resonator[-transparent]-{32,64,128,256,512,1024}.png`：已导出的 RGBA 位图。
- `preview.png`：图标及尺寸对照。
- `spectral-icon.js`：浏览器与 Node 共用的图形生成函数，没有运行时依赖。

## 在网页复用

固定图标可直接作为图片：

```html
<img src="assets/icon/spectral-resonator.svg" width="48" height="48" alt="Specatral Resonator">
```

需要参数控制时：

```html
<div id="brand"></div>
<script src="assets/icon/spectral-icon.js"></script>
<script>
  document.getElementById('brand').innerHTML = SpectralIcon.svg({
    id: 'brand', // 同一文档内每个内联实例使用不同 ID，避免渐变引用冲突。
    size: 128,
    layers: 4, // 2–5
    spacing: 34, // 20–40
    weight: 8, // 5–12
    background: 'dark', // 或 'transparent'
  });
</script>
```

SVG 的 `viewBox` 固定为 `0 0 512 512`，尺寸可自由缩放，不依赖字体或外部图片。小于 32 px 建议减少至 2–3 层或适当加粗；透明版的冷白线条在浅色底上对比度有限，应选择深色底版。

## 重新生成文件

在项目根目录运行：

```powershell
node assets/icon/generate.mjs
```

同时生成 PNG 需要可解析到 `sharp`；网页导出 PNG 不需要安装该依赖。

```powershell
node assets/icon/generate.mjs --png
```

如 `sharp` 安装在单独的位置，可用环境变量 `SPECTRAL_SHARP_PATH` 指定包目录。不涉及插件编译或音频处理。
