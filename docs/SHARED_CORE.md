# 共用核心（myriad-core-ffi）

本仓库 2026-10-26 带着历史从 [Myriad](https://github.com/Myriad-You/Myriad) 拆出来（原来在它的 `crates/` 和 `shared/` 下，这份说明原来是它的 `docs/development/SHARED_CORE.md`），各端按一个提交钉住本仓库。

站点以外的平台（目前是 macOS 原生 App）要和站点上的她说同样的话、按同样的规则变心情，所以不各自抄一份规则，而是链接同一份 Rust 代码。`crates/myriad-core-ffi` 把纯规则 crate 包成一个静态库，对外只有一个 JSON 进、JSON 出的 C 入口。

站点后端不经过它，照常直接依赖 `myriad-merope`、`myriad-agent-rules` 等 crate（Myriad 用 git 子模块钉住本仓库）。

## 边界

- **只收纯 crate。** 现在是 `myriad-merope` 和它依赖的 `myriad-agent-rules`，以及同步她的数据用的 `myriad-persona-sync`（格式、规范字节、时钟、比对）和多端的日志规则 `myriad-persona-log`（信封、版本向量、合并、遗忘、折叠、执笔权、时钟），「她的连接」帧的规则 `myriad-her-link`，以及客户端看屏幕时的宿主过滤 `myriad-perception-filter`，她的节点 `myriad-nodes`。不引入 tokio、reqwest、sea-orm，也不读写文件或网络。
- **每个调用都是输入的纯函数。** 现在几点、她的时区、心情这些状态都在输入里；库不读进程的时钟、时区和环境。crate 里的规则因此不用 `chrono::Local`：她的时区由调用方传入，后端传 `merope::clock::zone()`，现在就是进程时区（容器里 `TZ=Asia/Shanghai`）。
- **一个二进制里只能有一个 Rust 静态库。** 两个 Rust staticlib 会在 std 符号上冲突。别的平台将来要用的纯 crate，都并进这一个库，不另建。
- **密钥不进来。** 规则拿不到任何宿主密钥或凭据，错误信息里也没有。

## ABI

头文件：`crates/myriad-core-ffi/include/myriad_core.h`。

```c
MyriadCoreBuf myriad_core_call(const char *name, const uint8_t *input, size_t len);
void myriad_core_buf_free(MyriadCoreBuf buf);
```

| `status` | 含义 | 缓冲区内容 |
| --- | --- | --- |
| 0 | 成功 | 调用的输出 JSON |
| 1 | 没有这个调用 | `{"error":{"kind":"unknown_call","message":…}}` |
| 2 | 输入不对（不是 JSON、缺字段、多字段、类型不对、名字为空或不是 UTF-8） | `{"error":{"kind":"bad_input",…}}` |
| 3 | 调用里 panic（库内捕获，不跨 FFI 展开） | `{"error":{"kind":"panic",…}}` |

- 返回的缓冲区归调用方，用完交给 `myriad_core_buf_free` 一次。`ptr` 为空的缓冲区可以安全地交回。
- `input` 为空指针且 `len` 为 0 时按 `{}` 读。
- 字段名 camelCase；输入拒绝未知字段，拼错不会被静默忽略。
- 时间是 Unix 微秒（`i64`）。她的时区是 IANA 名（`Asia/Shanghai`）或固定偏移（`+08:00`）。用名字，是因为夏令时会挪动一天的边界，而且各平台要拿同一个名字。
- 浮点按位一致：`serde_json` 开了 `float_roundtrip`，平台侧用最短往返表示编码即可。

现在是 `ABI_VERSION` 2（2026-10-31 起：`nodes.fold`、`nodes.wire_send` 的 `ledger` 必填，并入、拆出的行必须在信封里带 `to`（拆出还要 `takes`），没有的拒收；`log.admit`、`log.answer` 的 `holder` 必填；钉第 1 版的端照 `crates/conformance/v1/nodes`、`v1/log` 对照）。

`meta.version` 返回 `abi`（`ABI_VERSION`）、`upstreamCommit`（构建时的 `MYRIAD_CORE_UPSTREAM_COMMIT`，没设为 `unknown`）和全部调用名。调用表在 `src/calls.rs` 的 `CALLS`。

## 加一个调用

0. 规则还在站点后端（Myriad 的 `backend/`）的，先搬进纯 crate（只挪不改，后端改为 re-export 或薄包装，现有测试证明行为不变），再登记调用。回想的算术（`myriad_agent_rules::memory`）、聊天日记的门槛（`myriad_merope::diary`）、她的作息（`timing::*_at`）、聊天记录的文本（`chat_history`）就是这样进来的。
1. 在 `src/calls/` 里它所属领域的模块（心情、说话、她自己的生活、作息、体征、说话习惯、海龟汤、连载、先开口、换装……）写调用函数，再按名字的字典序登记进 `src/calls.rs` 的 `CALLS`。函数只做 JSON 与 crate 类型之间的转换，规则本身留在原 crate。
2. 在 `src/tests.rs` 或 `src/tests/` 的领域模块里写一组往返测试：经 C 入口的输出要和直接调 crate 函数逐字节（浮点逐位）相同。再把名字登记进该模块的 `TESTED`（或 `every_call_is_tested_and_version_lists_them` 的列表），漏登记会失败。
   - 规则本身的输出要确定：同一输入在任何平台、任何一次运行都得出同一结果。按 `HashMap` 的遍历顺序决定先后的地方要补一个确定的次序（例如 `contrast::overused` 的同分按片段本身排）。
3. 改了已有调用的输入或输出形状、或删掉调用，要加 `ABI_VERSION`。只是新增调用或可选字段则不必。
4. `cargo test -p myriad-core-ffi`，`cargo clippy -p myriad-core-ffi --all-targets -- -D warnings`。

## 同步（`sync.*`）

她的数据跨端时的格式只有 `myriad-persona-sync` 里的 Rust 类型这一份定义。平台把自己的记录拼成粗实体交给 `sync.canonicalize`，拿回实体和它的规范字节；存的、发的、比对的都是这份字节。`crates/myriad-persona-sync/fixtures/` 是黄金文件：每类实体一份、一份完整的信封、一组比对（`compare/local.json`、`remote.json` → `comparison.json`）、一个 rig 包（atlas 字节是 `atlas-pixels`）。别的平台按钉住的提交复制这些文件，经 C 入口逐字节比对。故意改格式时用 `MYRIAD_SYNC_BLESS=1 cargo test -p myriad-persona-sync` 重写它们。

| 调用 | 输入 → 输出 |
| --- | --- |
| `sync.affect_fold` | `{anchor, events:[变化], upto}` → `{anchor}`：把 `upto`（含）以前的变化折进锚点，锚点停在最后一条之后并记下它（`throughEvent`）；只有站点写锚点 |
| `sync.affect_replay` | `{anchor, events:[变化], now, baselineNow}` → `{affect}`：她对站长现在的心情 |
| `sync.archive_key` | `{passphrase, kdf}` → `{key}`：按文件头里的 argon2id 代价从口令（NFC）派生密钥 |
| `sync.archive_open` | `{start}`（文件开头，base64）→ `{header, offset}`；或 `{key, header, index, last, data}`（一块密文）→ `{data}`：除最后一块外每块恰好 `chunkBytes` + 16 字节 |
| `sync.archive_part` | `{part}` → `{head}`；或 `{bytes}` → `{part, headBytes}`（不够一个头时 `{part: null}`） |
| `sync.archive_seal` | `{salt, noncePrefix, fields}`（16、19 字节随机数，base64）→ `{header, line}`；或 `{key, header, index, last, data}`（一块明文）→ `{sealed}` |
| `sync.canonicalize` | `{entities:[粗实体]}` → `{entities:[{entity, canonical}]}`；`digest` 可留空，写了就必须对 |
| `sync.check_rev` | `{rev, now}` → `{accepted, aheadMs, maxAheadMs}`：`now` 是站点的时间，`rev` 超前它 5 分钟以上就不收 |
| `sync.compare` | `{local:[实体], remote:[实体]}` → 比对结果（各字段、各类的相同／只在一端／两端不同，`sameName`、`needsSubject`，`unreadablePictures` 两端各自读不到的图） |
| `sync.is_empty` | `{persona, memories, diary, affectEvents}` → `{empty}` |
| `sync.hlc_next` | `{last?, now, node}` → `{rev}` |
| `sync.hlc_observe` | `{last?, seen, now, node}` → `{last}` |
| `sync.legacy_rev` | `{updatedAt, origin?}` → `{rev}`（没有修订的旧行现算） |
| `sync.manifest` | `{envelopes, logs?, holder?}` → `{manifest}`：每类实体、向量、文件的条数和 digest，与读的顺序无关；给了日志页（`logs`）时多一项 `log`：每一行按 id 一条，连同它的载荷，没有日志页时清单和以前逐字节相同。有日志页就必须给 `holder`（这份文件给谁，伙伴的文件是 `{pc, owner}`），它不能存的受众的页拒收 |
| `sync.persona_diff` | `{before, after, revs, legacyRev, rev, origin, reason?}` → `{changed, revs, history}`：人设一次写入逐字段比（字段没了算 `null`），变了的字段记成这次的 `{rev, origin}`，被换掉的旧值连同它原来的 rev 进 `history`（原来就没值的不进）；没有 rev 的旧字段按 `legacyRev`、`site` 算；`reason` 默认 `edit` |
| `sync.rig_package_id` | `{atlas: base64, manifest}` → `{id}` |

格式里几条平台要照做的规矩：

- **第 2 版的几类只在搬家和导出时出现。** `conversation`（站长的一场私聊连同每一句，含「没听完」标记和句子里的站点图片）、`proactive_line`、`intention`、`owner_state`、`pace`、`present`、`wearing`、`field_history` 是第 2 版加的。首次比对的 pull 只发前 13 类（`Kind::COMPARED`），`sync.compare` 也只读它们。
- **导出文件 `myriad.her-archive`。** 第一行是明文头（规范 JSON），之后是分块的 XChaCha20-Poly1305：nonce 为头里的 19 字节前缀 + 块序号（u32 大端）+ 最后一块标记，头这一行是每块的附加数据。截断、换块、改头都打不开，口令错和文件坏说的是同一句话。流里依次是信封页、日志页（只在第 2 版）、它们引用的文件（rig 是 `manifest.json` 加 `atlas.png`）、清单。导出文件总是加密。盐和 nonce 前缀由调用方从系统随机源取来传入，核心不读随机数。黄金文件在 `fixtures/archive/`。
- **文件头里的伙伴（第 2 版文件）。** 有日志页的文件头可以带 `partner`：写文件那一端的伙伴 id（字符串，`sync.archive_seal` 的 `fields.partner`；只有第 2 版能带，第 1 版头里有它就拒绝）。导入的一端先读头，用它问 `partner.file`：是这一端的 id 就收；这一端空着、从没连过，就认它；头里没有 `partner` 的旧文件只有这样的一端能照原样收；别的一律拒绝。收的时候 `nodes.wire_take` 的 `holder.owner` 就是回答的 `id`。它是 id，不是凭据，谁看过头都能抄：头里的 id 只防拿错（错的文件、错的 Mac），不认证任何人，所以文件从不改变或增加一端的 id。
- **日志页（第 2 版文件）。** 她的节点和节点日志进导出文件，用的是端和端之间同样的一批：一个受众的行和它们的载荷 `{audience, rows, payloads?}`（`nodes.wire_send` 的一批，`log.rows` 帧的正文），规范 JSON，段的头是 `{"part": "log", "bytes": n}`。有日志页的文件头写 `version: 2`（`sync.archive_seal` 的 `fields` 给 `logs: true`）；没有的照旧写第 1 版，只读第 1 版的一端照样打开。这一层只查页的外形（规范 JSON、同一个受众、整份文件里每行 id 只出现一次、每个载荷都有本页的行指向它）；写第 1 版头的文件里放日志页，封装时就拒绝（`Sealer::part`），读的一端也照样拒收。写文件时清单按读文件的一端（`holder`）查每页的受众，所以给伙伴的文件里不会有群的日志或别人的私聊；页里的行和 body 应当就是给这一端的 `nodes.wire_send` 的输出。导入的一端把每一页当作从文件来源那一端收到的一批，经 `nodes.wire_take` 做所有检查（受众、桩、载荷），再合并。所以站点导入时不用再拒收节点，Mac 专属附件里的节点和节点日志可以搬进来（见「她的节点」›「在端之间」）。家留着的节点在日志页里只有桩，它的整行另放在家页里（`{"part": "home"}`，只给伙伴的电脑，见「她的日志」›「家自己的备份」）。

- **不透明的人设键。** 人设里本端不认识的键（站点的 `displayName`、`draftSource`，以及以后别的端加的键）是 `opaque` 类：原样存下，原样传回（值和 `rev` 都不动），按单值合并。比对不列它们：不用这个键的一端谈不上和别人不同。
- **读不到的图。** 立绘、贴纸、贴纸头像的位置上要么是图片资产，要么是 `{"unreadable": true}`：这一端有这张图，但读不出来（外链失效、文件没了）。没有可拉的文件，界面照比对里的 `unreadablePictures` 提示「站点上有张图读不到」。站点上外链的图（`http`/`https`）在第一次被设备读到时，经出站守卫下载、存进站点的媒体库，她的人设、衣柜立绘、贴纸头像和贴纸改指向这份副本；之后外链失效也不影响。下载失败的地址 10 分钟后再试，这期间带标记。rig 以某个地址为母图时，那个地址保留不改，图从副本读。图按字节的 sha256 跨端，换地址不改变跨端的任何内容。
- **人设的字段修订与历史。** 每个人设字段各有自己的 `{rev, origin}`：站点存在 `agent_persona.field_revs`，每次经站点编辑人设都用 `sync.persona_diff` 比一遍，变了的字段拿到这次的 rev，被换掉的值进 `agent_persona_field_history`（每个字段最多留 50 条），站长可以在「人格」各项、名字、时区与外观设定（`visual.*`）旁边的「历史」里看并换回，换回也只是一次新的编辑。pull 的 persona 里每个字段的 `rev` 是它自己的（没编辑过的按行的 `updated_at` 现算），被清空的字段以 `null` 带着它的 rev 下行，整个 persona 的 `rev` 是其中最新的。贴纸头像按图片字节算，换地址不算编辑，仍用行的 rev、不进历史。
- **她的时区。** 人设字段 `zone` 是 IANA 名（`Asia/Shanghai`），或空（`null`）：没设时各端用自己的时区（站点是进程的 `TZ`）。规范化时去掉首尾空白，空串记为 `null`。站点存在 `agent_persona.zone`，核心调用里的 `zone` 参数就该传它（没设时传本机时区）。
- **心情按变化重放。** 每次心情变化记一条 `affect_event`：`at`（这次写入盖的时刻）、`baseline`（这次之前她往哪里回落）、`impulse`（`{"appraisal":{emotion, arousal, scale}}`、`{"music":{listenedSeconds}}` 或 `"none"`：只是又过了一会儿）。重放：从锚点出发，按 `(at, id)` 逐条先按整秒数回落到这一条的 `at`，再施加冲量，四个值夹到 0–100；最后回落到 `now`。锚点及以前的变化已经在锚点里，不再施加。变化可以只给 `{id, at, baseline, impulse}`，事件 body 加上 id 也行（别的字段不读）。站点记下的变化与站点自己读出的心情逐位相同。站点每天折一次：站长的变化留 14 天、别人的 30 天，更早的折进锚点后删掉；这是站点的维护，她住在设备上（主机是设备）时也照常进行。收到比锚点还早的变化（在它的时刻被折进锚点之后才到）时，收的一端把 `at` 改成锚点的时刻并标 `late`，再保存。
- **取代边。** 记忆被别的记忆取代（`superseded`，以及 `changed_mind`、`forgiven` 等换成新行的退役）时，旧行的 `supersededBy` 写新行的 id，与退役同一次写入；多条旧行可以指向同一条新行，新内容已经有别的行在说时指向那一行，只是收回、没有新行时为空。`faded` 等没有新行的退役不写边。
- **没有停用时间的旧记忆（R9）。** 一端有些记忆只记了「已停用」、没记什么时候：上线时按退役处理，`invalidAt` 取这条记录最后一次写入的时间，`invalidReason` 可空。格式照常接受（`retired` 当且仅当有 `invalidAt`，不要求晚于 `validFrom` 或 `createdAt`）；这条换算由那一端在拼粗实体时做，核心不猜。
- **在穿哪一套各端各自。** `visual.activeOutfitId` 与 `visual.activeFullBodyOutfitId` 不是人设字段（Q12、R6）：站点不发，别的端也不要发；衣柜本身照常同步。
- **资产按内容 id 比对。** `sync.compare` 只看 rig、立绘、贴纸、贴纸头像的内容 id（rig 的包 id、图片字节的 sha256）；两端写的 `bytes`、`mediaType` 不同不算不同（R8）。
- **体征不带群。** `vitals_day` 的 `groups` 只有每个群的计数，规范化时去掉群的标识并按计数排序。
- **时钟超前。** 站点不收超前它 5 分钟以上的 `rev`，回 409 `clock_ahead`（带 `serverTime`、`aheadMs`、`maxAheadMs`）。设备的请求头 `X-Sync-Clock` 带「下一次写会拿到的 rev」（`sync.hlc_next` 算出、不落盘），`hello` 与 `role` 必带；以后的 push 里每个实体和每个人设字段的 `rev` 都按同一规则查，有一个超前就整批不收，什么都不写、也不吸收进站点的时钟。设备收到 `clock_ahead`：停止同步并告诉用户时钟不对，outbox 留着（暂存）；时钟改对后，把时钟退回到最近一次从站点看到的 rev，按 outbox 顺序用 `sync.hlc_next` 给每条待推的写重新打 rev，再推。推之前可以用 `sync.check_rev` 按 `hello` 给的 `serverTime` 加上经过的时间自查。
- **站点自己的时钟。** 站点只有一个时钟：宿主机时间加上量出的可信偏差（后端 `crate::clock`；SQL 里的时间也由它作参数传入）。每个进程（web、persona、federation）启动时立刻量一次宿主机时钟偏了多少，失败了一分钟内重试并逐步拉长，之后每 15 分钟一次；SNTP，UDP 123 不通时用 HTTPS 的 `Date`（有出站代理时经代理）。可信（HTTPS，或至少两个 SNTP 服务器相差不到 1 秒）、不超过 45 分钟、偏差不超过 3 小时时就按它校准，否则用宿主机时间并在日志里写明原因。校准后的时间只增不减：偏差变小或校准结束时慢下来追平，不往回跳。`hello`、`clock_ahead` 和站长的设备列表都带 `serverClock`：`{checkedAt, offsetMs, source, trusted, thresholdMs, maxCorrectionMs, wrong, corrected}`，检查关掉或还没跑过时是 `null`。`offsetMs` 是宿主机时钟减去真实时间（毫秒，负数是宿主机慢），什么都没答上时为 `null`（无法检查，不算偏差）；`checkedAt` 是站点时钟、自 epoch 的毫秒；`source` 是 `sntp`、`https` 或 `null`；`wrong` 是宿主机偏差超过 `thresholdMs`；`corrected` 是站点时钟现在按这次测量校准。超前检查、`hello` 和 `clock_ahead` 的 `serverTime` 都是站点时钟；`serverTimeSource` 说它来自哪：`clock`（未校准，即宿主机时间）、`sntp` 或 `https`。上限 5 分钟不变，真超前的设备照样被拒。设备据此分清是哪一端不准：没有 `corrected` 而且 `wrong` 时是站点的时钟不对，设备不要去改自己的。

## 她的日志（`log.*`、`lease.*`）

多端的做法：每个主机核心（每一台电脑、服务端）各存一份她的只追加日志，任何时刻只有一个她在想。各端交换版本向量、互发对方缺的行，收到的先检查、再取并集；她现在是什么样，由同一套规则从日志折叠出来。规则全在 `crates/myriad-persona-log`。它只用 `myriad-persona-sync` 的时钟（`hlc`）和规范字节，不并进那个 crate：快照同步和第一次见面的合并是另一套模型，以后只留给第一次见面，分开放，哪天删掉它不牵连日志。

一行的信封（`Row`，规范 JSON，字段名 camelCase）：

| 字段 | 意思 |
| --- | --- |
| `version` | 行的格式版本，现在是 1；读得懂更旧的，更新的拒收 |
| `id` | 全局唯一，去重按它 |
| `origin` | 写它的那一端，就是时钟的节点名：`site`、`dev_…`、`loc_…` |
| `author` | `owner`（主人说的、设的、批准和撤回的、让她忘掉的）或 `her`（她的想法、看法、心情、开口、整理） |
| `epoch` | 只有 `her` 的行有：写的那一端当时持有的执笔权 epoch，那一端还没见过任何执笔权时是 0；`owner` 的行没有 |
| `dseq` | 只有 `shared` 的行有：这一端在这个受众里第几条共享的行，从 1 起、不跳号。按「端 × 受众」计数：各端保存的受众不一样（群的日志只在服务端），整端一个计数会在别的端留下永远补不上的洞 |
| `hlc` | 写的时候的时钟，节点必须是 `origin` |
| `audience` | `personal:<主人>`、`own:`、`group:<群>` |
| `subject` | 关于谁；`own:` 里没有，`personal:` 里只能是其中的人 |
| `object` | 关于哪个对象：`<类型>:<id>`，例如 `node:…`、`memory:…`、`persona:<字段>` |
| `kind` | 允许清单里的一种（`kinds.rs`）：不在清单里的拒收，新种类不会悄悄出机 |
| `payload` | `{ref, key?}`：内容在哪、用哪把钥匙封的。内容本身永远不进信封 |
| `replicate` | `shared` 或 `local`：`local` 的行不占 `dseq`，永远不编给别的端 |
| `forgotten` | 桩：载荷没了，位置（`id`、`origin`、`dseq`）还在 |
| `to` | 只有并入（`node.absorbed`）和拆出（`node.split_off`）的行有：另一个节点（`node:<id>`），并进的那个或拆出来的那一块。写在信封里，桩也留着它 |
| `takes` | 只有拆出的行有：拆到 `to` 去的段（行 id，排好序、不重复，最多 64 个） |

几条各端要照做的规矩：

- **合并是并集。** 同一个 `id` 只算一次；桩赢过它原来的那一行；同一个 `id` 内容不同、或同一受众里同一端的同一个 `dseq` 被两行占了，都是格式被写坏了，留下已有的、报 `conflicts`。`log.merge` 的 `stubbed` 是本端变成桩的行：删它们的载荷。
- **收一批行**（`log.admit`）：有一行的时钟超前本端 5 分钟以上，整批不收、什么都不吸收（`clockAhead`）；其余每行单独判：信封不完整、`local`、对象换了受众、主体不在群里的拒收并计数。对象原来在哪个受众由调用方传 `known`，群成员传 `members`（不传就不查）。同一端她的行在同一个受众里，epoch 沿它自己的顺序只增不减：比本端在这个受众里已收的那一端最高 epoch 低的拒收（`epoch_back`）；已收的最高 epoch 由调用方按 `{受众: {端: epoch}}` 传 `epochs`，回答里带收下这一批之后的。按受众判，是因为各受众分开计数、分开发，一端单机时期的行可能在它另一个受众后来的行之后才到。账上的对象收成桩，版本向量照样补齐。
- **忘掉是吸收的。** `forget` 行的 `object` 是被忘掉的对象，或 `subject:<id>`（这个受众里关于这个人的全部）；它自己没有载荷，也永远不会被忘掉。账只有受众、目标和时间（`Entry`），没有一个字的内容。不管 `forget` 排在对象的行之前还是之后、从哪一端先到，被忘掉的对象都折叠成只剩 `forgotten: true`。`log.answer` 永远先发 `forget` 行。
- **撤回同步**（`retract`，伙伴 2026-10-30 按推荐默认定）。原则：撤回和忘掉一样，只是家里什么都留着。伙伴在哪一端按撤回，那一端就写两行：shared 的 `retract`（伙伴写的，对象是一个节点 `node:<id>`，没有载荷，永远不会被忘掉，让每一端都知道），和 local 的 `retract.home`（同样的对象和时钟，不占 `dseq`，永远不出机，说明这一端是家）。两行最好在同一个事务里写，都过一次 `log.ledger` 记进这一端的账。**只有自己写了 `retract.home` 的一端是家。** `local` 的行从来不会从别的端来（`log.admit` 拒收 `local`），所以别的端发来的撤回，哪怕行上写的 `origin` 是这一端（坏掉的端，或这一端丢了日志以后又收回自己写的那一行），在这里也只是忘掉。同一个节点在两端都撤回过（各自离线时按的），两端都是家，各自留着自己手里有的，没有哪个时钟决定谁是家（伙伴 2026-10-30 定：「两台都留」）。别的每一端都把撤回当作忘掉这个节点：行变成桩、body 删掉，之后再到的这个节点的行（别的端同时写的、离线回来的）也只收成桩，所以推不回去；不认识的端写的撤回也照收，在每一端都等于忘掉（安全的方向）。账里撤回是 `{kind: "retract", homes}`（`homes` 是写过撤回的各端，排好序，只作记录），家是 `{kind: "home"}`。**家从不因为桩丢 body**：`log.merge` 在家里不让桩换掉手里完整的行，反过来完整的行到了就补上手里的桩（`filled`，存它的 body），所以同样的行不管按什么顺序到家，结果都一样，家导入任何导出文件（连它自己的）也不丢东西；某一端已经删了的 body，只要还有一端或一份导出完整地留着，到了家就补上（伙伴 2026-10-30 定：「补回来」）。别的端写的、在那一端知道撤回时还没到任何一个家的那一行，那一端照忘掉的样子删掉，哪里都不留（伙伴 2026-10-30 定：「当作忘掉」）。`local` 的行只在写它的那一端，撤回从来不删它。**一条规则**：一行有忘掉够到就变成桩；有撤回够到、而没有这一端的家够到，也变成桩；否则留着（`local` 行在写它的一端也留着）。`log.admit`、`log.merge`、两个折叠、`wire_*` 都按这一条；发给别的端时哪一端都不是家，所以被撤回的节点的行从每一端发出去都只是桩，家也一样：它的 body 不离开家，连导出文件也不带。撤回是单向的，和忘掉一样；再同步回来要伙伴另写一行，核心还没有这一行。撤回以后家里这个节点的新行，写的一端应当写成 `local`；写成 shared 的也只会作为桩离开家。
- **节点的别的名字在信封里。** 一行忘掉或撤回只点名一个节点，但它也够到并进这个节点的每个节点（一路并下去），和拆到这个节点的段；被忘掉或撤回的节点自己原来的段，拆走了的也跟着走（和折叠一样）。哪些是这样，写在并入、拆出那一行的信封里（`to`、`takes`），桩也留着，账里也各有一项（`{kind: "into", to}`、`{kind: "split", to, takes}`），所以每一端都从自己的日志算出同样的结果，调用方不用另存什么，只给了部分日志的调用靠账也知道。**够到的只会变多。** 一个节点并进了两处（两端各自离线时并的），两处的忘掉、撤回、家都够到它；一段被拆了又拆、或同时拆到两处，它到过的每一处都够到它（宁可多拿）。折叠里显示哪一次并入（先写的那一次：先比时钟，再比端）只是显示，不影响够到什么。所以一行一旦变成桩，同样的账在每一端、不管行按什么顺序到，都还够到它；一个时钟写得早的并入也挡不住对另一处的撤回。**名字只从信封来**：每一行并入、拆出都必须在信封里写 `to`（拆出还要写 `takes`），没有的行哪一端都拒收（`log.canonical`、`log.admit` 的信封检查），不再有从 body 读名字的旧路子（这种行没有发出去过）。一行并入、拆出只要算数（下面的时钟规则），它的名字就照信封记，不管它的 body 是什么、到没到、读不读得通、被扣下没有；折叠对 body 的检查（`bad_body`、`not_minted`、`together`、`kind_fixed` 这些）只管显示：折叠跳过的那一行照样让忘掉、撤回够到它说的节点（宁可多拿）。**时钟规则（结构冻结）**：一行并入、拆出只有在它的时钟早于它自己的节点和 `to` 那个节点的每一次撤回时才算数，显示和够到都是；晚于或等于的（撤回以后才写的）在每一端都作废：不记名字、不够到什么，折叠跳过它（`void`）。只看时钟，所以不管行按什么顺序到，每一端都一样。`uncertain`：一次时钟早的撤回如果很晚才到，之前按那行并入已经变成桩的行不会再变回来（桩不回头），只是那以后的够到不再经过它。**上线**：一行带 `to` 或 `takes`，旧的核心就整批拒收这一批（它的信封检查不认识这两个字段）；所以每台电脑和服务端都钉到这一版以前，哪一端都不要写它们，也不要写 `retract`。
- **调用方的账。** `log.admit`、`log.merge`、`nodes.wire_take` 都带这一端的账（`ledger`），`nodes.fold`、`nodes.wire_send` 的账必填（折叠和发都只按账里的名字和撤回）；行里有撤回而没给账的调用一律拒绝（经 C 入口是 `bad_input`，直接用 crate 的 Rust 调用方拿到错误），免得家因为一次没说自己留着什么的调用把节点删了。`log.merge` 的 `log` 不是整个日志时必须给账：核心看不出给的是不是全部，这一条是调用方的约定。`log.merge` 的 `incoming` 里有 `local` 的行就整批拒绝：`local` 的行从来不会从别的端来，这一端自己写的行直接进日志、过 `log.ledger`。这一端自己写的每一行都过一次 `log.ledger`（`retract.home` 只能从这里进账）；收到的行，账由 `log.admit`、`nodes.wire_take` 的回答给出。 只给了部分日志的调用（`log.merge` 只给了一部分行，或者这一端刚记下一个名字、一个撤回）之后，这一端其余的行哪些该变成桩，问 `log.reach`（给这些行的信封和账，回答 `stubbed`），不要自己按名字和拆分去算。
- **家怎么显示**（协调人 2026-10-31 定，伙伴回来可以改）：家里留着的东西永远作为家自己的节点显示。家是一个节点被撤回的那一端（它自己写了这个节点的 `retract.home`）；别的端写的 shared 的并入或拆出，只要动的是它，在家里都不显示（`nodes.fold` 跳过，`retracted`），所以它在家里还是它自己，看得见。**被撤回的节点结构冻结**：哪一端（家也一样）都不再并它、拆它、往它身上并或拆（不管它是并走的、被并的还是拆出的部分）；家只能改它的内容（名字、摘要、看法、段这些），写的都是 `local` 行。整理守这一条（见「她的节点」›「在端之间」），折叠也守：撤回以后写的并入、拆出，每一端都作废（`void`，上面的时钟规则）。**家里只有直接的忘掉拿得走**：家留着的节点 T，只有忘掉 T 自己、忘掉这一端也留着的、T 并进去的节点，或者忘掉它的主体（`subject:`），才把它的行变成桩；忘掉 T 并进去的、这一端没留着的节点 P（哪怕 P 已经空了），或忘掉 T 拆出去的部分 Z，都拿不走 T（`forget::View::forgets_here`）。别的端照常：忘掉 P、Z 够到的都拿走。
- **家自己的备份**（协调人 2026-10-31 定，伙伴回来可以改）：日志页里被撤回的节点只有桩，所以家的导出文件另带家页（`Part::Home`，段的头是 `{"part": "home", "bytes": n}`，只在第 2 版文件里）：同样的一批的外形，放家留着的节点的整行（连 `local` 行）、它们的 body、`retract` 和 `retract.home`，由 `nodes.home_send` 按受众写出。只有给伙伴电脑的文件能带它（`Manifest::with_home`，`sync.manifest` 的 `homes` 要 `holder` 是 `pc`），清单另记一项 `home`；站点和服务端写的文件不带，收的也不收（`nodes.home_take` 只给 `pc`，别的 `bad_input`）。恢复到一台新电脑：先把每一页家页交给 `nodes.home_take`（页里的每一行都必须是页里的标记或这一端的账留着的节点的，body 必须是它自己的规范字节），回答的行原样存成这一端自己的、body 存下、账换成回答的 `ledger`：这台电脑就是家了；然后照常经 `nodes.wire_take` 收日志页，日志页里的桩不会换掉家里完整的行。新电脑上已经忘掉的节点，家页里它的行收成桩。`sync_tests.rs` 对过：恢复到新电脑以后它是家，从服务端收的桩换不掉它；服务端写或收家页都拒绝。
- **折叠**（`log.fold`）：按 `(hlc, origin, dseq, id)` 排；发生过的事（看到、判断、说了的话、段和段里每一句、判例、记忆写入、心情变化）都留着；会变的值（节点上她的看法 `node.view_changed`、一件事挂在哪 `link.attach`、人设字段、衣服）最后一行是现在的，之前的进 `history`；记忆退役是单向的，除非之后有一次恢复。级联（给一个对象算出要连带删的派生内容）还没有。
- **执笔权**（`lease.*`）只由服务端发，epoch 只增，只有三种移法：持有者自己交（`handover`，带它在每个写过的受众里最后的 `dseq`：`through` 是 `{受众: dseq}`，没写过的受众不列）；服务端接（`takeover`）：持有者失联满 3 分钟、主人的位置证明主人在另一个场所（两边的场所都要知道、主人的位置 3 分钟内看到过、而且持有者失联和主人离开都过了 3 分钟）；主人按「让她来这里」（`force`）。`lease.here` 是持有者自己执行的那条：和服务端断开、主人又不在这个场所（本机输入或主人的设备经局域网连上来）满 3 分钟，它自己停下，不再想、不写她的行，只收主人写的、做完手上的事。服务端接管时它一定已经停下，所以不会有两个她；`holderLastSeen` 要算到持有者自己还以为连着的最后一刻（最后一次收到它，加上它判断断线用的心跳超时）。时间都是初值。其中「失联、主人又不在多久算停下」是服务端的设置（`stopAfter`，微秒，1 分钟到 1 天，不给是 3 分钟）：服务端用它判断接管，并随 `lease.grant`、`lease.epoch` 把同一个值交给持有者，持有者用它跑 `lease.here`。两边是同一个值，所以服务端永远不会比持有者停下得早。
- **单机时期是第 0 代。** 一端还没见过任何执笔权（单机用、没连过服务端）时，她就在那里想（`lease.here` 不传 `lease`，答 `thinking`），她的行写 `epoch: 0`；服务端发的第一份是 1，没有哪一份是 0。这些行在每个受众里占着那一端自己的 `dseq`，第一次配对或同步时照原样整段带上去，在那个受众里排在它后来的行之前；从它知道第一份执笔权起，不再写 0，再写就被 `epoch_back` 拒收。
- **时钟不能被一次跑快带偏**（`log.clock_*`）：日志的时钟要存下来、只增不减，所以机器的钟跑快时盖的一行会让之后每一行都超前，服务端拒收超前 5 分钟以上的（`hlc::check_ahead`），这一端就一直同步不了。三条规则：时钟的物理部分用「校正后的时间」，即本机时间加上和服务端量出的偏差（`log.clock_offset`：请求发出、服务端的 `serverTime`、收到回答，取中点），偏差由调用方存着、每次握手和每次 `clock_ahead` 时重量；盖章时物理部分最多到校正后的时间加 5 分钟，再往后只走计数（`log.clock_next`、`log.clock_observe`），已经超出的时钟原地不动、`poisoned` 为真；修复（`log.clock_repair`）只给还没离开这一端的行重新盖章，从第一条超前的开始按日志顺序往后，并给出以后要存的时钟。离开过这一端的行永远不重盖：服务端收了它，它就没有超前。收到 `clock_ahead`（带 `serverTime`）时这一端的顺序是：量偏差，修复没同步的行，存新时钟，再发。
- **两个她只可能来自 `force` 和单机时期。** 折叠时对照执笔权的历史（`leases`）：旧 epoch 的行写在 `force` 签出之后的，是原来那台还在跑时写的，照样留下，标 `concurrent`，碰到的对象也标；第 0 代的行写的时候别的端已经持有执笔权的（签出时间早于这一行、持有者不是它自己），是她在别处住着时这台被单机用了，同样标 `concurrent`，`with` 是当时那一份；那时还没有任何一份、或那一份就是它自己的第一份（时钟稍快会让单机的最后几行排在签出之后）的，是同一个她，不标；`takeover` 之后迟到的旧行是她自己的记忆补上来，不标；不是那个 epoch 的持有者写的（`not_holder`）、交出以后还写的（`after_handover`）是执笔权的 bug，列在 `standing` 里查。同一个 epoch 出现两次时，先签的算，再按持有者的字节序（`lease.newer`）。

- **哪一端能存哪个受众**（`myriad_persona_log::holder`，2026-10-30 起）：`holder` 是 `{core, owner}`，`core` 是 `pc`（伙伴的每一台电脑）、`server`（服务端，今天是站点的后端）或 `child`（手机、XR 这些子机核心），`owner` 是伙伴在私聊受众里的 id。服务端存所有受众，只是哪一端都不存 `local-owner` 或别的伙伴 id 的私聊，除非那就是它的 `owner`（见「伙伴的 id」）；电脑只存伙伴自己的私聊（`personal:<owner>`，只有伙伴一个人）和她自己的（`own:`）；子机核心不存日志。所以群的日志、群里成员的人物节点、别人和她的私聊永远到不了伙伴的电脑：`log.answer` 不发给它，`log.admit` 和 `nodes.wire_take` 在它那一端拒收。按受众判就够了：伙伴私聊里的主体只能是伙伴，她自己的受众里没有主体（`Row::check`），节点关系的另一头按同一个受众算 id。这是伙伴倾向同意、还没最终定的默认（原生 SYNC.md 问题 3），改它就是改所有端。

| 调用 | 输入 → 输出 |
| --- | --- |
| `lease.here` | `{me, lease?, now, disconnectedSince?, ownerHereAt?, stopAfter?}` → `{state: "thinking"}`、`{state: "stopped", stoppedAt}` 或 `{state: "child"}`。还没见过任何执笔权的那一端不传 `lease`：单机，`thinking`，她的行写 `epoch: 0` |
| `lease.move` | `{lease, request, now, issued, holderLastSeen?, holderPlace?, ownerPlace?, stopAfter?}` → `{granted: Grant}` 或 `{refused: 理由}`；`request` 是 `{handover: {from, epoch, to, through?}}`、`{takeover: {server}}` 或 `{force: {to}}`；`issued` 是服务端给新一份的 rev，必须晚于现在这份 |
| `lease.newer` | `{a, b}` → `{newer: "a" \| "b"}` |
| `lease.behind` | `{through?, from, held}` → `{behind}`：新执笔的那一端开始想之前还在等什么。`through` 是交出那份的 `{受众: dseq}`，`from` 是交出的那一端，`held` 是这一端每个受众的版本向量 `{受众: {端: dseq}}`；`behind` 是还没补到的受众和要补到的数，空的就是补齐了、可以想。接管和「让她来这里」没有 `through`，不用等 |
| `log.admit` | `{rows, now, known?, members?, ledger?, epochs?, holder}` → `{accepted, refused?, clockAhead?, ledger, epochs}`：查的是别的端来的一批；批里有撤回时必须给 `ledger`；`holder`（这一端）必填（ABI 2 起），它不能存的行拒收（`not_held`，`Holder::holds_row`：它不能存的受众，和她自己的受众里以别的伙伴 id 或 `local-owner` 为 id 的 `subject:`、`affect:` 对象）；这一端自己写的行走 `log.ledger`，不过 `log.admit` |
| `log.answer` | `{rows, theirs, holder}` → `{rows, notHeld?}`：一个受众里对方（持有这个受众的 `theirs`）缺的行，`forget` 在前；行跨了受众是 `bad_input`；`holder`（对方）必填（ABI 2 起），对方不能存这个受众时什么都不给，`notHeld: true` |
| `log.canonical` | `{row}` → `{row, canonical, digest}`；信封不对就是 `bad_input` |
| `log.clock_next` | `{last?, now, offset, node}` → `{rev, poisoned}`：`now` 是本机时间，`offset` 加上它是服务端的时间 |
| `log.clock_observe` | `{last?, seen, now, offset, node}` → `{last}`：`seen` 最多把物理部分推到上限 |
| `log.clock_offset` | `{sent, serverTime, received}` → `{offset, uncertainty}`（微秒） |
| `log.clock_repair` | `{last, floor?, unsynced: [{id, hlc}], now, offset, node}` → `{last, restamped}`；`floor` 是确知别处收过的最新 rev |
| `log.fold` | `{rows, leases?}` → `{objects, standing?}`：每个对象的 `rows`、`current`、`history`、`retired`、`concurrent`，或只有 `forgotten`；被撤回的对象带 `retracted`（写过撤回的各端），不是家的一端只有这一项 |
| `log.holdings` | `{rows}` → `{vector, beyond?}`：一个受众的行里每一端连续持有到第几条，和跳号之后先到的；行跨了受众是 `bad_input` |
| `log.ledger` | `{rows, ledger?}` → `{ledger}`：账加上这些行（这一端自己写的，或整个日志）给的项；信封不完整的行是 `bad_input` |
| `log.merge` | `{log, incoming, ledger?}` → `{log, added?, stubbed?, filled?, duplicates, conflicts?}`；`log` 可以只给和这批有关的行，这时必须给 `ledger`；行里有撤回时也必须给；`incoming` 里有 `local` 的行是 `bad_input`；家里桩不换掉完整的行，完整的行补上桩（`filled`） |
| `log.missing` | `{mine, theirs}` → `{missing: [{origin, from, through}]}` |
| `log.reach` | `{rows, ledger}` → `{stubbed}`：给的行（这一端日志里的任意一部分，信封就够，桩也行）里，账现在在这一端拿走、还完整的那些，排好序；信封不完整的行、没给账都是 `bad_input` |

载荷不在行里，另存、封好，按行的 `payload.ref` 取。对话、记忆和心情这几种行的载荷，每一端都按同一种形状写（`myriad_persona_log::payloads`，规范 JSON，时间是 Unix 毫秒；读的一端不认识的字段照样收、忽略）。谁说的、谁写的看行的 `author`，关于谁看 `subject`：

| kind | 对象 | 载荷 |
| --- | --- | --- |
| `segment.open` | `segment:<id>` | `{at, place}`：`place` 是 `web`、`app`、`group` 或别的界面 |
| `segment.line` | `segment:<id>` | `{at, text, line?, replyTo?}`：`line` 是写它的那一端自己的消息 id |
| `segment.close` | `segment:<id>` | `{at}` |
| `memory.write` | `memory:<id>` | `{at, text, kind, replaces?}`：`replaces` 是它取代的 `memory:<id>` |
| `memory.retire` | `memory:<id>` | `{at, why, by?}`：`by` 是现在说着这件事的那条 |
| `memory.restore` | `memory:<id>` | `{at}` |
| `affect.change` | `affect:<subject>` | `{at, valence, arousal, band, cause}`：变化之后的心情，0 到 100；`cause` 是类别，不是原话 |

节点（`node.*`）和判过的事（`case.*`）的载荷随节点的规则一起进来，不在这张表里。

## 伙伴的 id（`partner.*`）

伙伴私聊的受众是 `personal:<id>`。以前 Mac 用 `local-owner`，站点用它自己的账号 id。伙伴 2026-10-30 定：换成一个新的 id，两端都用它。2026-10-31 协调人把模型改简单（伙伴回来可以改）：**id 由站点发，Mac 只从认证连接拿；文件从不改变或增加 id**。规则在 `myriad_persona_log::partner` 和 `myriad_nodes::migrate`。

- **形状**：`partner_` 加 32 个小写十六进制数字：前 12 个是生成它的时间（Unix 毫秒，不早于 2025 年），后 20 个是随机数（80 位）。它是 id，不是凭据，不开任何东西。
- **谁生成**（`partner.mint`）：站点，每个账号一次（不是账号 id 本身）。时间和 10 个随机字节由调用方给，核心不读时钟和随机源。Mac 从不生成。
- **Mac 怎么拿到**（`partner.link`）：只从连到伙伴自己账号的认证连接（站点的 `welcome` 里的 `partner`）。还是 `local-owner` 的 Mac 拿到站点的 id，就在这条连接上第一次同步以前迁一次（`from` 是 `local-owner`）；已经是这个 id 的（连过的，或从文件认的就是它）什么都不做；是别的 id 的一律拒绝，连过的、从文件认的都一样：从文件认的 id 下可能是别的账号的伙伴的东西（Bob 的导出导进了 Alice 空的 Mac），哪一次连接都不把它迁进另一个账号。原生要做的：这时提出把这台 Mac 清空（从空的开始）再连。没连过的 Mac 就一直是 `local-owner`。没有别名表、没有比大小、没有 `others`。
- **文件**（`partner.file`）从不改变或增加 id。文件头里的伙伴 id 就是这一端的 id 时收；这一端还没有 id（还是 `local-owner`）、`personal:` 和 `own:` 里一行都没有、也从没连过（核心按调用方给的 `here` 查：`audiences` 是这一端有行的每个受众，`linked` 是 id 是否从连接来的）时，可以把文件的 id 认作自己的（`adopt`，伙伴 2026-10-30 定：「空的 Mac 直接认文件」）；头里没有伙伴的旧文件（页在 `local-owner` 下）也只有这样的一端能照原样收。别的一律拒绝：认过一个 id 的一端不会再被别的文件换掉 id，有 id 的一端也不收 `local-owner` 下的页。回答的 `id` 就是收这个文件的页时 `nodes.wire_take` 的 `holder.owner`。**限制**（协调人定，伙伴回来可以改）：有东西、没连过的 Mac 收不了另一台没连过的 Mac 的文件；要在两台 Mac 之间搬，要么两台都连上，要么导入到一台空的 Mac 上。
- **谁也不存别人的伙伴私聊**（`Holder::holds`）：哪一端（服务端也是）都不存 `personal:local-owner` 或 `personal:<伙伴 id>`，除非那就是它自己的伙伴（`holder.owner`）。所以没迁的旧 Mac 发来的 `local-owner` 一批被拒收（`not_held`），不会按同样的节点 id 存进站点；`#674` 还没上线，这样可以。服务端的 `holder.owner` 是这个账号的伙伴 id，一个账号一份日志。她自己的受众（`own:`）里也一样：对象是 `subject:` 或 `affect:` 而 id 是 `local-owner` 或别的伙伴 id 的行（没迁的 Mac 写的她对伙伴的心情，别人伙伴的），每一端收的时候都拒收（`not_held`，`Holder::holds_row`）。
- **怎么带过去**：「她的连接」协议 1 的 `welcome` 多一个可选的 `partner`（站点的伙伴 id，字符串）。认识这个字段的一端在 `hello` 的 `capabilities` 里列 `partner`；站点只在 `hello` 列了它时才放，所以旧的一端收不到它。`hello` 不带 id。导出文件见「同步」›「文件头里的伙伴」。
- **迁移**（`partner.migrate`，纯函数）：受众 `personal:<from>` 改成 `personal:<to>`，主体是 `from` 的改成 `to`，以人为 id 的对象（`subject:<from>`、`affect:<from>`）也改，伙伴私聊和她自己的受众里都改；行的 id、`origin`、`dseq`、`hlc` 都不动，所以同一行在每一端迁出来都是同样的字节。**账也一起迁**：给这一端的账，回答迁过的账，受众和以人为 id 的目标一样改，所以迁移以前「忘掉关于伙伴的全部」在迁移以后照样够到新 id 下的行。body 里提到伙伴的地方：片段和条目的受众标签、伙伴私聊里 `person:<from>` 的键（`node.created` 的 `key`、`node.related` 的 `to`、`node.absorbed` 的 `into`；她自己的受众里人的键算出的 id 就按键本身，改了就指到别的节点，所以不改）、谁知道、谁的进度，迁完重新给出规范字节。别的受众（群、别人的私聊）一行不动。同一端在同一受众里的同一个 `dseq` 落到两行上是 `clashes`，迁移补不了，留日志顺序先的那一行。`from` 只能是 `local-owner` 或伙伴 id：站点账号 id 下的行迁不了（它们的节点 id 是按账号 id 算的），所以不会悄悄迁成对不上的节点。**站点要确认**（`uncertain`）：`personal:<账号 id>` 下一行都没有（不只是节点行），`own:` 里没有以账号 id 为 id 的对象（`subject:<账号 id>`、`affect:<账号 id>`），账里也没有这样的目标。有的话要另有一条认可过的迁法，不在这一版里。调用方按受众存的东西（`known`、`epochs`、执笔权的 `through`、版本向量、下一个 `dseq`）用 `personal:<to>` 改键，计数接着原来的往下数。
- **节点的 id 不随伙伴的 id 变。** 伙伴私聊（`personal:<一个人>`，这个人是 `local-owner` 或任何 `partner_…`）里的节点 id 用一个固定的名字算：受众按 `personal:local-owner`（`myriad_nodes::key`）。这就是每台 Mac 一直用的那个，所以已有的 id 都不变，迁移只改受众的标签：每一行的对象、信封里的 `to`、`takes`，账里的节点都照旧成立。几个人的 `personal:a,b` 照受众本身算，也不迁。
- **「关于你」只有一个**：伙伴私聊里 `person:<这个私聊的伙伴 id>` 和 `person:local-owner` 都是「关于你」，算出同一个节点（键按 `person:local-owner`），`Key::is_about_you` 和算 id 用的是同一条规则，所以每个查「关于你」的地方（折叠、整理、名字的检查）都认得两个写法。别的伙伴 id 是别人。
- `sync_tests.rs` 对过：两台 Mac 从 `local-owner` 连上站点迁过去，按两种顺序同步，三端都只折出一个节点、id 就是原来的那个，迁移以前写的并入和拆出照样折得出来；迁移以前忘掉她对伙伴的心情，迁移以后站点那边的同样的行进来就是桩；迁移以前「忘掉关于伙伴的全部」（`subject:`），迁移以后站点那边的行进来也是桩；陌生人的旧文件页和别的伙伴 id 的页，在有东西的 Mac 上、在站点上都不收；她自己的受众里以 `local-owner` 或别的伙伴 id 为 id 的心情行，迁过的 Mac 和站点都不收；空的 Mac 认了 Bob 文件的 id，再连 Alice 的账号被拒，Bob 的东西不会迁进 Alice 的账号，清空以后再连什么都不迁。

| 调用 | 输入 → 输出 |
| --- | --- |
| `partner.file` | `{here: {id?, linked?, audiences?}, header?}` → `{id, adopt?}`：`header` 是文件头里的伙伴 id，没有就是旧文件 |
| `partner.link` | `{here: {id?, linked?, audiences?}, theirs}` → `{id, from?}`：`from` 是先要迁走的 id |
| `partner.migrate` | `{rows, bodies?, ledger, from, to}` → `{rows, bodies?, ledger, moved?, clashes?}`：`moved` 是信封或 body 变了、要写回的行 |
| `partner.mint` | `{at, random}`（`at` 是 Unix 毫秒，`random` 是 10 字节的 base64）→ `{id}` |

## 她的连接的帧（`link.*`）

「她的连接」的协议定义是 `shared/her_link_contract.json`。协议 2（契约里的 `next` 块，2026-10-23 定，还没有主机在说，服务端那一侧是 S6）让它成为每台设备唯一保持的连接，加了 `fact`、`hand`、`say`、`lease`、`log`、`peer`、`group` 七个帧族（`group` 是心智在电脑上时，服务端把群里的新消息借给它想、再替它把话发到群里；群的日志仍只在服务端），信封多一个 `v`（帧格式版本，1 是规范 JSON 文本）。结构见 Myriad 的 [ARCHITECTURE.md](https://github.com/Myriad-You/Myriad/blob/preview/docs/development/ARCHITECTURE.md)「Her link and her devices」。

`crates/myriad-her-link` 编译时读这份契约，按它判断一帧能不能发、能不能收，规则不会和契约漂移：信封字段、`v`、类型、`seq`／`ack`／`at`、可靠帧必须有 `id` 且符合 `idPattern`、其余帧不能有 `id`、正文的必填和可选字段、`oneOf`（例如 `facts` 和 `sealed` 恰好一个）、`maxItems`、发送方的角色、帧大小，以及密封字段的形状（`{suite, to, keyId, enc, ct}`，套件是契约里的那一个，`enc` 32 字节，`ct` 至少一个标签长）。协议 1 的帧在协议 2 的信封里照旧按它们自己的正文检查。HPKE 的封和开是各端自己的（用各自的钥匙），核心只给 `info`（`myriad-link/2 <t>`）和附加数据（规范 JSON `{from, t, to}`）的写法。

| 调用 | 输入 → 输出 |
| --- | --- |
| `link.check` | `{frame, roles?}` → `{accepted: {t, family, delivery, canonical}}` 或 `{refused: {code, detail}}`；`code` 是契约 `next.errorCodes` 里的一个；`roles` 是发送方的角色（`device`、`host`、`mind`、`server`），不给就不查 |
| `link.encode` | `{frame}` → `{text}`：要发的帧检查过后的规范文本；不通过是 `bad_input` |

## 授予和听众（`grants.*`、`audience.*`）

权限分三层：能力声明的、用户批准的、宿主在运行时授予的；只有授予决定行为，声明、批准、提议和计划都不算。她自己发起的 Work 另有个人自治授权，每次检查都按「现在授予的」重新过滤，并受认领时的上限（cap）约束。规则在 `myriad_agent_rules::grants`，从站点后端的 `consciousness/grant.rs`、`consciousness/dispatch.rs` 搬来；一条记忆能在谁面前说（听众）在 `myriad_agent_rules::memory::audience`，从后端 `memory/unified.rs` 的 `admits`、`audience_admits` 搬来。

`actor` 是 `{id, personal}`：宿主对行动者的 id，和宿主认不认这是一个人（心跳、system 身份不是）。授权是 `{actor, allowed, revoked}`；`grant` 不给或 `null` 是没有。`cap` 不给或 `null` 是没有上限。列表保持被缩小的那个列表的顺序和重复（授权的 `allowed`、`granted`、`required`），按集合读就是原生的 `Set`。输出是值不是句子，提示文字各端自己写。

| 调用 | 输入 → 输出 |
| --- | --- |
| `grants.granted` | `{declared, approved, runtimeAllowed}` → `{granted}`：三层的交，按 `approved` 的顺序；批准了没声明的名字丢掉，不授予 |
| `grants.effective` | `{granted, cap?}` → `{effective}`：上限之内的授予；上限只缩不扩 |
| `grants.check` | `{actor, grant?, granted, cap?, required}` → `{outcome: "allowed"}`、`{outcome: "autonomy_withdrawn"}`（有上限而授权已撤回、或上限内什么都没授予）或 `{outcome: "missing", missing}`（按 `required` 的顺序列出全部缺的；站点只报第一个） |
| `grants.autonomy` | `{actor, grant?, granted}` → `{verdict: "require_user_review"}` 或 `{verdict: "allow_personal_work", granted}` |
| `grants.may_enter_work` | `{actor, grant?, granted, source}` → `{mayEnter}`；`source` 是 `user` 或 `autonomy` |
| `grants.cap_still_allows` | `{actor, grant?, granted, cap?}` → `{allows}` |
| `grants.claim` | `{actor, source, grant?, granted}` → `{cap}`：认领用的上限，`null` 是留给用户 |
| `grants.prepare` | `{actor, requested, granted, candidates}` → `{grant}` 或 `{refused: "heartbeat_identity" \| "empty_after_filter"}`；空请求取 `granted` 里在 `candidates`（宿主自己的候选表）中的 |
| `grants.revoke` | `{grant, personal}` → `{grant}`（`revoked: true`，名字保留）或 `{refused: "heartbeat_identity"}` |
| `audience.admits` | `{learned, present}` → `{admits}`；听众是 `{venue: "private", members}`、`{venue: "group", group, members}` 或 `{venue: "own"}`（她自己的，不带 `members` 和 `group`），`members` 是宿主的 id（字符串）。学到时是 `own` → 任何现场都收；现场是 `own` → 只收学到时也是 `own` 的。字段值为 `null` 与不写相同：`own` 带 `members: null` 或 `group: null` 照收，带非空的 `members`、`members: []` 或任何 `group` 仍是 `bad_input`；私聊和群的 `members: null` 当作没写 `members`，是 `bad_input`，私聊的 `group: null` 当作没写、照收，群的 `group: null` 当作没写 `group`、是 `bad_input` |

- **谁算一个人、空请求取哪些候选，由调用方给。** 站点是 id 大于 0（心跳是 0），原生是不空且不是 `system`；候选表站点是 `max_user_agent_permissions`，原生另把 `mcp.` 开头的都算进去。规则只用调用方给的 `personal` 和 `candidates`。
- **她自己的记忆哪里都能说，授予永远三层求交。**（主人 2026-10-27 定，都照原生现状）她自己的记忆（`own`）任何现场都收，原生 `Audience.admits` 一直如此；站点原来在 `admits` 之外另走自己的查询，以后 `own` 行也交给 `audience.admits`。`grants.granted` 每次都是 `declared ∩ approved ∩ runtimeAllowed`，批准超出声明的丢掉，照原生 `PermissionState.granted`；站点 Tapp 原来在安装、更新时从声明里选出批准、运行时不再与声明求交。
- **两端不一致的，核心不答。** 存储里没记成员的旧记忆怎么读（站点当作它那个人的）、群 id 截多长，留在各端。

## 她的节点（`nodes.*`）

节点是她记在心上的一件事。规则在 `crates/myriad-nodes`，服务端和每台电脑都用它，所以同一串日志行在哪一端都折出同一个节点。第一批定了节点是什么、装什么、行怎么写、怎么折、放置的输入输出长什么样；第二批定了整理：哪些节点该整理、按什么顺序，她整理一个节点时看到什么、她的回答怎么变成行，以及关系带来的后果；第三批补上整理的缺口：调用方给的同一受众候选、没有经历的节点也能有依据、没有节点时只整理收件箱、一起看过的不被并走、整理里改概括、看法和没收尾的事（看法只凭算数的依据改）。M7 定了节点在端之间怎么走（下面「在端之间」）。唤醒（除了挡着的那件变了带来的那一种）、反对、只忘掉一段还都不在。

**三种节点**（`kind`）。种类只在规则不同时才成立：

| `kind` | 是什么 |
| --- | --- |
| `task` 进行中的事 | 有「做完」的标准；一直要做、没有尽头的（每周发版）状态是 `ongoing` |
| `topic` 话题 | 没有做完的标准、会反复聊；可以带一个外面的对象（作品、仓库、地方），记它的各种叫法和每个人看到哪 |
| `person` 人 | 会说话、有自己隐私的一方：伙伴（「关于你」）、她自己、别人、一个群 |

**节点 id 只由受众和一个稳定键算出**：`sha256("node␟<受众>␟<键>")` 的前 16 字节十六进制。种类不进 id（种类能改），设备不进 id（两台电脑看到同一个仓库得到同一个节点），本机路径不进 id。键只有五种：

| 键 | 什么节点 | 种类 |
| --- | --- | --- |
| `repo:<主机>/<路径>` | 一个仓库，按它的远端地址（`nodes.repo_key` 规范化：先全部小写，再去掉协议、用户名和口令、端口、`.git`（不管大小写、去到没有为止）、结尾斜杠；本机路径和 `file:` 拒收。规范化的结果再规范化一次还是它自己，能直接交给 `nodes.id`） | 永远是话题 |
| `person:<id>` | 一个人；在这个人自己的私聊受众里就是「关于你」 | 永远是人 |
| `group:<id>` | 一个群 | 永远是人 |
| `her` | 这个受众里的「她自己」 | 永远是人 |
| `minted:<记号>` | 她新立的一件事；写第一行的那一端生成一次记号，随 `created` 行走 | 进行中的事或话题，可以改 |

记号以 `together-` 开头的（`minted:together-…`）是她和伙伴一起看过、读过的东西：只收别的节点进来，自己永远不并进别处（不并入、不收回）。

**人和群住在他们自己的受众里**（伙伴 2026-10-30 按推荐默认定，可以再改）：`person:<id>` 和 `group:<id>` 的节点只在那个人的私聊或那个群里，永远不在她自己的受众（`own:`）里；她自己的受众里只有 `her`、仓库和她新立的事。每台电脑都存 `own:`，所以这样她记下的别人的事就永远到不了伙伴的电脑。`nodes.id` 拒绝算这样的 id；万一有人照样写了，折叠时这个节点读作什么都没有、它的每一行跳过（`person_elsewhere`），`nodes.wire_send`、`nodes.wire_take` 也不让它的 body 走。

**头和身子。** 头是她对这件事的整理：名字、概括（每句带依据）、看法（每条带依据，改了留历史）、没收尾的事、想说没说的话、谁知道、状态，还有对象和每人的进度。身子是它依据的东西：经历片段、条目（事实和偏好）、指向原文的引用。原文本身不在节点里。

**每段经历、每条条目带三个标签**（每个 body 都带前两个）：

- `sync`：`shared`（可以去她的每一台设备），或 `{"local": <设备>}`（只在那台设备上）。标签要和行一致：`shared` 只能在 `shared` 的行上；`local` 只能在那台设备自己写的 `local` 行上。
- `saidBy`（是谁说的，按字母排、不重复）：`partner` 伙伴、`her` 她自己、`third_party` 第三方内容（网页、视频、字幕、别的 AI、编程 agent 的输出）、`other_person` 别人（群里的人，或别人私聊里的那个人）。一段对话的经历列出里面所有说话的人。伙伴写的行（`author: owner`）只能是 `["partner"]`。
- `audience`（经历和条目才有）：在哪说的。和行的受众相同；唯一的例外是伙伴自己在群里说的话，可以记在伙伴的私聊里。群里别人的话不进私聊。

**行和 body。** 节点的每次变化是日志里关于 `node:<id>` 的一行（信封不变），内容在另放的 body 里（`nodes.body` 检查它并给出规范字节）。每个 body 带 `version`（现在 1），拒收不认识的字段；列表要排好序、不重复，所以每个 body 只有一种字节。

| kind | body（除 `version`、`sync`、`saidBy` 外） |
| --- | --- |
| `node.created` | `kind`、`key`、`name`、`object?`（仓库键必须带 `repo` 对象，别的键不能带） |
| `node.kind_changed` | `base`、`kind`、`reason`、`because`：改种类是一行带理由的历史，不是关系 |
| `node.revised` | `base`、`field`（`name`、`summary`、`status`、`who_knows`、`object`、`progress`）、`person?`（只给 `progress`）、`value` |
| `node.view_changed` | `base`、`view`、`text`、`because` |
| `node.open_item` | `base`、`item`、`text`、`because`、`closed?` |
| `node.intent` | `base`、`intent`、`text`、`because`、`status`（`waiting` 等着、`said` 说了、`done` 完了、`dropped` 放下）、`trigger?`（`{at, zone}`：绝对时刻加当时的时区） |
| `node.span` | `audience`、`text`（她的话）、`refs?`、`from?`、`to?` |
| `node.entry` | `audience`、`text`、`refs?`、`quoteOnly?`（找不回原文的标「只有引文」，这时没有 `refs`；有 `refs` 就不能标） |
| `node.related` | `relation`、`to`（另一个节点的键）、`reason`、`because` |
| `node.unrelated` | `base`（收回这个节点上的哪几条 `node.related`）、`reason` |
| `node.done_when` | `base`、`text`（「做完」是什么，她的话；`null` 是没有）、`because` |
| `node.split_off` | `to`（拆出来的新节点的键，必须是 `minted:`）、`spans`（这个节点上被拆走的经历行）、`reason`、`because`：写在被拆的那个节点上 |
| `node.absorbed` | `into`（并进哪个节点的键）、`how`（`merged` 并入：本来是一件事；`folded_back` 收回：下属没有自己的状态了，回到上级成为经历）、`reason`、`because`：写在并走的那个节点上 |
| `node.used` | `via?`（回想顺着哪几条 `node.related` 带出了它；不写是直接想起它） |
| `node.upkept` | `run`、`seen`（每台设备她看到的最新一行的时钟，最多 64 台）、`floor?`（其余设备的界线）、`wrote?`（这次整理写的行）、`fallback?`（这次回答读不了、什么都没改时写原因） |

- `because` 是依据的 id（`span:<行>`、`entry:<行>`、`message:<id>`……），至少一个：概括的每一句、看法、没收尾的事、想说的话、改种类和关系都要有。
- `base` 是写的时候看到的那一版（行 id）；第一次写是空的；两边都改过、她合起来时列出两版。

**折叠**（`nodes.fold`）：输入任何顺序、可以重复、可以有桩，加上够到它们的 `forget` 行，结果和顺序无关。

- 头里每一项（种类、名字、概括、状态、谁知道、对象、每人的进度、每条看法、每件没收尾的事、每句想说的话）分开算：被别的行写进 `base` 的版本是旧的，进 `history`（留最新 20 个）；剩下的是当前的。
- **两边都改过**：当前的不止一个、而且说的不一样，就是两台设备从同一版各改了一次、互相没看见。这一项标 `bothChanged`，显示日志顺序里先写的那一个，其余的列在 `rivals` 里。后写的不会自动盖掉先写的；下一次有一行把所有当前版本都列进 `base`，才算合好。每一项都带 `base`：当前所有版本的行 id（排好序），就是下一次写这一项时该填的 `base`。两边从同一版改成一样的不算冲突，`base` 里两行都列着；之后哪一行只列了其中一个，也算两个都看见了。
- 经历和条目按日志顺序都留着，各带三个标签。`history` 列出每次改种类：从什么、到什么、为什么。
- 不合规矩的行跳过，并在 `skipped` 里写明原因：`no_body`、`bad_body`、`other_node`（created 的受众和键算出别的 id）、`sync_mismatch`、`author_mismatch`、`audience_tag`、`not_in_audience`（谁知道和进度只能填这个受众里的人）、`untrusted_intent`（想说的话只能来自伙伴的话或她自己）、`no_spans_here`（「关于你」只放条目、不挂经历）、`kind_fixed`、`status_for_kind`（人没有做完）、`object_for_key`（仓库对象只在仓库键上，仓库节点的对象拿不掉）、`self_relation`、`cycle`、`not_minted`（仓库和人的节点各只有一个，不能拆出来也不能并走；并进「关于你」的也是 `no_spans_here`）、`together`（一起看过的节点并进别处）、`absorbed_twice`（并走第二次，第一次为准）、`retracted`（被撤回的节点的 shared 并入或拆出：写在撤回之后的，或在它的家里）、`person_elsewhere`（人或群的节点写在了她自己的受众里，整个节点的每一行都跳过）。核心不读的 node 种类（原生旧的行）只算一次变化，不读内容。
- `createdBy` 写着 created 行是谁写的；头里每个版本都写着是谁写的，所以整理知道哪些是伙伴定的。
- 忘掉一个节点（或这个受众里关于某人的全部）：节点只剩 `{id, audience, forgotten}`，它的行进 `stubbed`，`forget` 排在前或后都一样。只忘一段经历是 M2 的事，这一批没有。

**五种关系**，都在同一个受众里（`to` 是键，按行自己的受众算 id，所以跨不出去），每条带她的一句理由、依据、谁建的、什么时候：

| `relation` | 意思 |
| --- | --- |
| `part_of` 属于 | 那件做完之前，这件必须做完或放弃；不能成环（按日志顺序，会成环的那一条跳过） |
| `blocks` 挡着 | 那件卡住的原因是这件 |
| `clashes` 撞上 | 两件在做同一件事或方向相反；两头写的算同一条 |
| `involves` 牵涉 | 那个人或对象出现在这件事的状态里 |
| `related` 相关 | 她觉得有关、但不是上面四种；两头写的算同一条 |

一条关系一直在，直到同一节点上有一行 `node.unrelated` 把它列进 `base`；`rows` 列出说它的每一行。指向被忘掉的节点的关系跟着消失，不管那个节点的行在不在这一批里（看遗忘账，不看行）。拆自、并入、改种类是历史，不是关系。

**关系带来的后果**（都是折叠和整理队列里的确定规则，不是她的判断）：

- 属于：上级的 `parts` 数它现在的下属，几件做完、几件放弃（下属没写状态算进行中）。一端变了，另一端排进整理。
- 挡着：挡着的那件一变，被挡的那件排进整理（这就是结构带来的唤醒）；互相挡着时两头都举手（`mutual_block`）。
- 撞上：新出现的撞上让两头举手（`clash`）。
- 牵涉：这一批没有自动后果；回想时一起想起是 M3 的事。
- 相关：不触发整理。它淡不淡看「用没用」，不只看多久：回想顺着它带出东西时写一行 `node.used`，折叠在关系上给出用过几次（`uses`）和最后一次（`lastUsed`），节点上也一样。怎么由这两个数算出可见度（权重、衰减）由调用方定、真机上调，核心不放阈值。

**整理留下什么**（折叠里）：

- 拆分：拆分和并入按日志顺序一件件算，每次看那段经历这时在哪。`node.split_off` 列的经历只要这时在被拆的节点上（本来写在这里的，或者并入、上一次拆分带过来的），就挂到新节点上，标着 `movedFrom`。两个节点都多一行 `lineage`（`split_off`、`split_from`）。
- 并入和收回：并走的节点标 `absorbedInto`，读作没了；它的经历和条目挂到并进的那个节点上（一路并下去就挂到最后那个），它的关系跟着它没了。两边各多一行 `lineage`（`absorbed_into`、`absorbed`）。
- 独立：一条 `part_of` 被收回时，下属多一行 `lineage`（`left`，原来属于）。
- 忘掉也跟着走：并进被忘掉节点的节点一起忘掉（它的 `forget` 列在 `forgets` 里，调用方写下来，body 删掉以后也一直算数）；被忘掉的拆分部分带走的经历一起忘掉，那行拆分也一起忘掉。

**整理队列**（`nodes.queue`）：只有状态变了才排进来，不看日历、不数次数。一个节点「看着」这些行：自己的行（`own`）、别的节点建给它或收回的关系（`relation`）、它下属的行（`part`）、它上级的行（`whole`）、挡着它的那件的行（`blocker`）；另外互相挡着（`mutual_block`）和撞上（`clash`）是新的时也排进来，好让她举手；头里有一项两边都改过、而没有哪一次整理同时看过它的每个当前版本时，也算它自己的行没看过（`own`，见「在端之间」）。回想（`node.used`）和整理标记（`node.upkept`）不算变化，跳过的行不算，相关不算。忘掉的、并走的、created 还没到的节点不排。

- 看过没有：节点上 `node.upkept` 的 `seen` 记着每台设备她看到的最新时钟（这个节点看着的行，加上和它一起给她看的邻居、候选的行：她看过、又在这次让它看着的节点，不会让它马上再排进来），`wrote` 列出这次整理自己写的行，这些算看过（所以自己写的行不会让它马上又排进来）；她回答的时候同一台设备上别的东西写的行不算看过。一台设备的行按它自己的顺序到，所以每台设备记一个时钟就够。`seen` 取的是这个节点看着的行和给她看的邻居、候选的行里每台设备最新的时钟，所以**调用方必须把这个节点看着的每一行都放进 `rows`**（它自己的、下属、上级、挡着它的）：给了某台设备较新的一行、却漏了同一台设备上它看着的较旧的行，那行会被当成看过。设备超过 64 台时留最新的 64 台，其余的合成一个 `floor`（毫秒）：不在 `seen` 里的设备，时钟不晚于它的算看过。
- 顺序：最早一行没看过的变化越早，越先整理（从旧到新消化积压，谁都不会一直等）；一样早的按那行的时钟（毫秒、计数、设备），再按受众、id。`max`（1 到 200）是这次给几个，`more` 是还有几个在等。

**整理一个节点**（`nodes.upkeep_input`、`nodes.upkeep_read`）：判断由她做（模型），用她自己的话；这里只定她看到什么、她的回答怎么变成行，两边主机才会写出同样的行。提示词以后再定。

- `upkeep_input` 给她看：这个节点（`n1`）的头（种类、名字、状态、「做完」是什么、概括（第几句就是 `t1`、`t2`……）、没收尾的事（`o1`……）、想说的话、看法（`v1`……）、下属的汇总、伙伴定过哪些（`partnerSet`）、哪些两边都改过（`bothChanged`）、是不是伙伴建的；没收尾的事和看法当前版本是伙伴写的标 `byPartner`）；它最新的经历（最多 24 段，每段截到 400 字，合计 12000 字，从旧到新编号 `s1`……）；它最新的条目（最多 12 条，每条 400 字、合计 4000 字，编号 `e1`……）；隔一条关系的邻居（最多 16 个，编号 `n2`……，带种类、名字、状态、概括第一句和关系的方向）；候选（`candidates`，调用方按自己的回想排好的同一受众别的节点，最多 12 个，编号接着邻居往下，带种类、名字、状态、概括第一句：可能是同一件事、或者收件箱那条该去的地方；已经是 `n1` 或邻居的、忘掉的、并走的、还没建的跳过；不在这个受众或不在给的行里就整次拒绝，重复也拒绝）；收件箱（最多 20 条，每条 400 字、合计 6000 字，编号 `i1`……；一条是伙伴在群里自己说的话时带 `audience`（那个群），规矩和经历一样，别的都拒收；放到节点上的经历记着它说在哪，群里的和这里的不能放成一段）；为什么现在整理它（`due`）。id 不给模型。
- 不给 `node` 时只整理收件箱（一个节点都没有，或者没有到期的）：这时必须有收件箱，候选从 `n2` 编起，只能立节点、把收件箱放到候选或新节点上、在给她看的节点之间建关系；对 `n1` 的动作是她没看到的编号，收回关系不允许（候选不带关系，她看不到要收回的那条）。回答不写标记：放好的条目照常从收件箱拿掉，放到的节点自己会排进队列。
- **依据**（`because`）只能是她看到的东西：经历 `s…`、条目 `e…`、收件箱 `i…` 就是它自己（`span:<行>`、`entry:<行>`、条目的 id）；概括的一句 `t…`、没收尾的事 `o…`、看法 `v…` 代表它们自己的依据。所以没有经历的节点（迁移来的、只被整理过的）也能举手、建关系、改种类、拆出和并入。
- `upkeep_read` 读 `{"actions":[{"do":…}]}`，按顺序变成要写的行，最后一行总是 `node.upkept`。每一行带着它的 `id`，调用方就用这个 id 写（再加时钟和设备、按顺序写）。`run` 是调用方这次整理生成一次的记号，行 id、新节点的键、新的想说的话的 id 都由它算出来，所以同一个回答在哪一端都写出同样的行。一次回答最多写 256 行（不算标记）。
- 她写的 `part_of` 照折叠的规矩先算一遍：日志里所有会被折叠考虑的 `part_of`（包括之前因为成环被跳过的），按日志顺序，加上这次收回的和新写的，会成环的那条不算。所以收回一条以后复活的旧关系也算在里面，她写的那条在折叠里不会被跳过。

| `do` | 她说的 | 写的行 |
| --- | --- | --- |
| `create` | 新节点 `as`（`new…`）、`name`、`done?`、`ongoing?`、`because`（立节点） | `node.created`；有 `done` 时 `node.done_when`；`ongoing` 时状态 `ongoing` |
| `split` | 拆出 `as`，带走 `spans`，`name`、`done?`、`ongoing?`、`reason`、`because`（拆分） | 同 `create`；这个节点上 `node.split_off`；新节点 `part_of` 这个节点 |
| `place` | 收件箱的 `items` 放到 `node`，用她的 `text`（收件箱归位） | `node.span`，引用就是这些条目；`placed` 列出放好的条目，调用方从收件箱拿掉 |
| `absorb` | 这个节点并进邻居或候选 `into`，`how` 是 `merged` 或 `folded_back`，`reason`、`because`（并入、收回） | `node.absorbed`；没收尾的事、还等着说的话、它的关系在并进的节点上再写一遍 |
| `relate` | `from?`（默认这个节点）→ `to`，`relation`、`reason`、`because`（挂两处就是第二条 `part_of`） | `node.related` |
| `unrelate` | `from?` → `to`、`relation`、`reason`（独立就是收回 `part_of`） | 在存着这条关系的每个节点上 `node.unrelated` |
| `kind` | `done?`、`ongoing?`、`reason`、`because`（进行中 ↔ 话题） | 变了的才写：`node.done_when`、`node.kind_changed`、状态 |
| `status` | `status`（`active`、`resting` 歇着、`done`、`dropped`、`ongoing`） | `node.revised` |
| `hand` | `text`、`because`：向伙伴举手 | `node.intent`，等着说 |
| `summary` | `sentences`，每句 `{text, because}`（改写概括） | `node.revised` 的概括 |
| `view` | `view?`（一个 `v…`；不写是新的一条）、`text`、`because`（看法变了） | `node.view_changed` |
| `open` | `item?`（一个 `o…`；不写是新的一件）、`text?`、`closed?`、`because`（没收尾的事，改写或收尾） | `node.open_item` |

- **种类只由「做完」的标准决定**：有做完的标准是进行中的事；没有、会反复聊是话题；没有、但一直要做（`ongoing`）是进行中的事，状态 `ongoing`。仓库和人的种类不能改。
- **看法只凭算数的改**：一条看法的每个依据都必须是直接给她看的经历、条目或收件箱的一条，只有伙伴或她说的（`saidBy` 只有 `partner`、`her`），而且不是在群里说的（按它自己的 `audience`）；网页、别的 AI、群里别人的话、或者只引概括和别的头一项的，都不改看法，列在 `held`（`not_counted`）。概括和没收尾的事不受这一条限制。
- **一起看过的不并走**：`minted:together-…` 的节点并进别处和人、仓库一样，整个回答不改（`not_allowed`）；别的节点并进它可以，从它拆出一部分也可以（和仓库一样，只有人不能拆）。
- **伙伴定的为准**：要改的头一项、没收尾的事或看法当前版本是伙伴写的、要收回的关系是伙伴建的，或者要并走的节点本身、它的某条关系、它头里的任何一项（种类、名字、概括、状态、「做完」、谁知道、对象、进度）、某条看法、没收尾的某件事、还等着说的某句话有一个当前版本是伙伴写的（并入只把经历、条目、没收尾的事、想说的话和关系带过去，而且以她的名义再写一遍，伙伴定的别的东西会跟着并走的节点消失），这一条动作就不做，列在 `held`（`partner_set`），她只能举手；同一个回答里别的动作照做。
- **读不了就什么都不改**：超过 32768 字节、不是 JSON、没有 `actions`、超过 24 条、不认识的动作、她没看到的编号、字段不对、没有依据或依据不是她看到的、`part_of` 成环、规则不允许的（人被拆或并走、一起看过的被并走、种类和键不合、经历挂到「关于你」或并进「关于你」、一条收件箱放两次、同一项（同一条看法、同一件没收尾的事）写两次、收回到不是上级的节点、收回不存在的关系、新节点上什么都没有），这些都只写 `node.upkept`，`fallback` 写明原因，队列照样往下走；这个节点下次有变化时再排进来。
- 写出来的每一行都先经 `nodes.body` 同样的检查，不合格的不写。

**放置**（`nodes.place_input`、`nodes.place_read`）：一段对话告一段落时，她把它切成一段一段，每段只讲一件事，挂到一个节点上或进收件箱。挂哪里由她判断（模型）；这里只定发给模型的东西怎么截、模型的回答怎么读，两边主机才会一样。提示词是 M2 的事。

- `place_input`：条目按顺序取，每条截到 600 字，最多 40 条、合计 12000 字（第一条一定要；`taken` 是取了几条，剩下的下一次再放）；候选节点按给的顺序最多 12 个，名字截到 40 字、概括 160 字，编号 `n1`、`n2`……，节点 id 不给模型。截断按 Unicode 标量计数，超出的以 `…` 结尾；控制字符换成空格。
- `place_read`：读 `{"spans":[{"start":1,"node":"n2","gist":"…"}]}`。条目从 1 编号，每段从自己的 `start` 到下一段之前；第一段从 1 开始，`start` 只增；`node` 是编号，`null` 或不写就是收件箱；`gist` 是她给这段的几个字，截到 120 字。回答包在代码块里或前后有话都能读。别的地方不对（不是 JSON、没有段、段比条目多、`start` 不对、编号不认识、没有 `gist`、超过 16384 字节），整段作为一段进收件箱，`fallback` 写明原因：什么都不丢，也不猜。

所有长度和条数都在 `myriad_nodes::limits`，`nodes.limits` 原样给出。它们是初值，改在这里就是改所有端。

| 调用 | 输入 → 输出 |
| --- | --- |
| `nodes.body` | `{kind, body}` → `{body, canonical, digest}` |
| `nodes.fold` | `{rows, bodies?, ledger}` → `{nodes, relations?, stubbed?, skipped?, forgets?}`；节点多 `retracted?`（写过撤回的各端）；`skipped` 的理由多了 `retracted`（家里别的端动它的 shared 并入、拆出）和 `void`（撤回以后写的并入、拆出） |
| `nodes.home_send` | `{audience, rows, bodies?, ledger, holder}` → 家页 `{audience, rows, payloads?, withheld?}`：这一端留着的节点的整行、body 和标记；`holder` 不是 `pc` 是 `bad_input` |
| `nodes.home_take` | `{page, holder, ledger}` → `{rows, bodies?, ledger}`：把家页恢复到这台电脑上，它成了家；`holder` 不是 `pc`、页里有不是留着的节点的行、body 不是规范字节都是 `bad_input` |
| `nodes.id` | `{audience, key}` → `{id}` |
| `nodes.limits` | `{}` → 每个上限 |
| `nodes.place_input` | `{items: [{id, saidBy, text}], candidates?: [{id, kind, name, summary?}]}` → `{items: [{n, id, saidBy, text}], taken, candidates?: [{label, id, kind, name, summary?}]}` |
| `nodes.place_read` | `{items: [id], candidates?: [{label, id}], output}` → `{spans: [{items, node?, gist}], fallback?}` |
| `nodes.queue` | `{rows, bodies?, ledger?, max}` → `{due: [{audience, node, since, why}], more?}` |
| `nodes.repo_key` | `{remote}` → `{key}` |
| `nodes.upkeep_input` | `{rows, bodies?, audience, node?, candidates?: [id], inbox?: [{id, saidBy, text, audience?}], ledger?}` → `{node?, spans?, entries?, neighbours?, candidates?, inbox?, due?, taken, local?}`；`local` 为真时这个节点被撤回、这一端是它的家，这一次整理只写 `local` 行 |
| `nodes.upkeep_read` | `{input: <upkeep_input 的输入>, run, sync, output}` → `{writes: [{id, node, kind, body, to?, takes?}], placed?, held?, fallback?}`；并入、拆出的那一行带 `to`（和拆出的 `takes`），调用方原样写进行的信封；`upkeep_input` 回答 `local` 时 `sync` 必须是 `local`，不然是 `bad_input` |
| `nodes.wire_send` | `{audience, rows, bodies?, theirs?, ask?, holder, ledger, elsewhere?}` → `{batches: [{audience, rows, payloads?, withheld?: [{id, why}]}], notHeld?}` |
| `nodes.wire_take` | `{batch, holder, now, known?, members?, ledger?, epochs?, held?, elsewhere?, people?}` → `{accepted, refused?, clockAhead?, ledger, epochs, bodies?, withheld?, ask?, elsewhere?}`；批里有撤回时必须给 `ledger` |

**在端之间**（`nodes.wire_send`、`nodes.wire_take`，M7「节点同步」）：每个主机核心都存自己的日志，节点从日志折出来；端和端之间换的是行和行的 body，不是折好的节点。

- **一批**是一个受众的行和它们指向的载荷：`{audience, rows, payloads: [{ref, data}], withheld: [{id, why}]}`，就是 `log.rows` 帧的正文，也是导出文件的一页日志。`ref` 是那一行自己的 `payload.ref`，`data` 是 body 的规范字节（base64）。行和 body 都按写它的那一端存的字节原样走，所以每一端折的是同样的字节；行上写着受众（`audience`）和写它的那一端（`origin`）。
- **发**（`wire_send`）：对方缺的行（`log.answer` 的规矩：`forget` 在前，`local` 的行永远不发），对方不能存这个受众时什么都不发（`notHeld`）。每一行都发；body 只在通过下面的检查时跟着走，不通过的这一行单独走，这一批的 `withheld` 写着为什么：对方的版本向量要数到这一行，少一行它在这一端的计数就永远停在那里。之后是对方要求再发的行（`ask`，对方 `log.vector` 的 `ask`，最多 64 个）：对方已经有这些行、缺它们的 body，这些行连同 body 再发一遍；不是这一端有载荷的 shared 行的 id 略过。每批不超过 256 行、64 个载荷、约 192 KiB（`wire::BATCH_*`，和契约 `log.rows` 的上限对照着测）。
- **收**（`wire_take`）：先过 `log.admit` 的全部检查（时钟超前整批不收、信封、`local`、对象换了受众、主体不在群里、epoch 回落、忘掉的账、这一端不能存的受众），不是这一批受众的行拒收（`envelope`）；收下的每一行的 body 再过一遍同样的检查，而且必须恰好是它自己的规范字节。不通过的 body 不收（行照收，`withheld`），折叠时这一行跳过（`no_body`）。没跟着来的 body 记发的一端说的理由，没说就是 `no_body`。被忘掉的对象的行进来就是桩，body 丢掉。同一个 `ref` 出现两次、`withheld` 里的 id 不是这一批的行、或者一个 body 既带着又说扣下，都是 `bad_input`。这一端要求再发的行回来时，调用方把自己存的那几行作为 `held` 给进来：一模一样的不再过 `admit`（早就收过了，再过会被 epoch 回落挡住），只收 body；不一样的拒收（`envelope`）。
- **扣下有两种**（`Why::lasting`）：`no_body`（发的一端自己还没有）和 `bad_body`（路上坏了、版本不一）是一时的，`wire_take` 把它们列进 `ask`，下一次 `log.vector` 带上要求再发，每次连接每行最多要一次；其余的（`secret`、`local_ref`、`not_shared`、`no_shape`、`too_large`、`audience_tag`、`person_elsewhere`）是这个 body 自己的事，再要也一样，不要。所以保证是：**一个 body 在哪一端有，就是同样的字节**；永久扣下的 body 只留在写它的一端，那一端折得到它、别的端跳过这一行，两边折出来的差别正好是这些行，按理由列着；一时扣下的要回来以后，两边折出同样的节点（`sync_tests.rs` 丢掉一个 body、要回来再对过）。
- **body 要能走**：核心读得懂（`node.*` 里 `nodes.body` 读的十五种，或 `myriad_persona_log::payloads` 里有形状的段、记忆、心情，而且没有形状里没写的字段：核心不认识的字段它也查不了，`bad_body`；以后给载荷加字段，要等各端都钉到认识它的核心再写；原生旧的节点种类和判例的 body 还没有形状，只发行，`no_shape`）；规范字节不超过 128 KiB（`wire::MAX_BODY_BYTES`，base64 后连同它的行放得进一批，所以一帧放得下；超了的每次都会被链接拒绝、挡住后面的行，`too_large`）；片段和记录标的受众就是这一行的受众，或是伙伴在自己私聊里说的群里的事（折叠认的同一条，`audience_tag`）；`sync` 是 `shared`（`local` 的标签哪怕在 shared 的行上也不走，`not_shared`）；引用和依据里没有本机文件或路径（`file:`、`/…`、`~…`、`C:\…`，`local_ref`；会话记录的 id `record:<会话>#<行>` 作为 id 走，记录本身留在那台机器上）；不是写在她自己受众里的人或群的节点（`person_elsewhere`，见上「人和群住在他们自己的受众里」；这样的节点的 created body 永远不出机，所以收的一端只能从发的一端说的理由认出它，而它之后的行可能在另一批里来：`wire_take` 把这一批认出的节点列在 `elsewhere`（看到的 created body，或发的一端为这个理由扣下的那一行；后一种只在收的一端自己能把这个节点 id 对上一个人或群的键时才算：`people` 里给的键、伙伴自己、这一批 body 里出现的键，在这个受众里算出的 id。发的一端怎么说都不能让收的一端扔掉普通节点的 body），调用方存下来，以后每次 `wire_take`、`wire_send` 都作为 `elsewhere` 给进去，这个节点的 body 就再也不收、不发；之前已经存了的它的 body 删掉）；她的文字里（节点的名字、片段、理由这些，载荷里除 id 以外的每个字符串）没有密钥扫描（`filter.scan_secrets`）会抹掉的东西，其余每个字符串（id、标记、map 的键，载荷的 `line`、`replyTo`、`replaces`、`by`）里没有已知形状的密钥（厂商前缀、私钥、`key=value`、Bearer 这些；只是看着随机的不算，id 和 digest 本来就像随机的）（`secret`）。写的一端写之前就该抹掉密钥，这一道是兜底：漏进 body 的密钥不会跟着 body 离开这台设备。body 读不了或不合形状是 `bad_body`，指向的载荷没有是 `no_body`。
- **合并**照 `log.merge`：并集，按 id 去重，桩赢；折叠按 `(hlc, origin, dseq, id)` 排，所以同样的行不管从哪一端、按什么顺序到，折出同样的节点（`sync_tests.rs` 把每一行单独、按全部 24 种顺序各送两遍对过）。同一项两边从同一版各改一次，折叠标「两边都改过」，两版都留着，后写的不会因为时钟晚就赢（上面「折叠」）。
- **整理标记跟着走**：`node.upkept` 是普通的 shared 行，`seen` 按设备记时钟，合并后每一端读到的都是所有标记。一行算看过，只要有一次整理看过它。只有一点要补：执笔权被伙伴强行换走、两边同时整理时，每一边的标记只盖住自己那一边的改动，合起来却什么都盖住了，「两边都改过」的那一项就没人管。所以整理队列多看一件事：头里有一项两边都改过，而没有哪一次整理（一个 `node.upkept`）同时看过它的每个当前版本，这个节点就排进来（理由记 `own`：是它自己的行还没被一起看过），直到她写一版把两版都列进 `base`，或者有一次整理同时看过两版。输出的形状不变。
- **忘掉跟着走**：忘掉是一行 `forget`，发的时候排在最前；收的一端先记进忘掉的账，同一批和以后再到的这个对象的行都只收成桩、body 丢掉，所以离线的一端回来也推不回去。`nodes.fold` 的 `forgets`（并进被忘掉节点的那些）由调用方写成 `forget` 行，照样同步。
- **撤回同步跟着走**（「她的日志」›「撤回同步」）：撤回和忘掉一样排在最前。`wire_send` 按 `ledger`（这一端的账）和给的行发：被忘掉或被撤回的节点的行只发桩，从家发也一样，哪怕给的行里没有那一行撤回；节点的每个名字都算（并进它的节点、拆到它的段），名字来自信封和账。`wire_take` 收到撤回，没有这个节点的家的一端把它的行收成桩、body 不收；有家的照收；又要回 body 的行（`held`）也照账来。批里有撤回时 `ledger` 必填。折叠里，家折出完整的节点并带 `retracted`；别的端只有 `retracted` 一项，指向它的关系跟着消失（和忘掉一样），并进它的节点、拆到它的段也一起走。`sync_tests.rs` 对过：离线撤回（家离线时伙伴撤回、服务端同时在这个节点上写，两种同步顺序服务端结果一样，那段在服务端知道撤回以前到了家，家就留着，没到的哪里都没有它的 body）；撤回和另一台电脑上的修改同时发生（三端、两种顺序，结果逐字节相同）；撤回之后导出（家导出的日志页里这个节点只有桩和撤回）；两端都撤回，两端都是家；家先收桩后收完整的行和反过来结果一样，导入自己的导出也不丢；不认识的端、刚连上的端写的撤回照收；别的端用这一端的名字写的撤回在这里只是忘掉，`retract.home` 收不进来；并进被撤回节点的节点、拆到被撤回节点的段，在已经没有那份 body、只剩桩的一端照样拿走。
- **被撤回的节点结构冻结**：不是家的一端，被撤回的节点只剩 `retracted`，排不进整理。家里，它的整理（`upkeep_input` 回答 `local`）只写 `local` 行，只改内容：不拆它、不把它并进别处，也不点名这一端没留着的节点；`upkeep_read` 给 shared 的 `sync` 就拒绝。别的节点的 shared 整理看不到它（不作为邻居、候选给出来），答案里点到它（并进它、和它连上关系、往它身上放）也不算数（`not_allowed`）；家里 `local` 的整理只给、只认这一端留着的节点。`sync_tests.rs` 对过：一个节点离线时被两端并进两处、一段被拆了又拆、一个时钟写得早的并入，两种顺序服务端都一样，撤回照样够到；折叠跳过的、body 被扣下的并入照信封记名字，撤回照样拿走它的节点；撤回以后写的并入在每一端作废（`void`），两种顺序都一样，拿不走别的节点；家留着的节点不因为忘掉它并进去的或拆出去的节点而丢；家里的整理不拆、不并被撤回的节点。
- **谁能存什么**按 `myriad_persona_log::holder`（「她的日志」）：电脑只收伙伴私聊和她自己的，群和别人的私聊只在服务端，子机核心不存。

## 一致性对照（`crates/conformance/`）

两份运行时（苹果系的 Swift、其余的 Rust）编排同一份核心规则时，可能编排得不一样；这里放两边都要过的对照。目录按版本分，原生按 `CORE_PIN` 复制进它自己的测试。`nodes/` 现在是 `v2/nodes`（ABI 2：`ledger`、`to`、`takes` 必填，加了家页的两个调用）；`v1/nodes` 留着 ABI 1 时的样子给还钉着它的端，本仓库不再跑它，到没有一端钉它时删掉。`log/` 也一样：`v2/log`（`log.admit`、`log.answer` 的 `holder` 必填），`v1/log` 是 ABI 1 时的样子。其余的还在 `v1/`。

- 每个文件是 `{version, call, cases: [{name, input, output}]}`。`input` 和 `output` 就是那个调用经 C 入口进出的 JSON；输入被拒时 `output` 是 `{"error": "bad_input"}`（不比对消息）。比对按规范字节。
- `log/`：信封、版本向量、收、合并、折叠、时钟；`lease/`：移动、自己停下、epoch 先后；`link/`：帧的检查（`myriad-her-link` 的测试跑它）；`grants/`：三层授予、自治上限、执行前的检查和个人自治授权，每个 `grants.*` 调用一份；`audience/`：一条记忆能在谁面前说；`nodes/`：节点的键和 id、body、折叠、上限、放置、整理队列和整理、在端之间发和收，每个 `nodes.*` 调用一份。
- `v2/nodes/` 由 `myriad-nodes` 的测试经 crate 跑，`MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-nodes` 重写它的 `output`。
- `log/`、`lease/` 由 `myriad-persona-log` 的测试经 crate 跑，`grants/`、`audience/` 由 `myriad-agent-rules` 的 `tests/conformance.rs` 经 crate 跑；`myriad-core-ffi` 的测试都经 C 入口再跑一遍。别的平台照样把每个用例交给 `myriad_core_call`。
- 只随本仓库一次写明理由的改动改：`MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-persona-log` 重写 `log/`、`lease/` 的 `output`；`grants/`、`audience/` 的 `output` 照后端的规则手写（`own` 和批准超出声明照原生，主人 2026-10-27 定），没有重写的开关。不兼容的改动另开 `v2/`，`v1/` 留到没有平台再钉它。

## 宿主过滤（`filter.*`）

客户端看主人的屏幕时，每一段看到的东西（一个窗口、一条消息）落到五种去向之一：`exclude`（不看）、`blind`（看不到，只知道不含文字的事实）、`present_only`（看得到，只影响她在这台设备上的当下）、`who_what`（只带走谁在聊什么）、`carry`（抹过密钥后能带走）。前三种什么都不带走。宿主规则在前，本机模型在后，默认不放行：输入缺了、值不认识、模型不可用、语言不支持、出错、拿不准，都按更严的走，也不改走云端。截取、调用本机模型、界面由客户端做，规则在 `crates/myriad-perception-filter`，来源表在 `shared/perception_sources.json`。

| 调用 | 输入 → 输出 |
| --- | --- |
| `filter.window` | `{bundleId?, title}` → `{seen}`（抹过密钥、压成一行、最多 160 个字符的标题）或 `{withheld: "password_app" \| "key_file"}`：整个窗口都不读 |
| `filter.scan_secrets` | `{text}` → `{text, wiped: [{kind}]}`，`kind` 是 `private_key`、`vendor_prefix`、`bearer`、`assignment`、`flag`（`-u 用户:…`、`mysql -p…`、`--password …`、`sshpass -p`、`docker login -p`）、`url_credentials`（网址里的 `用户:…@`）、`netrc`、`cookie`、`high_entropy` |
| `filter.secret_values` | `{text}` → `{found: [{kind, value, start, end}]}`：`filter.scan_secrets` 会抹掉什么、在哪，和它是同一遍扫描（`secrets::values`），没有第二套规则。每一项是扫描结果里的一个记号，按位置先后；后一条规则吞掉前一条的记号时（`Cookie: a=<密钥>`）只给最外面那一段和它的 `kind`。`start`、`end` 是原文的 UTF-8 字节偏移（不是字符数，也不是 UTF-16 单位），`value` 恰好是原文的 `start..end`；把每段换成记号就是 `scan_secrets` 的 `text`。输出就是密钥本身：只留在内存里，不进日志、文件、模型输入，也不离开设备 |
| `filter.scan_identifiers` | `{text}`（密钥扫描之后）→ `{text, wiped: [{kind}]}`，`kind` 是 `card`、`sg_nric`、`jp_my_number`；命中说明是主人自己的身份信息 |
| `filter.source` | `{bundleId?, urlHost?, market?}` → `{kind}`：`finance`、`health`、`dating`、`job`、`legal`、`adult`、`account`、`government_id`、`messaging` 或 `null`；没给市场或市场不认识时所有市场的网站都算；`table` 只给回归样本用 |
| `filter.page` | `{urlHost, path?, title?, passwordField?, market?}` → `{read, page, basis, kind, category, othersWords}`：网页的正文读不读。`page` 是 `ordinary`、`messaging`（读，`othersWords` 为真）、`unknown`（表里没有的网站，读）或 `sign_in`、`sensitive`、`account`（不读）；`basis` 说是哪条规则：`ordinary_site`（表里的普通网站，`category` 是 `video`、`news`、`reference`、`dev`、`shopping`……）、`docs_host`（`docs.`、`developer.` 这类文档主机）、`messaging_site`、`sensitive_site`、`sensitive_host`（表里没有、域名说明了用途）、`account_path`、`account_title`、`password_field`、`unknown_site`。路径和标题只在本机读，主机名、路径、标题都不进输出 |
| `filter.text_hits` | `{text}`（两道扫描之后）→ `{hits: [{kind, rule}]}`，`kind` 是 `finance`、`health` |
| `filter.check_relay` | `{source, who, topic}`（交给模型转述的原文和它的回答）→ `{ok: true}` 或 `{ok: false, reason: "empty" \| "digits" \| "copied"}` |
| `filter.decide` | 各步的结果 → `{rules, disposition, kind?, reason, missing?, leaves, leavesKind}`；网页另给 `page`（`filter.page` 的 `page`）和 `passwordField` |
| `filter.lists` | `{}` → `{rules, captureExcluded, classifyAsks, presentOnlyLeavesKind, titleLimit, marks}` |

- **形状有版本。** 每个输入可以带 `shape`（默认 1），每个输出都带 `shape`；核心不会说的形状直接拒绝，不猜。`filter.decide` 和 `filter.lists` 还带 `rules`：规则的修订号，词表、名单、判定一改就变，客户端记下是哪一版规则定的去向。
- **新值按更严的读。** `filter.decide` 的枚举都按字符串收：新客户端发来旧核心不认识的值（新的来源类别、新的模型状态），按更严的那一个处理，不拒绝。缺了某步需要的输入是 `blind`（`reason: "missing"`，`missing` 说缺了哪项），也不拒绝。
- **网页按黑名单读。**（2026-10-25 起，原来是「认不出就不读」）看得见密码框的页面、敏感网站（表里的，以及表里没有、但域名本身说明了用途的：域名里有 tax、law、gov 这类词（整段才算），或含 bank、insur、health、clinic、hospital、dating 这类词根（长名字里也算，`mybank`、`healthline`），或以 `.gov`、`.go.jp`、`.bank` 这类结尾，`page::sensitive_host`）、任何网站上的账号页（路径里有 `settings`、`account`、`password`、`billing`、`payment`、`checkout`、`orders`、`login`……，或标题是账号设置、支付方式这类）都不读；其余都读正文，包括表里没有的网站（`unknown`）。读出的正文照样经过文字规则、号码扫描和本机模型的分类再决定去向（`filter.decide`），认不出的网站不再单凭这一点什么都不交出。敏感的那几类永远压过普通（`pay.普通网站` 列在敏感里就是敏感）。
- **敏感的什么都不交。** 来源是敏感类别、网页是账号页或认不出时，`decide` 给 `present_only`，但 `leaves` 是 `nothing`：连「主人在忙私事」的标记也不带，不论 `PRESENT_ONLY_LEAVES_KIND`。看得见密码框是 `exclude`。私密窗口、证件号、文字规则命中仍只交那个不带类别的标记。
- **一致性对照**：`fixtures/host-filter/pages.json`（`filter.page` 加 `filter.decide`，自带 `.example` 来源表）、`terminal.json`（终端里的密钥写法）和 `secret-values.json`（`filter.secret_values` 的位置和原文，含多字节文字、跨行私钥、被吞掉的记号），crate 直接跑一遍、经 C 入口再跑一遍；别的客户端按钉住的提交复制这三份，经 C 入口逐条比对。
- **拒绝不带原文。** 这些调用读的文字里可能有密钥，所以 `bad_input` 的消息只说缺了或多了哪个字段，或者输入不合形状，从不引用输入里的值（`filter.secret_values` 也一样）。输出也不带被抹掉、被命中的原文和它的位置；唯一的例外是 `filter.secret_values`，它的用处就是把这些值交给客户端在别处遮掉。
- **还没定的两件事各是一个常量。** 敏感分类问几次（`CLASSIFY_ASKS`，现在问两次（通用、内容标注），两次都明确不敏感才 `carry`）、`present_only` 离开设备的标记带不带类别（`PRESENT_ONLY_LEAVES_KIND`，现在不带）。现在取的都是更严的那一边；客户端按 `filter.lists` 的 `classifyAsks` 决定问几次。
- **回归样本**在 `crates/myriad-perception-filter/fixtures/host-filter/`：原生仓库合成样本的副本（密钥和号码是占位符，测试在运行时用固定种子展开），加上本机模型在这批样本上录下的回答（`model-replay.json`）。`cargo test -p myriad-perception-filter --test corpus -- --nocapture` 打印真规则＋录下的模型回答按 `decide` 合成后，每类放过和收紧了多少。

## 别的平台怎么链接

不从工作区构建：工作区里常有别人还没提交的改动。按本仓库的一个提交构建：

1. 把这个提交 `git archive` 到一个隔离目录（整个仓库就是要构建的全部：crate、`shared/` 里编译期读的契约、`Cargo.lock`、`rust-toolchain.toml`）。
2. 设 `MYRIAD_CORE_UPSTREAM_COMMIT=<提交>`（取归档自己记录的提交号，`git get-tar-commit-id`），`cargo build --release --locked --target <triple> -p myriad-core-ffi`。变量名和 `meta.version` 的 `upstreamCommit` 沿用拆出来之前的名字，指的是本仓库的提交。
3. 链接 `libmyriad_core_ffi.a` 和头文件；启动或测试时断言 `meta.version` 的 `upstreamCommit` 就是钉住的提交。

用 Rust 的端（站点后端，以后的其他客户端）不经 C 入口，直接依赖 crate。

macOS 原生的做法：原生 App 仓库的 `CORE_PIN`、`scripts/build-core.sh` 与 `docs/CORE.md`（只有 Command Line Tools 时手工拼 xcframework 目录）。冷构建约 4 分钟，静态库约 50 MB（带着整个 std 与 LTO 后的依赖），链接后只留下用到的部分。iOS 的静态库由持续集成构建（`.github/workflows/ci.yml`），还没有一端链接过。
