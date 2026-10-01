# 发布许可与知识产权审计：2026-09-30

本轮对象是当前未提交工作树中的 Spectral Resonator 0.15.0、Windows x86-64 CLAP 依赖配置，以及现有 Windows 安装流程。它延续 2026-09-29 的依赖许可证核查，新增安装器、微软运行库、二进制字体和产品名称的核对。

**结论：没有发现需要仅为依赖兼容而把自有代码改为 GPLv3 的依据；但现有发布流程存在应在公开发布前处理的许可声明和运行库条款流程缺口。暂不建议把当前安装包直接作为“已完成许可合规审查”的正式版本发布。** 这不等于已经认定侵权，也不意味着必须停下开发。

MIT 或 GPLv3 都不能代替这些工作。本轮不替作者确定许可证，不对代码、图标或其他资产重新授权。

## 后续更新：2026-10-01

作者先选择 MIT，随后明确改为 GPLv3。本次最终声明为 GPL-3.0-only：完整标准 [LICENSE](../../LICENSE)、三个自有 crate 的元数据，以及 [版权范围](../../COPYRIGHT.md) 已同步。自有标志另按 CC0 1.0 开放；名称、标志、内测风险和输出作品权属见 [README](../../README.md) 与 [名称声明](../NAMING_AND_LICENSE.md)。先前已经对外授予的 MIT 权利不追溯撤回。

F01 中的项目许可文件、Cargo 元数据及资产许可范围说明已补齐，F08 的资产授权说明已补充；第三方通知、字体、运行库条款流程和发布包许可交付仍待处理。GPL 选择没有自动解决这些问题。显示品牌现为 Noisy Cat Audio，插件名为 Specatral Resonator；名称未完成商标检索。现有安装包未重建。

2026-10-01 按作者要求对公开文档作中性措辞整理，原始审计记录和源码快照已另行归档。下文保留历史发现、版本及哈希；宿主名称的省略不扩大任何验证结论，原始记录中的来源与署名并未从归档证据中删除。

## 整理更新：2026-10-02

项目／社区继续使用 Noisy Cat Audio，产品为 Specatral Resonator。三个自有 crate 已配置正式仓库地址，插件网站、手册、支持入口及安装器链接已指向该仓库；删除虚构邮箱，使用 Issues 作为公开反馈入口。F07 中的网址占位问题已在源码中处理，尚未验证重新构建的发布物。以下发现清单保留原审计时点的事实；第三方通知、字体许可和运行库同意流程的交付缺口仍未关闭。

## 1. 证据范围与作者确认

- 作者在本次审计中确认：本项目不涉及雇主、学校或客户安排的任务、相关知识产权协议，也未使用他们的代码、素材或保密资料，属于个人项目。这是作者陈述，本轮没有独立审阅相关合同。
- HEAD：`ec71770ec8f13704bcaa1593af65f2fce9f6831e`。存在多项已有未提交变更，不能以 HEAD 单独代表本轮源码。
- Cargo：`1.95.0 (f2d3ce0bd 2026-03-21)`。使用 `--locked --offline --filter-platform x86_64-pc-windows-msvc` 重新取得依赖信息。
- Windows metadata 共 183 个包：3 个自有 crate、169 个插件普通及构建依赖包、11 个开发视图附加包。169 包包含构建工具和过程宏，不代表全部进入最终 DLL。
- 当前视图未出现 VST3 相关包；本结论不扩展到其他平台、其他 feature、未来 VST3 或新增样本资源。
- 审计读取了元数据、许可文本、补丁记录、字体及现有二进制，核对了安装脚本与制品哈希；没有重新构建、执行生产安装器或修改系统运行库。
- 没有完成全量代码相似性比对、商标注册簿查询、专利自由实施分析、中国裁判文书检索或所有发布平台规则审查。这份记录不是不侵权保证或律师法律意见。

## 2. 发现清单

P1 表示正式发布前应解决或明确核实，P2 表示建议完善。优先级是处理顺序，不是诉讼概率或违法认定。

| 编号 | 优先级 | 发现与证据 | 建议及完成标准 |
| --- | --- | --- | --- |
| F01 | P1 | 根目录没有项目 `LICENSE`，三个自有 crate 未声明 `license`；图标和文档的授权范围也没有统一说明。 | 确定实际版权署名与自有内容的许可范围；采用完整标准许可，设置 Cargo 字段。明确第三方代码、字体、微软运行库等继续适用原许可。 |
| F02 | P1 | 裸 CLAP 打包只复制二进制；安装器 `[Files]` 只有 CLAP、图标及临时运行库载荷，没有项目许可或第三方声明文件。 | 为源码包、裸插件压缩包和安装器分别建立许可交付路径。最终用户应能获取完整的必要许可、版权及适用 NOTICE，不能仅提供“用了 MIT 库”的列表。 |
| F03 | P1 | 四套默认字体均已按完整原始字节嵌入当前 CLAP。现有发布脚本没有提供字体许可文件或可阅读的许可页。 | 补齐 Hack、Noto Emoji、Ubuntu Light、emoji-icon-font 对应版权与许可；保留字体自身许可和适用名称限制。不能把所有字体改称 MIT。 |
| F04 | P1 | nice-plug 系列四个包的 Cargo 字段声明 ISC，但当前本地包中没有独立完整许可文件。另有其他包也存在包级许可文件缺省情况。 | 根据实际版本与上游提交补齐声明。已取得两个对应上游提交的 nice-plug ISC 全文及权利人信息，供后续实施；其他包不能仅凭通用模板替代其版权信息。 |
| F05 | P1 | 安装器内嵌微软运行库，并在缺失时以 `/install /passive /norestart` 执行；本项目向导没有许可页或针对运行库条款的同意流程。 | 按实际微软许可明确运行库分发依据、条款呈现及所需同意。优先评估保留微软交互式条款流程，或不内嵌并引导用户从官方来源安装；将微软条款与自有开源代码许可分开。 |
| F06 | P2 | 当前产品显示名和安装器名称均为 `Spectral Resonator`；已有其他产品使用相同描述性名称。 | 发布前考虑更有辨识度的独立产品名，至少突出可辨识的项目名称并说明无隶属或背书关系。完成目标市场名称检索后再评估是否需要更名；本轮没有认定该词为他人的有效注册商标。 |
| F07 | P2 | 插件网站、邮箱、手册及支持链接仍是 `example.invalid` 占位地址；仓库按钮因缺少 repository 元数据而禁用。 | 配置真实官方仓库与专用联系入口，说明免费、不定期维护及第三方售后的边界。投诉与普通支持应有可用渠道。 |
| F08 | P2 | 已有图标生成源码与有限相似性记录，但它们不构成完整商标检索，也未明确资产授权范围。 | 保留生成代码、版本与发布记录，明确图标和文档许可；对新增采样、IR、图片、字体及外部贡献记录来源，避免只凭“网上可下载”就合并。 |
| F09 | P2 | 安装包无发布者代码签名；已有构建哈希清单，但清单没有锁定完整源码和许可材料版本。 | 签名可提高来源辨识，但不是软件合法性的证明或当前必须购买的服务。先固定官方发布入口、源码快照、依赖锁、构建工具与声明材料哈希；条件合适时再签名。 |

相关本地证据：

- [裸 CLAP 打包脚本](../../scripts/bundle-clap.ps1)：复制制品；脚本没有许可材料打包步骤。
- [安装器载荷清单](../../installer/windows/spectral-resonator.iss)：三个 `Source` 项。
- [运行库启动逻辑](../../installer/windows/spectral-resonator.iss)：被动安装参数。
- [插件名称与联系方式](../../crates/spectral-resonator-plugin/src/lib.rs)及[手册、支持链接](../../crates/spectral-resonator-plugin/src/lib.rs)。
- [仓库入口读取逻辑](../../crates/spectral-resonator-plugin/src/preferences.rs)。

## 3. Rust 依赖：结论与需要保留的区别

本次重跑结果与前次 Windows 包计数一致。所有第三方包都有 Cargo 许可表达式，三个缺省项均为自有 crate。没有从当前声明中发现必须把插件自身置于 GPL 下的依赖；这只是声明层面的兼容路径筛查，不是每个上游源文件的完整权属证明。

| 组件或表达式 | 本轮确认与发布要求 |
| --- | --- |
| nice-plug 0.4.2、nice-plug-core 0.4.2、nice-plug-egui 0.5.1 | 对应上游提交 `91ba75d20c43ca0cd21afa802d0ae8c0511f48ed`。上游 LICENSE 为 ISC，包含 `Copyright (c) 2022-2024 Robbert van der Helm`，要求在副本中保留版权和许可声明。 |
| nice-plug-derive 0.1.2 | 对应另一提交 `ba4eb8aca372be239d25cabf8b4bdfe6bbce911c`。本轮分别取回其 ISC 许可，避免仅按当前主分支或其他包的版本推定。 |
| self_cell 1.3.0 | `Apache-2.0 OR GPL-2.0-only` 是备选许可；可以选择 Apache-2.0 路径，不是被迫采用 GPL。 |
| dpi 0.1.2 | `Apache-2.0 AND MIT`，同时满足；本地含 `LICENSE` 和 `LICENSE-LIBM-MIT`。 |
| unicode-ident 1.0.26 | `(MIT OR Apache-2.0) AND Unicode-3.0`，Unicode 条款仍需覆盖。 |
| egui / epaint / wgpu 等 | 多数为 MIT 或 Apache-2.0。选择合适路径后仍需保留相应版权、许可及适用 NOTICE。 |
| clipboard-win、error-code | `BSL-1.0` 表示 Boost Software License，不是 Business Source License。 |
| Rust 标准库与工具链带入代码 | 不属于 Cargo 包计数。本轮保存了所用 Rust 1.95.0 的 `COPYRIGHT-library.html` 作为后续声明整理依据；未声称其中列出的所有平台组件都进入本插件。 |

自动盘点在 15 个第三方包中没有找到名称含 LICENSE、NOTICE、COPYING 等的候选文件。**“没有独立文件”不等于没有许可。** accesskit、gl_generator、khronos_api 的源码含许可头；其他包可能依赖上游 workspace 的许可文件。此项用于定位需要补全声明的来源，不用来直接认定无权使用。

这 15 个包为：accesskit、clipboard-win、ecolor、egui、egui-wgpu、emath、epaint、gl_generator、khronos_api、nice-plug、nice-plug-core、nice-plug-derive、nice-plug-egui、profiling、spirv。版本与路径见逐包 CSV。

从普通及构建依赖链保存了 303 个候选许可、版权和通知文件，并记录哈希。它们包含子目录内容，**不是已经校审完成的最终 THIRD_PARTY_NOTICES**；需要去掉不适用部分、补齐缺失文本，并核对真正分发的对象及选用许可路径。

### 本地 vendor 补丁

四个 vendor 包都有 `PATCHES.md`。对本机同版本原始包缓存的比较显示：baseview、egui-baseview、nice-plug-egui 的现有文件没有被删除，差异集中于源码补丁、Cargo 配置及新增补丁记录。nice-plug 对应原始包缓存本轮未找到，因此未完成同样的逐文件比较。

baseview 与 egui-baseview 保留 MIT / Apache-2.0 文本。采用 MIT 路径时保留相应声明；若采用 Apache-2.0 路径，应另外核对修改文件显著标记及适用 NOTICE 等要求，不能认为只有一个 PATCHES.md 就已经满足所有要求。

nice-plug README 单独将 branding 标志列为 CC BY-SA 4.0；本项目当前的自有图标不因此自动适用该许可。不要把上游代码许可直接套用到其标志、截图或其他素材。

## 4. 已进入二进制的字体

这部分不是仅根据 `default_fonts` feature 推测。本轮直接在现有 CLAP 中找到四份 TTF 的完整原始字节序列，并记录文件哈希及偏移：

| 字体 | 原始字节数 | 在 CLAP 中的十进制偏移 | 许可来源 |
| --- | ---: | ---: | --- |
| Hack-Regular.ttf | 309408 | 10798272 | MIT、Bitstream Vera 条款；另有 DejaVu 公有领域贡献说明 |
| NotoEmoji-Regular.ttf | 418804 | 11107680 | SIL OFL 1.1；字体版权字段为 Google 2013 |
| Ubuntu-Light.ttf | 361676 | 11526484 | Ubuntu Font Licence 1.0；字体版权字段为 Canonical 2011 |
| emoji-icon-font.ttf | 324132 | 11888160 | 对应字体包中的 MIT 许可文本 |

检查 TTF name 表时，Hack 中确实包含较完整许可文本；Noto 包含许可摘要及链接，Ubuntu 包含版权与许可名称，emoji-icon-font 在所查版权和许可字段中没有对应记录。因此不能笼统说“二进制里完全没有任何版权信息”，也不能反过来认为“只要嵌入字体，用户就已经获得所有必要条款”。

建议把完整字体许可和版权集中放进可阅读的许可目录 / 页面，随安装器和裸插件发布。OFL 与 UFL 允许适当方式的随包或元数据声明，但要求满足各自条款；不能要求用户先逆向插件才能找到它们。

## 5. Windows 安装器与微软运行库

### 5.1 Inno Setup 与中文翻译

仓库的 `INNO-LICENSE.txt` 与本机 6.7.3 编译器附带的 `license.txt` SHA-256 一致：

`2E5346868C2A18434489824E11D65C3031620F792FEFC415D05F19CD441ABF5C`

已读条款允许商业用途，但要求保留适用版权、网站信息、不冒认来源、标明修改等。不能仅因工具免费就忽略这些条件，也不能将 Inno Setup 自身声明换成项目 MIT。

仓库中的 `ChineseSimplified.isl` 与上游 `is-6_7_3` tag 下文件在换行归一化后相同，保留了维护者及来源注释。这个比对确认文件来源一致，不是独立审查翻译中每段文字的完整权利链。

### 5.2 微软运行库有条件的再分发路径

现有包内载荷是微软 Visual C++ x64 运行库 14.44.35211.0。本轮验证其 SHA-256 与构建脚本一致，Authenticode 状态为 Valid，签名者为 Microsoft Corporation。**这证明所查文件的来源完整性，不独立证明项目已履行分发条件。**

微软官方部署文档明确说明，再分发限于获得适用 Visual Studio 许可的用户，并受 Microsoft Software License Terms 约束。随后进一步核对到：

1. 本机安装 `Microsoft.VisualStudio.Product.Community`，显示版本 17.14.23（安装版本 17.14.36811.4）。
2. Visual Studio Community 2022 条款的 Individual License 允许个人开发、测试自己的应用，无论出售或其他用途。作者本轮确认属于个人项目，因此未发现必须购买商业 Visual Studio 许可的依据；本轮没有独立审查许可接受记录或全部实际使用条件。
3. 同一条款的 Distributable Code 部分授予列明组件的再分发权。官方 2022 REDIST 清单也覆盖符合条件的 Visual C++ Runtime 文件和官方来源的运行库分发包，要求保持适用文件不变并遵守许可。
4. 分发要求包括：应用提供实质主要功能，并要求分销商及外部最终用户同意能充分保护该微软组件的条款。运行库自己的终端用户许可不是一份任意再分发授权，不能只读“可以安装和使用任意份数”就推导出可以任意转售、改授或打包。

当前源码仅显示安装前告知“将安装微软运行库”，随后启动 `/passive` 流程。微软官方文档将 `/passive` 描述为显示进度、但不要求其他用户交互；本项目向导没有 `LicenseFile` 或独立条款接受环节。因此，现有材料不足以证明已满足所需的条款同意和传递要求，这是需要修复的流程问题。

不需要把微软条款强加给 MIT 自有代码。可以明确区分组件，在安装微软运行库时呈现适用条款并实现所需同意；或者不在包中内嵌该组件，向缺失运行库的用户提供官方安装指引。最终实现应核对实际版本条款和交互，而不是简单把 `/passive` 换成另一个参数后宣称合规。

如果以后改用 GPLv3，也不能把微软组件本身重授为 GPL。分开分发、系统库边界和组合方式需要按实际发布物判断；这里没有得出“GPL 插件一律不能运行在 Windows”或“一律可以覆盖所有打包方式”的结论。

### 5.3 当前制品与可追溯性

| 对象 | 本轮读取的 SHA-256 |
| --- | --- |
| CLAP 0.15.0 | `5D7C77F40E1BAF0FB6703CFD4B7C51A4696963A0C53D106FEA96C1178537C260` |
| Windows Setup 0.15.0 | `9FFFB99217B2E702A0919C5A9E918877D5C55E083C00DDE02A625E5A6A5B63D9` |
| vc_redist.x64.exe | `CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B` |
| 当前 Cargo.lock | `97FE930C008B1055A4DB984F5DBD497310B4055305823AC4104C8E6A81696357` |

安装包签名状态为 NotSigned，与项目文档一致。上述制品哈希与现有 `.exe.json` 构建记录相符。本轮没有重新解包核对整个 Setup 的全部内部载荷，也没有从清洁源码重构建来证明当前工作树与制品逐字对应；安装器缺口主要依据当前构建脚本及源文件，字体嵌入则已直接核对实际 CLAP 字节。

## 6. 名称、图标与来源识别

实际插件 `NAME`、安装器 `ProductName` 都是 `Spectral Resonator`；外部产品文档也使用相同描述性名称。本项目文档明确记录了参考对象，且强调独立实现、不保证相同音色，这种区分应保留。

同名是已确认事实；它是否构成商标侵权或不正当竞争，需要结合权利状态、地域、显著性、使用方式及混淆等判断。这里没有完成中国商标或产品名称权益检索，不能把同名直接判成侵权，也不能把描述性较强直接视为绝对安全。

对个人维护者，采用有辨识度的独立产品名、统一呈现当前项目名称、明确无隶属或背书关系，通常比长期解释两个产品的区别更省精力。品牌自身也尚未完成检索；免责声明不能替代实际不混淆的名称和页面设计。当前 CLAP ID 还处于开发命名；若后续调整，应评估已保存工程兼容性，不能为改名随意更换 ID。

本轮复核图标生成代码：SVG 由数学曲线及项目配色构造，没有在该生成代码中发现导入第三方图片或字体轮廓。默认 SVG、透明 SVG 及生成 JS 的哈希与 2026-09-29 记录一致。该记录只对照了八个官网样本，不等于反向图片检索或商标清查；本轮没有扩大视觉比对样本，也没有据此认定图标具有确定、完整的排他权。

第三方官网标志研究副本位于被忽略的 `target/`，本轮所查发布脚本未把这些研究副本列为载荷。以后发布源码归档时仍应按明确文件清单生成，避免直接压缩整个开发目录。

## 7. 修复顺序与验收标准

1. **确定自有内容授权。** 明确版权署名、代码 / 文档 / 图标范围，添加标准 LICENSE 与 Cargo 许可元数据；保留第三方例外。MIT 或 GPLv3 是这一项的选择，不影响下面各项仍需完成。
2. **完成第三方声明。** 以本轮包清单和候选文本为底稿，核对许可选择、原始版权、字体及 Rust 标准库通知。补齐上游缺省文本，生成可阅读材料，并保留版本与来源。
3. **修正运行库安装流程。** 确认适用微软条款与用户同意路径，或改成明确的外部先决条件；为失败、取消和静默安装定义一致行为。只对微软组件适用其条款。
4. **贯通所有发布形式。** 裸 `.clap` 不再作为唯一下载文件；提供含必要声明的发布归档或其他满足许可的交付方式。安装后的许可材料应可找到，卸载不得误删其他插件材料。
5. **建立官方识别与维护边界。** 配置真实网址、邮箱、支持信息及品牌说明。对名称做目标市场检索，选择是否更名。说明不定期维护，保留现有性能和音频宿主验收边界，不承诺“保证不侵权”或“绝对稳定”。
6. **验证新的发布物。** 检查最终归档 / 安装产物中的许可材料、版本、源码快照与哈希。安装器变更完成后再做隔离安装、取消、升级及卸载检查；本轮审计没有代替这些未来验收。

外部贡献规则可采用简明的来源确认：贡献者有权提交，贡献按项目既定许可提供；新增代码、素材或字体注明来源。DCO 或签署记录有助于追溯，但不转移第三方实际权利，也不能代替审查。对正常 MIT / GPL 许可使用者不需要逐个出具商业授权书或质量保证。

## 8. 已查官方来源与可复核材料

本轮实际读取的官方资料：

- [Microsoft Learn：最新支持的 VC++ Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170)：再分发资格及运行库版本说明。
- [Microsoft Learn：Redistribute Visual C++ files](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170)：资格、部署方式、`/passive` 行为；另保存其 [MicrosoftDocs 官方源文](https://github.com/MicrosoftDocs/cpp-docs/blob/main/docs/windows/redistributing-visual-cpp-files.md)。
- [Visual Studio Community 2022 许可入口](https://visualstudio.microsoft.com/license-terms/vs2022-ga-community/)及该页嵌入的[官方许可正文](https://visualstudio.microsoft.com/wp-content/uploads/2021/11/Visual-Studio-2022-Community-License-EN.docx)：个人使用权和 Distributable Code 条款。
- [Visual C++ Runtime 2015-2022 许可入口](https://visualstudio.microsoft.com/license-terms/vs2022-cruntime/)及[官方许可正文](https://visualstudio.microsoft.com/wp-content/uploads/2021/09/Visual-C-Runtime-2015-2022-License-1.docx)：终端用户使用范围，不能与开发者再分发授权混为一谈。
- [Visual Studio 2022 REDIST 清单](https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution)：Visual C++ Runtime Files 部分。
- [nice-plug 对应提交 LICENSE](https://codeberg.org/RustAudio/nice-plug/raw/commit/91ba75d20c43ca0cd21afa802d0ae8c0511f48ed/LICENSE)及 [derive 对应提交 LICENSE](https://codeberg.org/RustAudio/nice-plug/raw/commit/ba4eb8aca372be239d25cabf8b4bdfe6bbce911c/LICENSE)。
- [Inno Setup 6.7.3 中文翻译源文件](https://github.com/jrsoftware/issrc/blob/is-6_7_3/Files/Languages/Unofficial/ChineseSimplified.isl)。

本机证据目录：

`license-audit/2026-09-30/`（维护者本地审计归档，不随仓库分发）

其中 `packages-windows.csv` 保存逐包版本、许可表达式、范围与路径；`notice-files.csv` 保存候选声明及哈希；`notice-evidence/` 保留候选原文；`inventory-summary.json` 保存统计、vendor 差异、字体字节位置和素材哈希；`font-copyright-metadata.json` 保存字体内嵌声明；`source-snapshot/` 保存关键打包输入；`checked-source-hashes.json` 保存核查源码指纹。另有官方资料快照与 `inspect_inventory.py` 复核脚本。

本报告是发布前的有限范围审计记录。下次更换依赖、工具链、目标平台或运行库后，应按新的实际发布物重新检查，不把本轮结论自动沿用。
