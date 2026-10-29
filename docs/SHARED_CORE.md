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
| `sync.manifest` | `{envelopes}` → `{manifest}`：每类实体、向量、文件的条数和 digest，与读的顺序无关 |
| `sync.persona_diff` | `{before, after, revs, legacyRev, rev, origin, reason?}` → `{changed, revs, history}`：人设一次写入逐字段比（字段没了算 `null`），变了的字段记成这次的 `{rev, origin}`，被换掉的旧值连同它原来的 rev 进 `history`（原来就没值的不进）；没有 rev 的旧字段按 `legacyRev`、`site` 算；`reason` 默认 `edit` |
| `sync.rig_package_id` | `{atlas: base64, manifest}` → `{id}` |

格式里几条平台要照做的规矩：

- **第 2 版的几类只在搬家和导出时出现。** `conversation`（站长的一场私聊连同每一句，含「没听完」标记和句子里的站点图片）、`proactive_line`、`intention`、`owner_state`、`pace`、`present`、`wearing`、`field_history` 是第 2 版加的。首次比对的 pull 只发前 13 类（`Kind::COMPARED`），`sync.compare` 也只读它们。
- **导出文件 `myriad.her-archive`。** 第一行是明文头（规范 JSON），之后是分块的 XChaCha20-Poly1305：nonce 为头里的 19 字节前缀 + 块序号（u32 大端）+ 最后一块标记，头这一行是每块的附加数据。截断、换块、改头都打不开，口令错和文件坏说的是同一句话。流里依次是信封页、它们引用的文件（rig 是 `manifest.json` 加 `atlas.png`）、清单。导出文件总是加密。盐和 nonce 前缀由调用方从系统随机源取来传入，核心不读随机数。黄金文件在 `fixtures/archive/`。

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

几条各端要照做的规矩：

- **合并是并集。** 同一个 `id` 只算一次；桩赢过它原来的那一行；同一个 `id` 内容不同、或同一受众里同一端的同一个 `dseq` 被两行占了，都是格式被写坏了，留下已有的、报 `conflicts`。`log.merge` 的 `stubbed` 是本端变成桩的行：删它们的载荷。
- **收一批行**（`log.admit`）：有一行的时钟超前本端 5 分钟以上，整批不收、什么都不吸收（`clockAhead`）；其余每行单独判：信封不完整、`local`、对象换了受众、主体不在群里的拒收并计数。对象原来在哪个受众由调用方传 `known`，群成员传 `members`（不传就不查）。同一端她的行在同一个受众里，epoch 沿它自己的顺序只增不减：比本端在这个受众里已收的那一端最高 epoch 低的拒收（`epoch_back`）；已收的最高 epoch 由调用方按 `{受众: {端: epoch}}` 传 `epochs`，回答里带收下这一批之后的。按受众判，是因为各受众分开计数、分开发，一端单机时期的行可能在它另一个受众后来的行之后才到。账上的对象收成桩，版本向量照样补齐。
- **忘掉是吸收的。** `forget` 行的 `object` 是被忘掉的对象，或 `subject:<id>`（这个受众里关于这个人的全部）；它自己没有载荷，也永远不会被忘掉。账只有受众、目标和时间（`Entry`），没有一个字的内容。不管 `forget` 排在对象的行之前还是之后、从哪一端先到，被忘掉的对象都折叠成只剩 `forgotten: true`。`log.answer` 永远先发 `forget` 行。
- **折叠**（`log.fold`）：按 `(hlc, origin, dseq, id)` 排；发生过的事（看到、判断、说了的话、段和段里每一句、判例、记忆写入、心情变化）都留着；会变的值（节点上她的看法 `node.view_changed`、一件事挂在哪 `link.attach`、人设字段、衣服）最后一行是现在的，之前的进 `history`；记忆退役是单向的，除非之后有一次恢复。级联（给一个对象算出要连带删的派生内容）还没有。
- **执笔权**（`lease.*`）只由服务端发，epoch 只增，只有三种移法：持有者自己交（`handover`，带它在每个写过的受众里最后的 `dseq`：`through` 是 `{受众: dseq}`，没写过的受众不列）；服务端接（`takeover`）：持有者失联满 3 分钟、主人的位置证明主人在另一个场所（两边的场所都要知道、主人的位置 3 分钟内看到过、而且持有者失联和主人离开都过了 3 分钟）；主人按「让她来这里」（`force`）。`lease.here` 是持有者自己执行的那条：和服务端断开、主人又不在这个场所（本机输入或主人的设备经局域网连上来）满 3 分钟，它自己停下，不再想、不写她的行，只收主人写的、做完手上的事。服务端接管时它一定已经停下，所以不会有两个她；`holderLastSeen` 要算到持有者自己还以为连着的最后一刻（最后一次收到它，加上它判断断线用的心跳超时）。时间都是初值。其中「失联、主人又不在多久算停下」是服务端的设置（`stopAfter`，微秒，1 分钟到 1 天，不给是 3 分钟）：服务端用它判断接管，并随 `lease.grant`、`lease.epoch` 把同一个值交给持有者，持有者用它跑 `lease.here`。两边是同一个值，所以服务端永远不会比持有者停下得早。
- **单机时期是第 0 代。** 一端还没见过任何执笔权（单机用、没连过服务端）时，她就在那里想（`lease.here` 不传 `lease`，答 `thinking`），她的行写 `epoch: 0`；服务端发的第一份是 1，没有哪一份是 0。这些行在每个受众里占着那一端自己的 `dseq`，第一次配对或同步时照原样整段带上去，在那个受众里排在它后来的行之前；从它知道第一份执笔权起，不再写 0，再写就被 `epoch_back` 拒收。
- **时钟不能被一次跑快带偏**（`log.clock_*`）：日志的时钟要存下来、只增不减，所以机器的钟跑快时盖的一行会让之后每一行都超前，服务端拒收超前 5 分钟以上的（`hlc::check_ahead`），这一端就一直同步不了。三条规则：时钟的物理部分用「校正后的时间」，即本机时间加上和服务端量出的偏差（`log.clock_offset`：请求发出、服务端的 `serverTime`、收到回答，取中点），偏差由调用方存着、每次握手和每次 `clock_ahead` 时重量；盖章时物理部分最多到校正后的时间加 5 分钟，再往后只走计数（`log.clock_next`、`log.clock_observe`），已经超出的时钟原地不动、`poisoned` 为真；修复（`log.clock_repair`）只给还没离开这一端的行重新盖章，从第一条超前的开始按日志顺序往后，并给出以后要存的时钟。离开过这一端的行永远不重盖：服务端收了它，它就没有超前。收到 `clock_ahead`（带 `serverTime`）时这一端的顺序是：量偏差，修复没同步的行，存新时钟，再发。
- **两个她只可能来自 `force` 和单机时期。** 折叠时对照执笔权的历史（`leases`）：旧 epoch 的行写在 `force` 签出之后的，是原来那台还在跑时写的，照样留下，标 `concurrent`，碰到的对象也标；第 0 代的行写的时候别的端已经持有执笔权的（签出时间早于这一行、持有者不是它自己），是她在别处住着时这台被单机用了，同样标 `concurrent`，`with` 是当时那一份；那时还没有任何一份、或那一份就是它自己的第一份（时钟稍快会让单机的最后几行排在签出之后）的，是同一个她，不标；`takeover` 之后迟到的旧行是她自己的记忆补上来，不标；不是那个 epoch 的持有者写的（`not_holder`）、交出以后还写的（`after_handover`）是执笔权的 bug，列在 `standing` 里查。同一个 epoch 出现两次时，先签的算，再按持有者的字节序（`lease.newer`）。

| 调用 | 输入 → 输出 |
| --- | --- |
| `lease.here` | `{me, lease?, now, disconnectedSince?, ownerHereAt?, stopAfter?}` → `{state: "thinking"}`、`{state: "stopped", stoppedAt}` 或 `{state: "child"}`。还没见过任何执笔权的那一端不传 `lease`：单机，`thinking`，她的行写 `epoch: 0` |
| `lease.move` | `{lease, request, now, issued, holderLastSeen?, holderPlace?, ownerPlace?, stopAfter?}` → `{granted: Grant}` 或 `{refused: 理由}`；`request` 是 `{handover: {from, epoch, to, through?}}`、`{takeover: {server}}` 或 `{force: {to}}`；`issued` 是服务端给新一份的 rev，必须晚于现在这份 |
| `lease.newer` | `{a, b}` → `{newer: "a" \| "b"}` |
| `log.admit` | `{rows, now, known?, members?, ledger?, epochs?}` → `{accepted, refused?, clockAhead?, ledger, epochs}` |
| `log.answer` | `{rows, theirs}` → `{rows}`：一个受众里对方（持有这个受众的 `theirs`）缺的行，`forget` 在前；行跨了受众是 `bad_input` |
| `log.canonical` | `{row}` → `{row, canonical, digest}`；信封不对就是 `bad_input` |
| `log.clock_next` | `{last?, now, offset, node}` → `{rev, poisoned}`：`now` 是本机时间，`offset` 加上它是服务端的时间 |
| `log.clock_observe` | `{last?, seen, now, offset, node}` → `{last}`：`seen` 最多把物理部分推到上限 |
| `log.clock_offset` | `{sent, serverTime, received}` → `{offset, uncertainty}`（微秒） |
| `log.clock_repair` | `{last, floor?, unsynced: [{id, hlc}], now, offset, node}` → `{last, restamped}`；`floor` 是确知别处收过的最新 rev |
| `log.fold` | `{rows, leases?}` → `{objects, standing?}`：每个对象的 `rows`、`current`、`history`、`retired`、`concurrent`，或只有 `forgotten` |
| `log.holdings` | `{rows}` → `{vector, beyond?}`：一个受众的行里每一端连续持有到第几条，和跳号之后先到的；行跨了受众是 `bad_input` |
| `log.merge` | `{log, incoming}` → `{log, added?, stubbed?, duplicates, conflicts?}`；`log` 可以只给和这批有关的行 |
| `log.missing` | `{mine, theirs}` → `{missing: [{origin, from, through}]}` |

## 她的连接的帧（`link.*`）

「她的连接」的协议定义是 `shared/her_link_contract.json`。协议 2（契约里的 `next` 块，2026-10-23 定，还没有主机在说，服务端那一侧是 S6）让它成为每台设备唯一保持的连接，加了 `fact`、`hand`、`say`、`lease`、`log`、`peer` 六个帧族，信封多一个 `v`（帧格式版本，1 是规范 JSON 文本）。结构见 Myriad 的 [ARCHITECTURE.md](https://github.com/Myriad-You/Myriad/blob/preview/docs/development/ARCHITECTURE.md)「Her link and her devices」。

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

节点是她记在心上的一件事。规则在 `crates/myriad-nodes`，服务端和每台电脑都用它，所以同一串日志行在哪一端都折出同一个节点。这是第一批：节点是什么、装什么、行怎么写、怎么折、放置的输入输出长什么样。整理（合并、拆分、收件箱归位、歇着）、唤醒、看法什么时候变、反对、只忘掉一段，以及同步的传输都不在这一批。

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

- `because` 是依据的 id（`span:<行>`、`entry:<行>`、`message:<id>`……），至少一个：概括的每一句、看法、没收尾的事、想说的话、改种类和关系都要有。
- `base` 是写的时候看到的那一版（行 id）；第一次写是空的；两边都改过、她合起来时列出两版。

**折叠**（`nodes.fold`）：输入任何顺序、可以重复、可以有桩，加上够到它们的 `forget` 行，结果和顺序无关。

- 头里每一项（种类、名字、概括、状态、谁知道、对象、每人的进度、每条看法、每件没收尾的事、每句想说的话）分开算：被别的行写进 `base` 的版本是旧的，进 `history`（留最新 20 个）；剩下的是当前的。
- **两边都改过**：当前的不止一个、而且说的不一样，就是两台设备从同一版各改了一次、互相没看见。这一项标 `bothChanged`，显示日志顺序里先写的那一个，其余的列在 `rivals` 里。后写的不会自动盖掉先写的；下一次有一行把所有当前版本都列进 `base`，才算合好。每一项都带 `base`：当前所有版本的行 id（排好序），就是下一次写这一项时该填的 `base`。两边从同一版改成一样的不算冲突，`base` 里两行都列着；之后哪一行只列了其中一个，也算两个都看见了。
- 经历和条目按日志顺序都留着，各带三个标签。`history` 列出每次改种类：从什么、到什么、为什么。
- 不合规矩的行跳过，并在 `skipped` 里写明原因：`no_body`、`bad_body`、`other_node`（created 的受众和键算出别的 id）、`sync_mismatch`、`author_mismatch`、`audience_tag`、`not_in_audience`（谁知道和进度只能填这个受众里的人）、`untrusted_intent`（想说的话只能来自伙伴的话或她自己）、`no_spans_here`（「关于你」只放条目、不挂经历）、`kind_fixed`、`status_for_kind`（人没有做完）、`object_for_key`（仓库对象只在仓库键上，仓库节点的对象拿不掉）、`self_relation`、`cycle`。这一批不读的 node 种类（原生旧的行、以后整理的行）只算一次变化，不读内容。
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

**放置**（`nodes.place_input`、`nodes.place_read`）：一段对话告一段落时，她把它切成一段一段，每段只讲一件事，挂到一个节点上或进收件箱。挂哪里由她判断（模型）；这里只定发给模型的东西怎么截、模型的回答怎么读，两边主机才会一样。提示词是 M2 的事。

- `place_input`：条目按顺序取，每条截到 600 字，最多 40 条、合计 12000 字（第一条一定要；`taken` 是取了几条，剩下的下一次再放）；候选节点按给的顺序最多 12 个，名字截到 40 字、概括 160 字，编号 `n1`、`n2`……，节点 id 不给模型。截断按 Unicode 标量计数，超出的以 `…` 结尾；控制字符换成空格。
- `place_read`：读 `{"spans":[{"start":1,"node":"n2","gist":"…"}]}`。条目从 1 编号，每段从自己的 `start` 到下一段之前；第一段从 1 开始，`start` 只增；`node` 是编号，`null` 或不写就是收件箱；`gist` 是她给这段的几个字，截到 120 字。回答包在代码块里或前后有话都能读。别的地方不对（不是 JSON、没有段、段比条目多、`start` 不对、编号不认识、没有 `gist`、超过 16384 字节），整段作为一段进收件箱，`fallback` 写明原因：什么都不丢，也不猜。

所有长度和条数都在 `myriad_nodes::limits`，`nodes.limits` 原样给出。它们是初值，改在这里就是改所有端。

| 调用 | 输入 → 输出 |
| --- | --- |
| `nodes.body` | `{kind, body}` → `{body, canonical, digest}` |
| `nodes.fold` | `{rows, bodies}` → `{nodes, relations?, stubbed?, skipped?}` |
| `nodes.id` | `{audience, key}` → `{id}` |
| `nodes.limits` | `{}` → 每个上限 |
| `nodes.place_input` | `{items: [{id, saidBy, text}], candidates?: [{id, kind, name, summary?}]}` → `{items: [{n, id, saidBy, text}], taken, candidates?: [{label, id, kind, name, summary?}]}` |
| `nodes.place_read` | `{items: [id], candidates?: [{label, id}], output}` → `{spans: [{items, node?, gist}], fallback?}` |
| `nodes.repo_key` | `{remote}` → `{key}` |

## 一致性对照（`crates/conformance/`）

两份运行时（苹果系的 Swift、其余的 Rust）编排同一份核心规则时，可能编排得不一样；这里放两边都要过的对照。目录按版本分，现在是 `v1/`，原生按 `CORE_PIN` 复制进它自己的测试。

- 每个文件是 `{version, call, cases: [{name, input, output}]}`。`input` 和 `output` 就是那个调用经 C 入口进出的 JSON；输入被拒时 `output` 是 `{"error": "bad_input"}`（不比对消息）。比对按规范字节。
- `log/`：信封、版本向量、收、合并、折叠、时钟；`lease/`：移动、自己停下、epoch 先后；`link/`：帧的检查（`myriad-her-link` 的测试跑它）；`grants/`：三层授予、自治上限、执行前的检查和个人自治授权，每个 `grants.*` 调用一份；`audience/`：一条记忆能在谁面前说；`nodes/`：节点的键和 id、body、折叠、上限和放置，每个 `nodes.*` 调用一份。
- `nodes/` 由 `myriad-nodes` 的测试经 crate 跑，`MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-nodes` 重写它的 `output`。
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
