# 接上她：给 iOS 和桌面端作者的接入说明

这一页讲一个新的端怎么用协议 2 和子机核心（`myriad-link-child`）接上她。它是一张路线图：每一步做什么、调哪个调用、规则写在哪。规则本身只写在两处，这一页不重抄：

- 线上的协议：`shared/her_link_contract.json`（下面叫「契约」）。每种帧的说明在 `frames.<类型>.description`。
- 核心的调用和各端要照做的规矩：`docs/SHARED_CORE.md`（下面叫「核心文档」）。

写这一页时核心的 main 是 `1032d1329`，`ABI_VERSION` 是 4，契约 `protocol` 是 2、`schemaVersion` 是 3。你钉哪个提交，就以那个提交上的契约和核心文档为准。

没核实的地方标了 uncertain。

## 先定你是哪一种端

她只有一个主机。每个拨到主机的端都是子机，hello 里的 `role` 一律写 `child`（契约 `frames.hello`）。在这之上，一个端做多少事分四层，后一层包含前一层：

| 层 | 这一端做什么 | 要读的节 | 核心里用到的 |
| --- | --- | --- | --- |
| 1 | 只当子机：聊天、她的形象、通知 | 1、2、7、9 | `link.child_*` |
| 2 | 加上调用：主机能让这一端做事，这一端能调主机 | 加 6 | `link.child_*`，或自己的连接加 `link.call_receive` |
| 3 | 存一份她的日志，和主机同步 | 加 3、4 | `log.*`、`nodes.wire_send`、`nodes.wire_take`、`partner.*` |
| 4 | 她能住在这一端（持笔），或在主机够不着时当临时主机 | 加 5 | `lease.*` |

哪一种端能存日志，核心有规定（`myriad_persona_log::holder`，`crates/myriad-persona-log/src/holder.rs` 的 `Core`）：`pc`（伙伴的电脑）存伙伴自己的私聊和她自己的；`server` 存全部；`child`（核心文档写的是「手机、XR 这些子机核心」）不存日志。所以：

- 有两种 `lease` 帧按契约不只属于第 4 层：`lease.place`（`from` 是 `device`，任何一端都报自己在哪个现场、主人在不在）和 `lease.epoch`（「发给每台设备」，设备靠它显示她在哪）。它们是同步族的帧，所以没开口子的第 1、2 层的端发不了 `lease.place`（`passthrough_off`），收到 `lease.epoch` 时只会拿到 `unread`。uncertain：站点会不会给不存日志的子机发 `lease.epoch`、要不要它报 `lease.place`，这个仓库里核实不了；要做「显示她在哪」的第 1、2 层的端，先问站点一侧。
- **桌面端**是 `pc`，四层都可能做。
- **iOS** 按今天的规定是 `child`，做第 1、2 层。iOS 要不要做第 3、4 层不是这一页能定的：那是改 `holder` 的规定，等于改所有端。uncertain：我没找到关于 iOS 的单独决定。

## 1. 连接和握手

**你不用自己写这一层。** 握手、编号和确认、心跳、可靠帧的重发、回合续传、关闭码，子机核心里有一份现成的，各端共用。你写的是外壳：开套接字、读时钟、存设备令牌，把发生的事喂给会话，照它交回的去做。

外壳的循环、容易错的地方、Rust 和 Swift 两种接法的代码，在核心文档「子机核心（`myriad-link-child`）：各端怎么接上她」。这里只列你要自己给的东西：

| 事 | 值 | 出处 |
| --- | --- | --- |
| 地址 | 站点的 `/api/agent/link`，WebSocket 升级 | 契约 `path` |
| 凭据 | 请求头 `Authorization: Bearer <设备令牌>`，令牌以 `mys_` 开头，是站点主人在站点上给这台设备发的 | 契约 `auth` |
| 升级被拒 | 普通的 HTTP 应答 `{error, code}`。把状态码和应答正文交给 `link.child_after`（Rust：`Session::after_http`），它说重连、停下还是删凭据 | 契约 `refusals` |
| 套接字关了 | 把关闭码和理由交给 `link.child_after`（Rust：`Session::after_close`） | 契约 `closeCodes` |
| 重连等多久 | `link.child_after` 带 `attempt` 时给 `backoffMs`（Rust：`contract().timing.backoff_ms(attempt)`） | 契约 `timing` |
| 时间 | 全由外壳给。Rust 接口是毫秒，C 入口是微秒，差一千倍，传错了不会报错 | 核心文档 |

握手是会话做的：连上后它发 `hello`（`protocol`、`role`、`surface`、`capabilities`、`app`、上次听到的 `lineage` 和 `hostEpoch`），主机回 `welcome`。你从 `welcome` 交付里拿到：

- `deviceId`、`host`（她住在哪：`site`，或这台设备自己是她的住处时是 `app`）。
- `lineage`、`hostEpoch`：存下来，下次新建会话时传回。
- `sync`：`on` 或 `off`，同步族在这条连接上走不走（第 4 节）。
- `partner`：伙伴的 id，主机没说就是 `null`（第 4 节）。

主机接受了你的哪些能力，在每一步回答的 `hostCapabilities` 里（Rust：`session.host_capabilities()`）。

几个关闭码要对主人说对话（契约 `closeCodes`，「重连」一栏是它的 `reconnect`）：

| 码 | 名字 | 外壳做什么 |
| --- | --- | --- |
| 4001、4002 | `revoked`、`owner_changed` | 删掉凭据，不再连 |
| 4003 | `replaced` | 同一台设备的另一条连接顶替了这一条。不自动重连 |
| 4004、4005 | `host_moved`、`host_frozen` | 她搬到别的端了，或正在搬。重连 |
| 4406、4415 | `unsupported_protocol`、`version_unsupported` | 这个 App 和站点的版本对不上，有一边要更新。不重连，不是网络问题。说了 `versions` 的端以先到的 `verdict` 交付为准：是哪一边旧了它说得更清楚 |
| 4408、4503 | `timeout`、`unavailable` | 重连 |

你不用自己查这张表：`link.child_after` 的回答里 `then` 是 `reconnect`、`stop` 或 `forget`。前两种带 `down.why`（原因）；`forget` 带 `removal`（凭据为什么没了）。

## 2. 能力声明

hello 里的 `capabilities` 是这一端能做什么。主机照它发帧，所以**说了的每一项，带来的每种帧你都要有处理代码**。

- 清单从契约的 `capabilities` 里选，别的名字主机会丢掉。
- 每个端放一道测试：`link.child_claims`（Rust：`myriad_link_child::capabilities::unread`）给出「声明了却不读的帧」，断言它是空的。规则在核心文档「能力清单要是真的」。
- 会话自己读的主机帧是 `Session::READS`。别的帧（第 3 节）要开口子才到得了外壳。
- 这道测试管不到同步族的帧。`log.vector`、`log.rows`、`lease.grant`、`lease.epoch`、`lease.takeover` 不跟着哪项能力走，跟着 `welcome.sync` 走，所以不在那张「能力带来哪些帧」的表里（同步族里表上有的是跟着能力走的那几种：`deputy` 带来的 `lease.deputy`、`log.full` 带来的 `log.full`、`lend` 带来的借话帧）。第 3、4 层的端要自己保证这几种帧都有处理代码。

和后面几节有关的能力：

| 能力 | 说了以后 |
| --- | --- |
| `versions` | 连上后报这一端的版本（`link.versions`），主机回结论 |
| `calls` | 调用（第 6 节） |
| `streams` | 能报值和事；这一端读不了的帧自动报给主机（`link.dropped`） |
| `partner` | `welcome` 里带伙伴的 id |
| `log2`、`log3`、`log4` | 这一端的核心读得懂哪一代的日志行。说你钉的核心的 `ABI_VERSION` 对应的那几项（ABI 4 就三项都说）。对方按你说的最高一项决定发不发新种类的行（`nodes.wire_send` 的 `peerAbi`） |
| `log.pages` | 日志一页一页地同步（第 4 节） |
| `log.full` | 第一次全量同步可以走 HTTPS 下载（契约 `frames.log.full`） |
| `deputy` | 这一端能当临时主机；`lease.grant`、`lease.deputy` 要用 `lease.taken` 回执（第 5 节） |

## 3. 会话不读的帧：开口子

会话只读聊天和形象这一批帧，加上 `call.*`、`stream.*`、`link.*`。日志、租约、`nodes`、`proposals`、`act.request` 这些它不读，也不会逐族做进去。第 3、4 层的端要开一个口子：

- 开：`link.child_new` 的 `passthrough: true`（Rust：`session.passes(true)`）。
- 收：会话不读的主机帧以 `frame` 交付交给外壳，带类型、id、序号、第几条连接、送达方式。内容就是你刚交给 `received` 的那段文本，自己读。
- 确认：`delivery` 是 `reliable` 的 `frame`，你收下后要交事件 `took`。没确认之前，这一端发出的确认号停在它前面。
- 发：事件 `send`，会话替你编号、过契约检查。可靠帧默认由会话留着重发；带 `keep: false` 的发一次、不留，重发归你。
- 同步族（`lease`、`log`、`group`、`chat`）的帧只在 `welcome.sync` 是 `on` 的连接上发，会话替你拦。

全部规则在核心文档「会话不读的帧：交给外壳，外壳的帧代发」。两条要记住的：

- **卡确认号的帧必须尽快了结。** 确认号是累计的，一帧没确认，后面所有帧的确认都被它卡住。落库慢就先落一个「已收下」的记号再确认。
- **等确认的帧多了就放慢读套接字。** 等确认的最多 256 个，满了这条连接会被断开（`behind`）。

## 4. 日志同步（第 3 层）

每个存日志的端各存一份她的只追加日志，端和端之间交换版本向量、互发对方缺的行。规则在核心文档「她的日志（`log.*`、`lease.*`）」和「她的节点」里的「在端之间」。

### 开始之前

- `welcome.sync` 是 `off` 就什么都不做：两边都不发同步族的帧。
- 伙伴的 id 只从 `welcome.partner` 拿（`partner.link`）。第一次同步前，一台还没迁过的设备要把手里的行迁到这个 id 下（`partner.migrate`）。规则在核心文档「伙伴的 id（`partner.*`）」。
- 时钟：日志的时钟要存下来、只增不减，而且对着服务端的时间。偏移用 `log.clock_offset` 算（`{sent, serverTime, received}`，微秒），写行时用 `log.clock_next`，看到别处的时钟用 `log.clock_observe`。服务端拒收超前 5 分钟以上的批（错误 `clock_ahead`，带服务端的时间）。规则在核心文档「时钟不能被一次跑快带偏」。uncertain：一个只走这条连接的端拿哪一次往返来量偏移（`welcome.serverTime` 是毫秒，只有一个时刻），我没在核心里找到写明的做法，请照 macOS 端现在的做法。

### 一轮同步

对你存的每个受众：

1. **报你有什么。** `log.holdings` 从你的行算出 `{vector, beyond?}`，发 `log.vector`（`{audience, vector, beyond?, ask?}`）。`welcome` 之后每个受众发一次。
2. **对方发你缺的。** 它发来 `log.rows`（可靠帧）。每一批交给 `nodes.wire_take`，它做完全部检查，告诉你收下了哪些、拒了哪些、哪些 body 要再要（`ask`）。收下的用 `log.merge` 并进你的日志。落库后对这一帧 `took`。
3. **你发对方缺的。** 收到对方的 `log.vector` 时，用 `nodes.wire_send` 算出要发的批，每一批是一条 `log.rows`。它要的输入：这个受众和你的行、对方的向量 `theirs`、对方的 `ask` 和 `beyond`、对方是谁（`holder`）、你自己的 id（`here`）、你的账（`ledger`）、对方的 `peerAbi`。对主机的 `peerAbi` 从它在 `welcome` 里接受的能力算：有 `log4` 是 4，有 `log3` 是 3，有 `log2` 是 2，否则 1（契约 `frames.welcome`）。给错了要么什么都不发（`tooOld`），要么发出对方读不懂的行。`log.rows` 可以用 `keep: false` 发：它丢了能从下一轮的 `log.vector` 补回来，不必压在会话的待发箱里。
4. **对方有你没有的，就回问。** 收到的 `log.vector` 显示对方有你缺的行时（`log.missing` 给 `{mine, theirs}`，答的是 `theirs` 有而 `mine` 没有的），回一条你自己的 `log.vector`，对方才知道该发什么给你。契约的原话是 `log.vector` 也「作为对一条 `log.vector` 的回答」发。uncertain：契约没写怎么避免两边来回互发，我没读站点和 macOS 端的做法；至少不要在两边向量相同的时候再回。

### 一页一页地翻（`log.pages`）

两边都说了 `log.pages` 的连接上，缺的行一次只发一页，最多 16 批（契约 `limits.logPageBatches`；核心的 `wire::PAGE_BATCHES`）。规则在契约 `frames.log.vector` 和 `frames.log.rows`，下面是两边各自要做的：

**你是收的一边：**

- 一页的最后一批带 `more`（这一页的 id）。最后一页的最后一批不带。
- 你把带 `more` 的那一批了结之后（并进去了，或者整批不收），再发一条 `log.vector`，`after` 写这个 id。只在这时写 `after`，别的原因发的 `log.vector` 不写。契约里「回绝」说的是用 `error` 帧回，而 `error` 只有主机能发：走会话的子机了结这一批就是 `took` 它，然后发带 `after` 的 `log.vector`。uncertain：这样的端整批不收时要不要另外告诉主机，契约没写。
- 对方收到带 `after` 的才发下一页。

**你是发的一边：**

- 给 `nodes.wire_send` 传 `page`（16）和对方的 `beyond`。它最多出这么多批；回答里 `more` 说还有没发完的，`askLeft` 是这一页放不下的「要求再发的行」，你留着，下一页作为 `ask` 给进去。
- 这一页的 id 由你造，写在这一页最后一批的 `more` 上（核心不替你写）。一个 id 只用于一页、一条连接。
- 一页最多 16 批，「要求再发的行」也算在里面：先发向量和 `beyond` 显示缺的，再发要求再发的（上一页剩下的在前，再是这一条 `log.vector` 自己的 `ask`，一次最多 64 个，其余留到下一页）。
- 一页没翻完（它的 `more` 还没被一个 `after` 点名）时，只有 `after` 点名这一页的 `log.vector` 才换来下一页；别的 `log.vector` 不发行。重复的、过时的 `log.vector` 不会让你把行再发一遍。
- **例外：没有进展就停。** 一条 `log.vector` 的 `after` 点名了没翻完的那一页，可它的 `vector` 和 `beyond` 和当初要这一页的那一条一模一样，而这一页里有它缺的行——说明对方把这一页的行拒了。这一页就算翻完，后面不再跟页；等下一条不带 `after` 的 `log.vector` 再从头答一页。不这样做，你会把同一页被拒的行一直重发。只有「要求再发的行」的一页不改向量，照样算有进展。
- 翻页每条连接从头来。

没说 `log.pages` 的连接上，和以前一样一次发完，没有 `more`、`after`。

### 第一次全量

说了 `log.full` 的端，第一次同步时服务端可以回一条 `log.full`：一个一次性的 HTTPS 下载地址和它持有的向量。下载、并进去以后再发 `log.vector`，之后的增量走连接。没说的端，同样的行走 `log.rows`。

## 5. 租约和临时主机（第 4 层）

任何时刻只有一个她在想。她在哪一端想，由租约（执笔权）定，只有服务端发。规则在核心文档「她的日志」里的「执笔权」和「临时主机（代理凭证）」，帧在契约的 `lease.*`。这些帧都属于同步族，走第 3 节的口子。

**你每时每刻该是什么状态，问 `lease.here`。** 输入是 `{me, lease?, now, disconnectedSince?, ownerHereAt?, stopAfter?, stoppedAt?, deputy?, hostUnreachableSince?}`：这一端的 id、站着的那份租约、现在的时间。`disconnectedSince`、`ownerHereAt`、`stoppedAt`、`hostUnreachableSince` 是时刻，不是时长；`stopAfter` 是时长（停下时间，服务端随 `lease.grant`、`lease.epoch` 给，1 分钟到 1 天，不给是 3 分钟）；`deputy` 是凭证。时间都是微秒，和第 6 节调用用的毫秒不同。它答 `thinking`（她在这里想）、`child`、`stopped`（持有者自己停下了）、`frozen`（正在搬家）或 `interim`（临时主机）。不要自己推。

- `lease` 要给站着的那一份的全貌 `{epoch, holder, issued, how, from?, move?}`。说了 `deputy` 的端从 `lease.epoch` 的 `issued`、`from` 拼得出来。
- `now` 是你写行用的那只时钟读出的时间，不能落后于真实时间。
- `disconnectedSince` 是和服务端断开的时刻，只用于持有者自己停下；`hostUnreachableSince` 是够不着持笔主机的时刻，只用于临时主机。两个不是一回事。

帧和你要做的事：

| 帧 | 方向 | 你做什么 |
| --- | --- | --- |
| `lease.place` | 你发 | 这台设备在哪个现场、主人在不在用它。变了就发，`welcome` 之后也发 |
| `lease.ask` | 你发 | 请服务端把她交到这一端：`why` 是 `talk`、`place` 或 `force` |
| `lease.grant` | 服务端发 | 你是新的持有者。先用 `lease.behind` 看还差哪些行没补到，补齐了再开始想 |
| `lease.epoch` | 服务端发 | 她现在在哪。你手里的任期比它旧就立刻停下，变回子机 |
| `lease.yield` | 你发 | 你把她交出去（要睡了、主人走了、服务端要）。带上你在每个受众里最后的 `dseq` |
| `lease.stopped` | 你发 | 你断线后自己停下过，重连时报告什么时候停的 |
| `lease.takeover` | 服务端发 | 你不在时服务端把她接走了。你变回子机，把之后写的行发上去 |
| `lease.deputy` | 服务端发 | 给你一张代理凭证（见下） |
| `lease.taken` | 你发 | 对 `lease.grant`、`lease.deputy` 的回执（见下） |

### 临时主机

主机够不着的时候，她可以在一台预先拿到凭证的端上临时想。

- 说了 `deputy` 能力的端，服务端会预先发 `lease.deputy`：一张凭证 `{holder, under, issued, until, order}`，最多管一天（契约 `timing.deputyMaxMs`）。留着最新的一张。
- 把凭证作为 `deputy`、够不着主机的时刻作为 `hostUnreachableSince` 交给 `lease.here`。少了 `hostUnreachableSince`，它永远答 `child`。
- 四样都满足它才答 `interim`：够不着主机满了停下时间；主人此刻在这一端；凭证是给你的；凭证的 `under` 等于站着的那一任，而且没过期。这时她在这里临时想，知道自己是临时的，写的行用 `under` 那一任。
- 连回主机、凭证过期、或主人离开满了停下时间，答案回到 `child`（不是 `stopped`）。临时写的行照常发上去，保留，标为同时发生的。
- 凭证不是租约：它不移动执笔权。

### 回执

说了 `deputy` 的端，`lease.grant` 和 `lease.deputy` 不靠确认号算送达，靠你回的 `lease.taken`（`{of, id}`：是哪一种、它的 id）。

- 读通并存下这一帧之后回 `lease.taken`，并照常 `took`。两件事都要做。
- 这一帧再来一次，就再回一次：服务端重发是因为没听到上一次的回执。经会话的口子时，再来的那一次 `frame` 交付里 `again` 是 `true`，不用再 `took`。
- 读不了的不回，服务端会留着它。

## 6. 调用（第 2 层）

「调用」是一端按名字让另一端做一件事。动作的目录在契约的 `calls.actions`，失败码在 `calls.failCodes`。

**走会话的端**（推荐）：能力里说 `calls`，在 `link.child_new` 里给 `offers`（这一端会做的动作）。要报版本再说 `versions` 并给 `coreCommit`（你钉的核心提交，40 位十六进制；没说 `versions` 时它不起作用）。之后：

- 主机来调时，会话已经做完检查，交给你一个 `call` 交付。**先把 `callId`、`action`、`version` 三样写到盘上再做**；做完交 `callDone` 或 `callFailed`；等到 `call_settled` 交付（主机收到结果了）再把盘上那条划掉。
- 进程重启后、连上之前，把盘上没划掉的用 `callsLost` 交回（`[{callId, action, version}]`，三样都要），会话替你答「不知道做没做」，不会再做一遍。
- 断线时也照常 `tick`，期限才会到。

规则在核心文档「版本、调用和 stream」。

**自己写连接、不经会话的端**：收到 `call.ask` 怎么判，问 `link.call_receive`，不要自己写判定顺序。

- 输入：`action`、`version`、`for`、`deadlineAt`（都是 ask 里的）、`takenAt`（你收下它的时刻，主机的钟；它和 `deadlineAt` 都是毫秒）、`offers`、`refused`（这一端会做、但主人在设备上关了的）、`granted`（主机在这条连接上说过授予的；还没说过写 `null`，这个字段不能省）。
- 输出：`verdict` 是 `hold`（先搁着，等 `call.granted` 到了带着它再问一次）、`do` 或 `fail`（`code` 是失败码）；`version` 是答复里写的版本；`deadlineAt` 是截短过的期限。搁着的调用再问时，`deadlineAt` 交回上次给的那个。
- 它只管判。按 `callId` 去重、搁着的调用存在哪、到期叫停、结果的可靠送达，你要自己做。

规则在核心文档「收到调用怎么判（`link.call_receive`）」。

两条容易想错的：

- **开关关着的动作照样报。** `call.offer` 报的是「能做」，不是「主人允许」。主人在设备上关掉的动作放进 `refused`，被调时答 `call_not_granted`。
- **授予只认这条连接上主机说的。** 每条新连接上，`call.granted` 到之前来的调用先搁着，不做也不答。

## 7. 一致性用例怎么跑

对照都在 `crates/conformance/`。两种：

**一问一答的用例。** 每个文件是 `{version, call, cases: [{name, input, output}]}`。把 `input` 交给 `myriad_core_call` 调 `call` 那个调用，回答要和 `output` 一样（按规范字节比）。输入被拒时 `output` 是 `{"error": "bad_input"}`，不比消息。和这一页有关的目录：

| 目录 | 管什么 | 哪一层要跑 |
| --- | --- | --- |
| `v1/link/` | 帧的检查（`check.json`）、收到调用怎么判（`call_receive.json`） | 都跑 |
| `v2/log/` | 信封、版本向量、收、合并、折叠、时钟 | 第 3 层 |
| `v2/nodes/` | 节点的规则，包括端之间的发和收 | 第 3 层 |
| `v1/partner/` | 伙伴的 id | 第 3 层 |
| `v1/lease/` | 移动、自己停下、任期先后 | 第 4 层 |

这些用例查的是「你经 C 入口调到的核心」和别的端是同一个。它们过了，说明你链接对了、封装没把 JSON 改坏。

**连接的剧本。** `v1/child/` 里有二十六份。一个假主机照剧本和你的端对话，检查你发的每一帧。它们查的是你的连接层和别的端一致。

- Rust：给你的连接代码实现 `myriad_link_conformance::ChildEnd`，对每份剧本调 `play`。
- Swift：自己照剧本一步步走，假主机的每一步问 `conformance.host`。可以照着 `crates/myriad-core-ffi/src/tests/link.rs` 里的 `AcrossTheEntry` 写：它只经 C 入口，把每一份都演过。
- 头里写了 `passthrough: true` 的剧本只给开了口子的端演。

剧本的格式和每一份查什么，在核心文档「她的连接：子机一侧的剧本」。剧本不测你的套接字代码和界面。

## 8. 例子：Android XR

`Myriad-You/myriad-androidxr` 是一个只做第 1 层的端，可以照着看一个外壳长什么样（下面按它的 main `39d4e43`）：

- 依赖：`Cargo.toml` 里按提交钉 `myriad-link-child`。它钉的是较早的 `1c21bff`，那时还没有调用、stream 和口子，所以这些它都没有接。
- 能力清单：`crates/myriad-xr-link/src/lib.rs` 的 `XR_CAPABILITIES`，十四项。
- 外壳的分发：`crates/myriad-xr-ffi/src/lib.rs` 里把会话的输出转成给 Kotlin 的一步（`fn step`）。读不了的帧（`Delivery::Unread`）它记成一条问题，不悄悄丢掉。
- 会话、契约的读法、帧检查都是核心的，它没有自己的一份。它自己留着的是套接字外壳和凭据（Kotlin 一侧）、形象和语音。uncertain：Kotlin 的套接字外壳我没有逐行读。

它的钉比现在的 main 旧十几个提交。换钉时要补什么，看这段历史里每个提交说明的「影响的端」：

```bash
git log 1c21bff..<要钉的提交> -- crates/myriad-link-child crates/myriad-her-link shared/her_link_contract.json
```

其中改了 `Delivery`、`Unsent` 形状的有好几个（比如 `#118`、`#120`、`#121`、`#125`、`#126`、`#129`）。

## 9. 最小接入清单

第 1 层（只当子机）：

- [ ] 按一个提交钉住核心。Rust 直接依赖 `myriad-link-child`；Swift 链接 `libmyriad_core_ffi.a`（核心文档「别的平台怎么链接」）。iOS 的静态库核心的 CI 每次都在出（真机和模拟器两种），但还没有哪个端真的链接过它。
- [ ] 外壳的循环：升级、`connected`、`received`、`tick`、`disconnected`、`link.child_after`。一步一步来，不并发。
- [ ] 每句话一个新的回合 id。断线时主人说的话照样交给会话。
- [ ] 存下每次 `welcome` 的 `lineage`、`hostEpoch`，下次传回。
- [ ] 经 C 入口时：时间是微秒；某一步失败就留着旧状态；JSON 库把数都当双精度的，两个调用都带 `text: true`。
- [ ] 拿到 `unsent` 告诉主人这句没发出去；拿到 `unread` 记日志、提示版本不齐。
- [ ] 能力清单的测试：`link.child_claims` 的 `unread` 是空的。
- [ ] `v1/child/` 里不带 `passthrough` 的剧本全过；`v1/link/` 的用例全过。

第 2 层（加调用）：

- [ ] 能力里说 `calls`（要报版本再说 `versions`），给 `offers` 和 `coreCommit`。
- [ ] 先把 `callId`、`action`、`version` 写到盘上再做；等 `call_settled` 再划；重启先 `callsLost` 再连。
- [ ] 断线时照常 `tick`。
- [ ] 不经会话的端：判定只问 `link.call_receive`，`granted` 每次都写。

第 3 层（加日志）：

- [ ] 开口子；可靠的 `frame` 落盘就 `took`；等确认的多了放慢读套接字。
- [ ] 只在 `welcome.sync` 是 `on` 时同步；伙伴的 id 只从 `welcome.partner` 拿。
- [ ] 能力里说 `partner`（不说，`welcome` 里就没有伙伴的 id），和你的核心相符的 `log2`、`log3`、`log4`，以及 `log.pages`。
- [ ] 因为等确认的满了被断开（`behind`）之后，重连上要重新发 `log.vector`：`log.rows` 主机不会自己重发。
- [ ] `welcome` 后每个受众发 `log.vector`；收用 `nodes.wire_take` 加 `log.merge`；发用 `nodes.wire_send`。
- [ ] 翻页：了结带 `more` 的批之后才发带 `after` 的 `log.vector`；自己发页时一页没翻完不重发。
- [ ] `v2/log/`、`v2/nodes/`、`v1/partner/` 的用例全过；带 `passthrough` 的剧本全过。

第 4 层（加租约）：

- [ ] 状态只问 `lease.here`；拿到 `lease.grant` 先 `lease.behind` 再想；任期旧了立刻停。
- [ ] 说了 `deputy`：留着最新的凭证；`lease.grant`、`lease.deputy` 回 `lease.taken`，再来就再回。
- [ ] `v1/lease/` 的用例全过。

卡住了，报问题时带上：你钉的核心提交、哪个用例或哪份剧本的第几步、回答或 `fault` 的原文。
