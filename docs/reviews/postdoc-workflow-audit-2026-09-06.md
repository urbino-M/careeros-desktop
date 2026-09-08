# Postdoc 工作流集中逻辑审查 — 2026-09-06

## 后续实施状态（2026-09-06）

以下原始审查正文保留当时的证据与未修改声明；不代表这些问题现在仍未处理。用户随后授权按三批修复，实施范围为 R1–R6，Internship 功能缺口暂不处理。

- 第一批：执行权、取消/租约恢复、材料不可变版本与一致发布。
- 第二批：Gmail 预览审核与草稿恢复、任务画像快照及 CV 上传一致性。
- 第三批：有效材料状态回算、关闭机会拦截、前端刷新与 Postdoc URL 去重兼容。
- 完整 Rust 测试 110 项、前端测试 44 项、TypeScript 检查及 `git diff --check` 均通过；真实 Gmail/模型接口及桌面点击尚未验证。
- `npm run desktop:build -- --bundles app` 构建成功（release 编译约 2 分钟），产物 `src-tauri/target/release/bundle/macos/CareerOS.app`，主程序时间为 2026-09-06 01:54:54 HKT，arm64。`codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/CareerOS.app` 通过（valid on disk / satisfies its Designated Requirement）。这是本地 ad-hoc 签名，未做 Apple 公证；保留 5 条未使用符号的构建警告。未安装覆盖或启动 App，不能将静态构建检查等同于 GUI 验收。
- 本轮没有新增迁移或依赖，没有改写真实数据库、删除历史文件、提交或发布。11 个实现文件的职责、验证命令和剩余边界见 [Postdoc pipeline handoff](../features/postdoc-pipeline.md#three-batch-hardening--2026-09-06)。既有工作树改动保留。

## 结论与边界

不是“整个项目都不能用”，也不只是几个按钮问题。主要问题是：**同一条保护规则只覆盖部分写入入口，任务、文件、数据库与界面之间缺少一致的提交条件。** 单独成功的操作，组合到取消、重启、重试、并发编辑和网络失败时，会出现不一致。

本次把上一轮已报告但未修复的问题合并到下列 6 类根因；不把已修复的旧问题重复计入。没有修改实现、迁移或真实数据，没有调用付费 API、操作 Gmail、启动应用或构建安装包。仓库内仅新增本审查报告，保留原有所有未提交改动。

证据等级：

- **隔离复现**：提取当前源码中的真实 SQL / helper，在内存 SQLite、临时文件上构造故障时序；证明该边界缺少保护，不代表用户真实数据库已经发生损坏。
- **代码路径确认**：已核对调用链和异常退出点，但未执行完整桌面交互或真实网络故障。
- **功能缺口**：实现有意未接通，不等同于随机运行错误。

P1 表示建议在下一次稳定版交付前修复的数据一致性 / 审核边界风险；P2 表示有明确触发条件的功能正确性问题。不据此声称漏洞已被实际利用或数据已经外泄。

## 根因清单

| 编号 | 根因 | 典型用户表现 | 优先级 |
|---|---|---|---|
| R1 | 任务取消、恢复、完成没有共用执行权校验 | 点取消后仍保存；重启后永久排队；旧执行器结束新执行器的任务 | P1 |
| R2 | 材料发布有多套路径，文件写入早于数据库成功 | 保存失败但内容已变；文本与 PDF 不同版；历史备份被覆盖 | P1 |
| R3 | “材料是否齐全 / 机会是否关闭”使用不同状态来源 | 明明已齐又变待补；已关闭仍生成；补好 PDF 后界面仍显示缺失 | P2 |
| R4 | 画像版本与校验配置没有贯穿整个生命周期 | 老任务按新规则校验；上传失败但有效画像已被替换 | P2 |
| R5 | Gmail 审核检查和实际上传不是同一份固定数据 | 起草附件可能不是刚批准的版本；网络失败后重复建草稿 | P1 |
| R6 | 去重规范化修改了来源标识的含义 | 不同岗位 URL 被当成同一机会，沿用错误联系人或材料 | P2 |

### R1 — 任务生命周期没有完整的执行权检查

1. **取消期间仍能成功结束。** [执行外层][job-execute]只统一检查超时与租约；取消主要在模型等待阶段检查。PDF 生成、导入等本地步骤没有相同入口保护。[完成 SQL][job-finish]只判断 running，不判断 cancel_requested。隔离复现：running + cancel_requested=1 仍被写成 needs_review。
2. **取消后重启变成不可领取的队列项。** [启动恢复][job-recovery]把 running 改成 queued，却保留 cancel_requested=1；[领取 SQL][job-claim]只领取 flag=0 的任务。隔离复现：任务显示排队，但不会再被执行。这还会影响相同任务的去重。
3. **过期执行器可以结束新执行器正在跑的同一任务。** 完成 SQL 不核对 lease_owner / attempt；丢失租约的旧执行器会走错误结束路径。隔离复现：记录已归 worker-new，旧执行器的 finish 仍将其改为 failed 并清空租约。启动恢复不区分有效租约与中断任务，也扩大了多实例场景的风险；本轮未实测双开应用。

建议：利用已有 job ID、attempt、lease 字段明确本次执行身份，在领取、恢复、业务结果发布和最终结束时统一校验。取消应阻止后续发布，保留此前已经成功保存的发现；不能只把最后一个状态文字改成“取消”。

验收：取消发生在模型等待 / 渲染 / 发布前；取消后立刻重启；失联回收后旧执行器晚到。任何旧执行器不得更改新执行器状态或发布材料。

### R2 — 文件与修订记录不是同一次安全发布

搜索材料包和回复历史已经使用独立版本目录；下列其他入口仍不遵守同一原则。

1. [通用手动 / Agent 修订][material-write]先覆盖 live 文件，再插入修订、偏好和审核状态等数据库记录。后续 SQL 或提交失败时，函数报错却不恢复已改文件。CV 包装层只有拿到成功的 RevisionResult 后才进入后续渲染回滚；不能覆盖 apply_revision 自身写文件后报错的分支。**代码路径确认。**
2. [直接生成 CV][cv-write]先替换 PDF，再初始化结构基线并提交数据库；[Cover Letter][letter-write]甚至在编译前就写入当前正文 / 数据 / 模板。编译或持久化失败可能留下新文本与旧 PDF，或新 PDF 与旧修订记录。**代码路径确认。**
3. 通用修订、直接 CV 和 Cover Letter 的备份名仅精确到秒。同一材料一秒内两次保存使用同一备份路径，后一次 fs::copy 会覆盖前一次历史。固定的临时文件名也无法隔离并发通用修订。**代码路径确认；未做真实材料覆盖实验。**

建议：复用项目已有“唯一版本目录 → 校验完整 → 数据库切换当前指针”的发布方式。失败只留下未发布的诊断版本，不改当前版；备份 / 临时文件用唯一标识。基准内容校验需要与最终发布一致，不能仅在读文件时检查一次。

验收：模拟编译失败、数据库事务失败、同秒连续保存、两个写入者交错运行。当前正文 / PDF / 审核 / 修订记录必须同版，所有旧历史仍可读取。不是只测“预检失败时没动文件”。

### R3 — 业务状态没有统一从最终有效数据计算

1. **晚到的失败会降级已成功材料。** 不同检索可以涉及同一联系人。某任务成功安装材料后，另一个任务检测到 CV 已改变并失败，[错误处理][material-fail]仍无条件写 pending。隔离复现：ready 被旧错误 SQL 改成 pending；完整并发渲染时序尚未跑桌面实测。
2. **保存判定与生成判定使用不同的截止日期。** [数据库更新][merged-gate]会保留旧的有效截止日期，再据此判定 closed；[生成入口][material-gate]检查的却是未合并的新结果。旧截止日期已过期、新结果漏填日期且写 open 时，可以出现数据库已关闭、仍进入材料生成的情况。代码路径确认；此前已用真实 availability helper 的隔离输入验证两种结果不同。
3. **手动补齐最后一个 PDF 不更新完整性，也不刷新详情。** [CV 成功持久化][cv-insert]没有重新计算 material_status；[已发现筛选][discovery-filter]依赖这个持久化值。前端 [CV 生成按钮][cv-refresh]只更新局部预览，不重新加载 detail，而预览显示仍由旧 pdf.exists 控制。缺 PDF 的记录生成成功后，可能仍停留在“已发现 / 材料待补”，且预览被旧状态挡住。代码路径确认。

建议：明确“当前有效机会记录”和“当前已发布材料包”是唯一判定依据。成功、失败、手动补齐、Agent 补齐都在各自提交边界调用同一套完整性判定；失败记录属于那次尝试，不直接覆盖较新的有效状态。前端成功后刷新详情，不能仅更新提示文字。

验收：A 失败晚于 B 成功；旧过期日期 + 新结果漏填日期；仅缺 PDF 时手动补齐并保持在当前页面。机会、联系人、材料、投递标记仍是不同维度，不因为修状态而互相强制转换。

### R4 — 画像快照与实际校验未完全一致

1. Agent 修订的[输入使用任务快照][revision-snapshot]，但[输出预检与最终生成][revision-policy]仍传入全局 paths，加载当前画像 / 定制设置。若任务运行中改变页数、语言、结构或源 CV，旧任务可能按新的规则被拒绝或渲染。这与搜索路径已采用快照渲染的行为不一致。**代码路径确认。**
2. [上传 CV 的最终发布][profile-publish]先替换 master_profile，再读取并保存 onboarding。若旧 onboarding 无法解析，或其保存失败，上传报告失败，但 master_profile 已经变成新版本。现有“坏 PDF 不替换画像”测试只覆盖更早的提取失败，未覆盖这个提交失败点。**代码路径确认。**

建议：输入画像版本、用户定制策略与输出校验保持绑定；老任务继续用自己的快照，新任务使用新画像。上传先完成所有验证与暂存，再一致地发布有效画像指针；失败恢复之前的有效状态，原文件继续保留。

验收：任务进行中更换 CV / 页数规则；同一任务重试；上传最终保存失败。界面选择、任务快照与当前有效画像均应可明确解释，不能悄悄混用。

### R5 — Gmail 附件审核与远端草稿创建存在时间窗口

1. [读取批准路径并校验 hash][gmail-approval]之后，代码等待 token / Gmail 账号请求，再由 [build_mime][gmail-mime]重新读取该路径。若等待期间同路径 CV 被重新生成，上传的是新文件，而审核针对旧文件。隔离复现：校验版本 A 后替换为 B，使用真实 MIME helper 得到的附件是 B。**只在临时文件复现，未连接 Gmail；这意味着未审核内容可能进入远端草稿，不意味着已自动发给收件人。**
2. [创建后核验流程][gmail-verify]在远端 POST 成功后先 GET 核验，最后才保存本地草稿记录。GET 失败或后续本地提交失败时，远端已有草稿，但本地没有可恢复的记录；再次点击创建会再 POST，产生重复草稿。**代码路径确认；未调用真实 API。**

建议：审核与 MIME 构造使用同一份捕获的附件字节，并绑定当前批准版本；版本变化要求重新审核。拿到远端 draft ID 就进入可恢复记录，核验失败显示“已创建，待核验”并重查该 ID。若 POST 结果本身不确定，提示核对，不能假定未创建并自动重建。保留“不自动发送”的边界。

验收：校验后替换附件；POST 成功但 GET 失败；保存本地记录失败；重复点击恢复。不得上传未批准的替换字节，不得把远端成功误报为“未创建”。

### R6 — URL 去重误伤大小写敏感的岗位标识

[canonical_url][url]对整个 URL 使用 to_lowercase，包括 path 和 query 值。隔离复现：同一机构下 /jobs/AbC?token=A 与 /jobs/abc?token=a 得到相同规范值。服务端可以把这些 URL 识别为不同记录；缺少独立官方岗位 ID 时，现有匹配可能错误合并。此前修好的“冲突官方 ID 不合并”不能覆盖这个没有 ID 的情况。

建议：仅规范协议、主机及已明确无意义的追踪参数；保留可能区分职位的路径 / 参数值。不能把新规则直接用于批量重写旧身份键或自动拆分历史机会；旧记录冲突需明确兼容和人工确认。

验收：host 大小写、追踪参数可归一；path / 参数值大小写不同不可无条件合并；并保留现有“共享 URL + 不同官方 ID”的回归测试。

## 通用用户 / Internship：单列功能缺口

这不是要求把 Internship 并入 Postdoc，而是同一个用户应有一致的画像基础、独立的目标与申请流程。

- [Internship 工作区][intern-profile]仍明确跳过已上传 CV 的复制，注释假设它属于“原来的 Postdoc 用户”。[策略页面][intern-panel]的偏好与贡献卡还是静态待设置内容，没有真正的画像编辑 / 绑定。因此“上传一个 PDF 即可按本人背景搜索”当前只在 Postdoc 主链路接通；Internship 主要依赖本次搜索文字。**功能缺口，不声称它误用了别人的 CV。**
- [Internship 空结果][intern-empty]仍直接报失败；Postdoc 已把 no_matches 处理为正常可审核结果。**代码路径确认：不是模型故障，应区分无匹配、来源待核验和执行失败。**

建议后续单独确定“同一用户共享 CV、不同求职方向分开偏好”的产品规则，不引入多账户 / 多人画像框架；用户仍只上传一次 CV，目标按轨道独立。若暂不支持，界面应明确说明，不能用静态占位让用户误以为已按简历匹配。本轮不扩展实现范围。

## 已有保护与未确认事项

现有测试继续通过的保护包括：

- Postdoc 无匹配不再等于执行失败；缺材料仍保存发现。
- 同一机会下不同联系人状态独立，手动搁置与投递标记不混用。
- 已发现 / 全部的筛选与分类在分页前处理；仪表盘按求职轨道分开。
- 搜索材料包采用版本目录；已有手动内容不被普通补齐覆盖。
- 回复结果保留历史，旧回复 / 旧任务不能覆盖新回复结果；用户较新状态有版本保护。
- 有冲突的官方岗位 ID 不再仅因 URL 相同而合并；陈旧核验不能覆盖较新联系人资料。
- 相同机会的 continuation 在入队和重试均去重，保留原线程的边界已有测试。
- 推荐人来自源 CV 或用户定制；不默认要求所有用户提供三位。

以下未当成确定缺陷：同名联系人是否一定应拆分、改变服务商后是否能跨服务商恢复模型会话、真实模型的搜索质量、真实 Gmail 网络错误频率、Windows GUI / OCR 表现。没有证据证明当前真实数据已经被上述竞争条件破坏。

## 集中修复与验收建议

不需要重写应用或新增框架。按明确的跨边界切片收口，每批先补能失败的回归测试，再实现，并复跑前面批次的验收：

1. **执行与发布安全**：R1 + R2，先统一执行身份检查、不可变版本发布和唯一历史标识。涉及调度和多条材料写入链路，不伪装成 1–3 文件小修改。
2. **Gmail 审核边界**：R5，可以独立修复；保持无自动发送。先做可注入的本地故障测试，不用真实邮件验证数据安全。
3. **最终状态与用户输入一致性**：R3 + R4，状态从已提交的有效数据计算，输入 / 校验使用同一版本，前端刷新与之配套。
4. **去重兼容与通用用户补齐**：R6 加旧数据样本测试；Internship 产品缺口单列实施，不顺手改 Postdoc 流程。

交付标准：这些失败组合必须有测试；测试通过后再本机编译并验证实际操作。不能把“typecheck 通过”“构建成功”替代“功能成功”，也不能只增加错误提示而不修复写入规则。

## 本轮实际验证

- npm run typecheck：通过。
- npm test -- src/pages/ApplicationDetailPage.test.ts src/pages/ApplicationsPage.test.tsx src/pages/DashboardPage.test.tsx src/pages/AutomationPage.test.ts src/pages/OnboardingPage.test.tsx：5 文件，39 测试通过。以纯函数 / 服务端静态渲染测试为主，不等于完整点击交互验证。
- 使用外部 CARGO_TARGET_DIR=/private/tmp/careeros-search-repair-check，分别运行 cargo test --offline --manifest-path src-tauri/Cargo.toml <module>::tests --lib -- --skip imports_real_text_pdf_and_image_only_scanned_pdf_locally：
  - workflows 22、scheduler 19、db 6、materials 7、typst 8、cv_schema 11、gmail 2、onboarding 4；合计 79 通过。
  - 未运行完整 101 项测试集合；本轮跳过真实文字 / 扫描 PDF OCR 集成测试。存在原有 compact_thread 未使用编译警告。
- 临时离线探针：6 项全部复现缺陷（取消完成、取消重启、旧租约完成、材料降级、URL 大小写、MIME 附件替换）。直接提取当前源码 SQL / helper，不复制一份假业务实现；没有修改仓库实现以让它失败。
- 探针入口：[probe.mjs](/private/tmp/careeros-consolidated-audit.kmUOGq/probe.mjs)。使用临时合成附件与内存数据库；临时目录被系统清理后需重建，未来正式修复应把这些场景加入仓库测试。
- 没有写入项目 target/debug，没有读写真实 Application Support 数据，没有在线 API 调用，没有运行桌面 UI 或安装 / 发布。
- 范围审计：计划 0 个实现文件；实际 0 个实现文件，新增本报告 1 个文档。所有原有脏文件保留。

## 源码定位

行号对应本轮审查时的未提交工作树，后续修改可能移动。报告是问题清单和验收建议，不是已完成修复说明。

[job-execute]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/scheduler.rs:450>
[job-recovery]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/scheduler.rs:1169>
[job-claim]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/scheduler.rs:1236>
[job-finish]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/scheduler.rs:1394>
[material-write]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/materials.rs:707>
[cv-write]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/typst.rs:262>
[letter-write]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/cover_letter.rs:134>
[material-fail]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/workflows.rs:700>
[material-gate]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/workflows.rs:637>
[merged-gate]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/workflows.rs:1254>
[discovery-filter]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/db.rs:307>
[cv-refresh]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src/pages/ApplicationDetailPage.tsx:266>
[cv-insert]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/typst.rs:294>
[revision-snapshot]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/materials.rs:308>
[revision-policy]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/materials.rs:608>
[profile-publish]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/onboarding.rs:181>
[gmail-approval]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/gmail.rs:219>
[gmail-mime]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/gmail.rs:383>
[gmail-verify]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/gmail.rs:247>
[url]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/workflows.rs:1722>
[intern-profile]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/materials.rs:364>
[intern-panel]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src/components/InternshipPlanningPanel.tsx:35>
[intern-empty]: </Users/urbino/Library/CloudStorage/OneDrive-Personal/1urbino_Dr/2研-博/18 留学/000赴港材料准备/个人信息/A Customised CurVe CV/postdoc-os-desktop/src-tauri/src/workflows.rs:571>
