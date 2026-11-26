# Kiln — Mimari Tasarım

> Kiln geçici bir çalışma adıdır. Bu belge Kiln'i geliştirecek yapay zeka ajanları ve insanlar için ana referanstır.
> Kod, tanımlayıcılar ve arayüz metinleri İngilizcedir.

## 0. Doğrulanmış teknik zemin

Aşağıdakiler 2026-11-19'da Linux üzerinde (Claude Code 2.1.325, Codex CLI 0.151.0, rustc 1.100.0) çalıştırılarak doğrulandı. "Doğrulanmadı" diye işaretli olanlar §19'daki spike'larda kapatılacak.

| Konu | Durum | Kanıt / not |
|---|---|---|
| Claude Code yapılandırılmış I/O | ✅ | `claude -p --input-format stream-json --output-format stream-json --verbose` olay akışı veriyor |
| Claude izin istekleri IDE'ye geliyor | ✅ | `--permission-prompt-tool stdio` + host'tan `control_request{subtype:"initialize"}` → CLI `control_request{subtype:"can_use_tool", tool_name, input, permission_suggestions}` gönderiyor, `control_response{behavior:"allow"\|"deny"}` ile cevaplanıyor. Sadece `--permission-prompts host` verilirse istekler otomatik reddediliyor. |
| Subagent ilişkisi ve model | ✅ | Her `assistant` mesajında `parent_tool_use_id`, `message.model`, `usage` var; `--forward-subagent-text` subagent metnini akışa ekliyor; `result` içinde `subagent_stats`, `modelUsage` var |
| Abonelik kota bilgisi | ✅ | `rate_limit_event.rate_limit_info.unifiedWindows.{five_hour,seven_day}.{utilization,resetsAt}` |
| Slash komut listesi | ✅ | `initialize` cevabı `commands` listesini döndürüyor |
| Hesap izolasyonu (Claude) | ✅ | `CLAUDE_SECURESTORAGE_CONFIG_DIR=<boş dizin> claude auth status --json` → `loggedIn:false`, ama `projectsDirectory` ve `configDirectory` yine `~/.claude` (macOS, Claude Code 2.1.311). Belgelenmemiş bir değişken: yalnızca kimlik deposunu taşır; macOS'ta Keychain kaydı `Claude Code-credentials-<sha256(dizin)[:8]>` olur. Hesap bilgisi (`oauthAccount`) ve kullanım önbelleği (`cachedUsageUtilization`) ortak `~/.claude.json`'da kalır ve her girişte son girilen hesapla üzerine yazılır (iki gerçek girişle doğrulandı); istekler yine de deponun hesabına gider |
| Terminalsiz giriş (Claude) | ✅ kısmen | VS Code eklentisi gibi kontrol protokolüyle: `claude_authenticate{loginWithClaudeAi}` → `{manualUrl, automaticUrl}`, `claude_oauth_wait_for_completion` → `{account}`. İlk yarı kaydedildi; tamamlanan giriş henüz kaydedilmedi |
| Hesap izolasyonu (Codex) | ✅ | `CODEX_HOME=<boş dizin> codex login status` → `Not logged in`. Codex, `CODEX_HOME` `/tmp` altındaysa yardımcı binary oluşturmayı reddediyor → profil dizinleri kalıcı veri dizininde olmalı |
| Co-author kapatma | ⚠️ kısmen | `includeCoAuthoredBy` ayar adı Claude binary'sinde var, ama `--settings` ile verilip denenmedi; attribution başka bir anahtarla da yönetiliyor olabilir. Codex için ayar aranmadı. Garanti IDE tarafı temizlemedir (§8) |
| Codex yapılandırılmış I/O | ✅ (2026-11-26, CLI 0.172.0, §4) | `codex app-server` stdio üzerinden JSON-RPC (deneysel). Olay akışı canlı denendi (S5). `codex app-server generate-json-schema` şema üretiyor: `CommandExecutionRequestApproval`, `FileChangeRequestApproval`, `PermissionsRequestApproval`, `tokenUsage`, `RateLimitSnapshot`, `SubAgentActivity`, `collabAgentToolCall`, `SubagentStart/Stop` |
| UI çatısı | ✅ Linux | `gpui-kit 0.10.0` (Apache-2.0) `gpui-pre =0.3.12`'yi sabitliyor. İçinde dock, sekme, tree, tree-sitter'lı kod editörü (200K satır), diff, markdown, virtual list, mesaj kaydırıcı ve UI test desteği var. Boş pencere Wayland'da açıldı ve temiz kapandı. İlk release derlemesi 4 dk 37 sn, binary 51 MB (strip'siz), boş pencere RSS ≈ 93 MB |

## 1. Hedefler ve hedef olmayanlar

**Hedefler**
- Birden fazla Claude Code ve Codex aboneliğiyle aynı anda oturum çalıştırmak. Her oturum ve subagent hangi abonelikte çalıştığını göstersin.
- Ajanlar kendi resmi CLI'larıyla, abonelik üzerinden çalışsın; API anahtarı yok.
- Çok düşük kaynak kullanımı: GPU ile çizilen yerel arayüz; Chromium pakete girmez (tarayıcı paneli kullanıcının kurulu tarayıcısını çalıştırır).
- Git akışı IDE'ye ait olsun: politika IDE ayarıdır, prompt değildir.
- Kod okuma odaklı editör (tree-sitter renklendirme, diff) ve düzenleme desteği.
- Ajanların başlattığı servisleri görmek, durdurmak ve IDE'den başlatmak.
- macOS + Linux zorunlu, Windows hedef.

**Hedef olmayanlar** (YAGNI)
- LSP, eklenti sistemi, genel amaçlı gömülü terminal, ortak düzenleme, herkese dağıtım, API anahtarıyla ücretlendirme.

## 2. Süreç modeli

```
┌──────────────────────────── kiln (tek binary) ────────────────────────────┐
│ UI thread (GPUI)  ←→  uygulama durumu (Entity'ler)                         │
│ background executor: agent oturumları, git, servis denetleyicisi, IPC     │
│ yerel IPC soketi (Unix socket / Windows named pipe)                       │
└───────┬───────────────────────────┬───────────────────────────┬───────────┘
        │ stdio (stream-json)        │ stdio (JSON-RPC)          │ process group
        ▼                            ▼                           ▼
  claude -p ...                codex app-server            servisler (npm run dev …)
  CLAUDE_CONFIG_DIR=<hesap>    CODEX_HOME=<hesap>
        │ MCP stdio                  │ MCP stdio
        ▼                            ▼
  kiln mcp --session <id>  ──IPC──▶  kiln (ana süreç)
```

- Her oturum bir alt süreçtir. Hesap ortam değişkeniyle seçilir.
- Ajanlara Kiln'in araçları **MCP köprüsü** üzerinden verilir. `kiln mcp` aynı binary'nin bir alt komutudur, MCP stdio satırlarını olduğu gibi yerel sokete aktarır; protokolü ana süreç konuşur. Port açılmaz. Soket yalnızca kullanıcının açabileceği bir dosyadır (veri klasöründe ya da yol çok uzunsa geçici klasörde). Her sohbet kendi token'ını alır; token, sunucu tanımının ortam değişkeniyle geçer ve çağrının hangi sohbetin tarayıcısına gideceğini söyler. Claude'a `--mcp-config` ile eklenir ve araçları `--allowedTools mcp__kiln` ile sorusuz kullanır, çünkü önemli olanı (hangi siteye girileceğini) Kiln kendisi sorar. Codex'e `-c mcp_servers.kiln...` ile eklenecek.
- Tek async çalışma zamanı: GPUI executor'ları + `async-process`/`async-channel` (executor-bağımsız). Tokio yoktur: köprü yalnızca araç sunar, bunun için gereken MCP (`initialize`, `tools/list`, `tools/call`) `kiln-bridge` içinde elle yazılmıştır; soketi ve bekleyen çağrıları düz thread'ler taşır.

## 3. Workspace yapısı

```
kiln/
├── assets/themes/kiln.json  Kiln Dark / Kiln Light (gpui-kit tema formatı)
├── crates/
│   ├── kiln/               bin: uygulama kabuğu, alt komutlar (`kiln mcp`)
│   ├── kiln-core/          domain tipleri: id'ler, Project, Account, AgentEvent
│   ├── kiln-settings/      JSONC katmanlı ayarlar, hot-reload
│   ├── kiln-keymap/        VSCode keybindings.json formatı
│   ├── kiln-project/       dosya listeleme (gitignore), dil tespiti, bulanık arama, dosya oluşturma/adlandırma/çöpe taşıma
│   ├── kiln-workspace/     GPUI: pencere yerleşimi, konuşmalar sayfası, editör, paletler
│   ├── kiln-agent/         AgentBackend trait'i, oturum aktörü, izin motoru
│   ├── kiln-agent-claude/  stream-json + control protokolü, kurulu CLI sürümü ve güncellemesi
│   ├── kiln-agent-codex/   app-server JSON-RPC (şemadan üretilmiş tipler)
│   ├── kiln-accounts/      profiller, giriş akışı, kota anlık görüntüsü
│   ├── kiln-bridge/        ajanlara açılan MCP araçları + IPC
│   ├── kiln-services/      çalıştırma konfigleri, süreç denetleyici, süreç ağacı izleme
│   ├── kiln-file-icons/    renkli dosya/klasör ikonları (Material Icon Theme, MIT) ve isimden ikona eşleme
│   ├── kiln-browser/       kurulu Chromium tarayıcıyı DevTools Protocol ile sürer: sayfa, kareler, fare, klavye
│   ├── kiln-devices/       sayfaların WebUSB, WebHID ve Web Serial ile kullandığı USB, HID ve seri cihazlar (nusb, hidapi, serialport)
│   ├── kiln-git/           git CLI sarmalayıcı: status ve diff (ileride worktree, commit hattı, trailer temizleme)
│   ├── kiln-forge/         gh / glab sarmalayıcıları
│   └── kiln-sys/           unsafe gerektiren tek yer (process group, Job Object)
├── xtask/                  `cargo xtask check` ve kalite betikleri
├── quality/                eşikler, baseline'lar, config kilidi
└── docs/
```

**Bağımlılık kuralı:**
- Crate'ler yeteneğe göre ayrılır (gpui-kit kodlama kılavuzu): her özellik kendi modelini, view'larını ve komutlarını tek crate'te tutar.
- Protokol, ayar, git ve süreç mantığı GPUI'den bağımsız crate'lerde kalır ve UI olmadan test edilir. GPUI'ye sadece view sahibi özellik crate'leri (`kiln-workspace`, ileride `kiln-chat` vb.) ve `kiln` bağımlıdır.
- `kiln-core` hiçbir iç crate'e bağımlı değildir.
- Crate'ler ihtiyaç doğdukça, ait oldukları kilometre taşında oluşturulur.

## 4. Ajan soyutlaması

```rust
pub trait AgentBackend: Send + Sync {
    fn kind(&self) -> AgentKind;
    fn capabilities(&self) -> Capabilities;           // subagents, images, rate_limits, resume, interrupt
    fn spawn(&self, spec: SessionSpec) -> Result<SessionHandle>;
}

pub struct SessionHandle {
    pub events: Receiver<AgentEvent>,
    pub commands: Sender<SessionCommand>,             // SendUser, RespondPermission, Interrupt, Shutdown
}
```

`AgentEvent` (normalize edilmiş, her olay `raw: serde_json::Value` da taşır):
`SessionStarted{model, tools, commands, mcp_servers}`, `TextDelta`, `Thinking`, `ToolCall{id, parent, name, input}`, `ToolResult`, `PermissionRequest{id, tool, input, suggestions}`, `SubagentStarted{id, parent, model, description}`, `SubagentFinished`, `Usage{context_tokens, context_window}`, `RateLimit{windows}`, `TurnEnded{usage}`, `Error`.

- Yeni ajan eklemek = bir `kiln-agent-<x>` crate'i. Trait, ACP kavramlarıyla (oturum, tool call, izin isteği) uyumlu tutulur; ileride `kiln-agent-acp` ile Gemini CLI gibi ACP destekli ajanlar tek backend'le gelir.
- Protokoller resmi olarak kararlı değil: her backend kendi crate'inde izole, gerçek çalıştırmalardan alınmış fixture'larla sözleşme testi var, açılışta CLI sürümü kontrol edilir ve desteklenen aralık dışındaysa uyarı verilir.
- Codex tipleri kurulu sürümün şemasından üretilir ve repoya commit'lenir.

**Oturum geçmişi:** Kiln kendi indeksini tutmaz; CLI'nin sakladığı konuşmaları okur. Claude her konuşmayı `<config>/projects/<cwd>/<oturum id>.jsonl` olarak yazar (`<config>` `CLAUDE_CONFIG_DIR` ya da `~/.claude`; dizin adı yoldaki harf ve rakam dışı her karakterin `-` olmasıyla oluşur). Yalnızca bu klasörün yerel geçmişi gösterilir.
- Sol panel Claude Desktop'taki gibi: "New session" ve en son etkin olan üstte oturumlar. Liste VS Code eklentisinin kurallarıyla kurulur: her dosyanın baştan ve sondan 64 KB'ı okunur; başlık sırasıyla kullanıcının verdiği başlık (`custom-title`), CLI'nin yazdığı (`ai-title`), son istem (`last-prompt`), ilk istem. Subagent konuşmaları listelenmez.
- Bir oturum açılınca mesajları dosyadan okunur ve süreç `--resume <id>` ile başlar. Dosyadaki satırlar `parentUuid` ile bir ağaç kurar (önceki bir mesajı düzenlemek yeni dal açar); gösterilen konuşma son mesajda biten daldır, sıkıştırma sınırı `logicalParentUuid` ile aşılır. Tur sürerken başka oturum açılmaz.
- Sıralama son mesajın zaman damgasına göredir, dosyanın değişme zamanına göre değil: açılan süreç daha mesaj yazılmadan dosyaya kayıt ekler.
- Başka bir istemcide açık konuşma: CLI çalışırken `<config>/sessions/<pid>.json`'da hangi konuşmayı açtığını tutar. Süreci yaşayan ve Kiln'in kendi süreci olmayan bir kayıt, konuşmayı listede "Open" olarak işaretler. Kiln onu açınca `--resume <id> --fork-session` ile bir kopyasını sürdürür; iki süreç aynı dosyaya yazıp konuşmaları karıştırmaz.
- Claude Code'a yapıştırılan metin `<pasted_content>` içinde saklanır; geçmişte etiketsiz gösterilir. Sekiz satırdan uzun kullanıcı mesajı katlanmış gelir, "Show all N lines" ile açılır.
- Codex (`kiln-agent-codex`, `codex app-server`): konuşmalar `thread/list` ile listelenir (başlık `name`, yoksa `preview`), mesajlar `thread/read` (`includeTurns`) ile okunur, süreç `thread/resume` ile sürer. Klasörün listesi her harness'ın listesinin birleşimidir, en son etkin olan üstte; listeleyemeyen harness (ör. kurulu olmayan Codex) diğerlerini engellemez. Her `PastSession` onu saklayan harness'ı (`agent`) taşır; açılan konuşma o harness'la açılır, kaydedilen düzen (`SavedChat.agent`) sohbetin harness'ını tutar.

**Birden çok harness:** Pencere `kiln_chat::Backends`'i (varsayılan Claude, ardından Codex) tutar. Boş sohbetin ortasında "Claude Code / Codex" seçimi vardır; seçim süreci o harness'la ve onun ilk eklenen hesabıyla yeniden başlatır. Yeni sekme kullanıcının en son çalıştığı harness'la açılır.

**Konuşmayı devretmek:** Konuşma başlamış bir sohbet de diğer harness'a geçebilir: hesap panelinde onun bir hesabı ya da olağan girişi seçilince, ya da limit uyarısındaki "Continue in Codex" ile. Diğer harness saklanan konuşmayı sürdüremez, bu yüzden yeni bir konuşma başlar ve ona Kiln'in gösterdiği konuşma anlatılır (`SessionSpec.handoff`: kullanıcı ve ajan mesajları, araç çağrılarının girdisi ve çıktısının başı, sondan 60.000 karaktere kadar). Claude'a `--append-system-prompt`, Codex'e `developerInstructions` ile gider. Yalnızca ilk süreçte gider ve yeni konuşma `Started` bildirince düşer. Ekrandaki transkript kalır, altına "Continued in Codex" notu düşer. Mod ve effort taşınır, model harness'ın varsayılanına döner. Limit bir turu kestiyse, devralan harness'a "Continue where you left off." gider. Geri dönüş de aynı yoldan olur: bu kez Codex'in turlarını da içeren yeni bir Claude konuşması başlar. Geçmişten açılınca her konuşma yalnızca kendi kısmını gösterir. Codex 0.172.0'da `developerInstructions` ile anlatılan konuşmanın sürdüğü canlı denendi.

**Renk:** Kiln, etkin sohbetin harness'ının rengini alır. Claude turuncusu temanın kendi rengidir. Codex'te temanın `primary`, `ring`, `link`, `selection` ve `sidebar.primary` renkleri maviye çekilir, sohbetin vurgu rengi (`kiln_chat::Brand`) de maviye döner. Bir harness'ta karşılığı olmayan özellik gizlenir, boş ya da bozuk gösterilmez: `AgentBackend::features()` söyler (Codex'te skill/hook listesi ve prompt önbelleği sayacı yok). Codex'te recap, prompt önerisi ve Claude'un slash komutları zaten gelmez.

Codex çevirmeni `Ready`'yi bir kez yayar; sonraki model listeleri `ModelsChanged` olarak gelir. Sohbet her `Ready`'den sonra modelleri ve kotayı yeniden istediği için ikinci bir `Ready` sonsuz bir istek döngüsüne girerdi. Bu döngü görüldü: dakikada yüz binden fazla `model/list` ve `account/rateLimits/read` isteği gitti, ardından tur, Codex'in ağ iznini "revoked" bildirmesiyle düştü.

Codex'in alt ajanı aynı bağlantıda kendi thread'inde akar. Ana thread'deki `subAgentActivity` (`started`) Kiln'de bir `Agent` çağrısı ve `SubagentChange::Started` olur; açıklama `agentPath`'in son parçasıdır. Alt thread'in bildirimleri `threadId`'ye göre ayrılır: araç çağrıları `parent` ile o kartın içine girer, mesajları `SubagentChange::Text` olur, turu ana turu bitirmez. `completed` etkinliği ya da alt turun düşmesi kartı rapor olarak son mesajla kapatır. `StopSubagent` alt thread'in turunu `turn/interrupt` ile keser. Kiln'in başlatmadığı thread'lerin bildirimleri yok sayılır.

**Codex app-server:** satır başına bir JSON-RPC mesajı (`jsonrpc` alanı yok). `initialize` → `initialized` → `thread/start` ya da `thread/resume` → `turn/start`; tur sürerken gönderilen mesaj `turn/steer` ile o tura katılır, Stop `turn/interrupt`'tır. İzin modları `approvalPolicy` + `sandboxPolicy`'ye eşlenir: AskBeforeEdits → `untrusted` + workspace-write; EditAutomatically ve Auto → `on-request` + workspace-write; Plan → `on-request` + read-only; DontAsk → `never` + workspace-write; BypassPermissions → `never` + danger-full-access. Komut ve dosya değişikliği onayları (`item/*/requestApproval`) Claude'unkiyle aynı izin kartına düşer. Context `thread/tokenUsage/updated`'den (yalnızca toplam), kota `account/rateLimits/*`'ten gelir; 24 saate kadar pencere oturum, daha uzunu haftalık sayılır (Free planda tek 30 günlük pencere var).

**Uyku modu (kaynak tasarrufu):** Kiln'in kendisi hafiftir; RAM'in çoğunu CLI süreçleri harcar. Belirli bir süre boşta kalan oturumun süreci kapatılır, sıradaki mesajda `--resume` ile şeffafça geri açılır. Süre ayarlanabilir.

**Limit dolunca:** Oturum "Rate limited until HH:MM" durumuna geçer, sohbet görünür kalır, gönderme kapalıdır, sıfırlanınca açılır. Hesap başına kota çubuğu `rate_limit_event` / Codex `rateLimits` verisinden gelir.

## 5. Hesaplar

- `kiln-accounts`: eklenen hesaplar `<data>/accounts.json`'da (`{id, email, plan, agent}`), her birinin kimlik deposu `<data>/accounts/<id>/`. `<data>` macOS'ta `~/Library/Application Support/kiln`, Linux'ta `~/.local/share/kiln`. Bir harness için hiç hesap eklenmemişken sohbetler onun olağan girişiyle (Claude Code ya da Codex) çalışır; o harness'a ilk hesap eklenince sohbetler tur aralarında ona geçer. Olağan giriş listede yer almaz.
- **Yalnızca giriş ayrı:** Claude oturumu `CLAUDE_SECURESTORAGE_CONFIG_DIR=<hesabın deposu>` ile başlar. Oturum geçmişi, ayarlar, skill'ler ve MCP tanımları ortak `~/.claude`'da kalır; bu yüzden bir konuşma başka bir hesapla `--resume <id>` ile sürer. Hesap başına `CLAUDE_CONFIG_DIR` geçmişi de ayırırdı.
- Hesap kimlikleri tekrar kullanılmaz: macOS'ta Keychain kaydı depo yolunun özetiyle adlandırılır ve depo silinince kalır.
- Giriş: Kiln hesabın ortamıyla bir CLI süreci açar, `claude_authenticate` ile aldığı sayfayı tarayıcıda açar, `claude_oauth_wait_for_completion` yanıtındaki hesabı kaydeder. Aynı e-posta ikinci kez eklenmez.
- Hesap değiştirme: sohbetin süreci yeni hesabın ortamıyla ve aynı oturumla yeniden başlar; tur sürerken yapılmaz.
- Eklenen hesabın adı ve planı Kiln'in kaydından gelir, CLI'nin bildirdiği hesaptan değil. Kullanımı kendi turlarındaki `rate_limit_event`'ten okunur; `get_usage` dikkatle sorulur: CLI son 60 saniyede herhangi bir hesabın çektiği kullanımı `~/.claude.json`'daki ortak önbellekten döndürür; sonrasında kendi hesabıyla sorar ama cevabı ortak önbelleğe son giriş yapan hesabın kimliğiyle yazar ve onun göstergesini bozar (CLI 2.1.311'de üç hesapla doğrulandı). CLI 2.1.326'da bu davranış sürüyor, bu yüzden Kiln eklenen hesap için `get_usage`'ı yalnızca CLI'ın ortak anlık görüntüsü (`~/.claude.json` → `cachedUsageUtilization.fetchedAtMs`) 65 saniyeden eskiyse ve tüm sohbetlerde son sorudan 70 saniye geçmişse sorar; o durumda CLI hesabın kendi girişiyle taze sayıyı çeker. Hesabın sayısı son 70 saniyede bildirildiyse sorulmaz. Sohbetler iki dakikada bir ve kullanım paneli açılınca sormayı dener. Gelen sayı aynı hesabı kullanan tüm sohbetlerde hemen görünür (daha yeni olan, eskisinin yerini alır). Eklenen her hesabın son bildirilen kullanımı `<data>/usage.json`'da tutulur ve tüm sohbetlerce paylaşılır; yeni sekme ve yeniden açılan Kiln, hesabın ilk yanıtını beklemeden son sayıları ve ne zaman bildirildiklerini gösterir. Henüz bilinmiyorsa başlıkta "Usage" düğmesi kalır, paneli bunu söyler. Sohbet ayrıca ayrıldığı hesapların son kullanımını hatırlar.
- Limit dolunca: limit uyarısında limitli olduğu bilinmeyen ilk diğer hesap için "Continue with <e-posta>" düğmesi çıkar; tıklayınca o hesaba geçilir. Limit bir turu yarıda kestiyse (tur sürerken limit geldiyse ya da tur limitliyken bittiyse), hangi yoldan olursa olsun hesap değişince Kiln "Continue where you left off." mesajını kendisi gönderir; devam ettirilen oturum asıl isteği tuttuğu için bu yeter. Arada kullanıcı yeni bir mesaj gönderdiyse bir şey gönderilmez.
- Her hesap bir harness'a aittir (`Account.agent`, eski kayıtlarda `claude`). Hesap paneli tüm harness'ların hesaplarını, her birinin adı altında ayrı listeler. Hesabı olmayan diğer harness için onun olağan giriş satırı çıkar. Her grubun kendi "Add … account" düğmesi vardır. Kendi harness'ının hesabını seçmek konuşmayı o hesapla sürdürür; diğer harness'ın hesabını seçmek konuşmayı devreder (§4).
- **Codex:** hesabın deposu `CODEX_HOME` olur, girişi (`auth.json`) orada kalır. Ortak `~/.codex`'teki (ya da Kiln'i başlatan `CODEX_HOME`'daki) `config.toml`, `AGENTS.md`, `sessions`, `archived_sessions`, `skills`, `rules`, `prompts` depoya sembolik bağla bağlanır, `CODEX_SQLITE_HOME` ortak eve işaret eder; böylece thread dizini tek kalır ve konuşma başka hesapla sürer (Windows'ta bağ yok). Giriş Kiln içinde: hesabın ortamıyla açılan app-server'a `account/login/start` (chatgpt), dönen `authUrl` tarayıcıda açılır, `account/login/completed` sonrası `account/read`. Codex `/tmp` altındaki `CODEX_HOME`'u reddeder (§0).
- Rozet: Hesap etiketi + rengi oturum sekmesinde, sohbet başlığında ve her subagent kartında görünür.

## 6. İzinler

- Kurallar ajanın kendi kurallarıdır; Kiln ayrı bir kural motoru tutmaz. Claude kuralları kendi ayar dosyalarında tutar ve kendisi eşleştirir. Kabuk komutlarının ayrıştırılması (ör. `npm test && rm -rf ~` zincirinin parçalara bölünmesi) güvenlik açısından hassastır; bu işi ajana bırakmak hem daha az kod hem de daha az risk demektir.
- İzin kartı, Claude'un `permission_suggestions` alanında önerdiği kuralı gösterir (ör. `Bash(npm test:*)`). Kural önerisi olan bir istekte "don't ask again" seçeneği üç kapsamla gelir: bu oturum (`session`), bu proje (`.claude/settings.local.json`, kullanıcının kendi proje ayarı) ve tüm projeler (`~/.claude/settings.json`). Kiln, seçilen kapsamı CLI'ın kendi önerisindeki `destination` alanına yazıp öneriyi geri gönderir. Kuralı dosyaya CLI yazar. Öneri olarak yalnızca "düzenlemeleri otomatik kabul et" gelen isteklerde bu seçenek tek kalır ve yalnızca oturum boyunca geçerlidir.
- Codex geldiğinde onun onay istekleri (`*RequestApproval`) aynı izin kartında gösterilir. Kural kapsamları Codex'in desteklediği kadarıyla eşlenir.

## 7. Worktree'ler ve oturumlar arası koordinasyon

- Oturum açarken: ana checkout (sekmedeki "New session") ya da yeni worktree (sekmedeki dal simgeli "New session in a worktree"). Worktree şimdilik isteğe bağlı; varsayılan yapılması ve var olan bir worktree'yi seçmek sonra.
- Konum: `<data>/worktrees/<repo-hash>/<slug>`, branch `kiln/<slug>`; slug oluşturulma zamanıdır (`20261126-153012`). Worktree ana checkout'taki branch'ten başlar; o branch (base) `branch.kiln/<slug>.kilnBase` git ayarında tutulur, bu da worktree'nin Kiln'in olduğunu gösterir ve branch silinince gider. Proje reponun alt klasörüyse sohbet worktree'deki aynı klasörde açılır. Worktree sohbeti kendi projesi gibidir: Files ve Git sayfaları worktree'yi gösterir, son projeler listesine girmez.
- Worktree kurulum komutları `.kiln/run.json` dosyasının `setup` alanındadır (ör. `.env` kopyala, `npm ci`). Bu adım olmadan Node projelerinde worktree işe yaramaz. Kullanıcı bu komutları elle yazmaz; ajana "bu proje için başlatma komutlarını ekle" der, ajan `kiln_run_config_propose` aracıyla önerir, kullanıcı Kiln'de görüp onaylar (§10).
- Kapanış: worktree sohbetinin üstündeki şeritte branch, base ve "Discard" / "Merge into <base>" düğmeleri. Merge, commit edilmemiş değişiklik varken ve ana checkout'ta base açık değilken sebebini söyler; çakışan merge geri alınır. Başarılı merge'den ya da onaylı "Discard"dan sonra worktree ve branch silinir, sekme kapanır. Sekme bu düğmeler olmadan kapatılırsa (`ChatPanel::on_removed`), worktree'de hiç iş yoksa (yeni dosya dahil değişiklik yok, base'in bilmediği commit yok) worktree ve branch kendiliğinden silinir (`kiln_git::remove_unused_worktree`). İş varsa kullanıcı için diskte kalır. "Open MR/PR" M5'te (§9). Kurulum komutları `.kiln/run.json` ile M3'te gelir; o zamana kadar worktree'de `npm ci` gibi adımlar elle yapılır.
- Ajanlara MCP araçları:
  - `kiln_sessions_list` → eş oturumlar: id, hesap, ajan, branch, worktree, durum, başlık, değişen dosyalar.
  - `kiln_message_send(to, text)` → mesaj hedef oturumun turu bitince `[from session X]` etiketiyle iletilir. Token harcadığı için ayar: otomatik / kullanıcı onaylı / kapalı.
- Ajan bilgilendirmesi kısa bir ek sistem promptuyla yapılır (Claude `--append-system-prompt`, Codex developer instructions): uzun süreli süreçler için `kiln_service_*`, commit için `mcp__kiln__commit`, eşler için `kiln_sessions_list`.

## 8. Git

**Commit hattı (Kiln'e ait):**
0. **Açma/kapama:** `git.commit.enabled` (varsayılan kapalı) Kiln'in sohbetin işini kendisi commit etmesini açar: şerit, düğme ve `task_done` yalnızca açıkken çalışır; ajanın kendi `commit` aracı ve Git sayfası her durumda çalışır. 
1. **Tetikleyici** (`git.commit.trigger`, genel / proje / sohbet katmanında, §14): `agent` (varsayılan, ajan `commit` aracını çağırınca) / `task_done` (ayrıca, ajan çağırmadan dosya değiştiren bir tur bitince yazıcı model işin bitip bitmediğine karar verir; soru soruyor, iş kaldığını söylüyor ya da yarıda kalmışsa `NOT_DONE` der ve commit önerilmez). Sohbetin üzerindeki şerit (`chat-work`) commit bekleyen dosya sayısını, tetikleyiciyi ve "Commit & push" düğmesini gösterir (push politikası `never` ise "Commit"); düğme sohbetin turlarının değiştirdiği, hâlâ değişik duran dosyaları yazıcıya karar sordurmadan commit ettirir ve politika `never` değilse ardından push eder. Kullanıcının "commit push yap" demesine gerek kalmaz. Elle commit Git sayfasında her zaman var.
2. **Mesaj kaynağı:**
   - a) Ajan Kiln'in `commit({message, paths?})` aracını (`mcp__kiln__commit`) çağırır. En ucuzu bu, çünkü ajan ne yaptığını zaten biliyor ve git komutlarıyla uğraşmıyor. `paths` yoksa tüm değişiklikler (yeni dosyalar dahil), varsa yalnızca o dosya ve klasörler commit edilir (`kiln_git::commit_paths`); başka stage'li şeyler stage'li kalır. Kurallara uymayan mesaj kullanıcıya gösterilmeden sebebiyle ajana geri döner, ajan düzeltip yeniden çağırır.
   - b) `task_done` ya da düğmeyle: aynı hesapta tek seferlik yazıcı model çağrısı (`ChatView::ask_once` → `AgentBackend::ask_once`) diff + son turun isteği ve cevabıyla. Yazıcı `git.commit.writer` ile seçilir: `agent` harness adı (`AgentBackend::name()`: `claude` ya da `codex`), `model` o harness'ın model id'si (varsayılan `haiku`; boş, harness'ın varsayılanı). Sohbetin harness'ı yazıcınınkinden başkaysa çağrı yazıcının harness'ına, onun ilk eklenen hesabıyla gider; Kiln'de olmayan bir harness hata verir. Ayarlarda yazıcının harness'ı değişince model boşaltılır, çünkü model adları harness'a özgüdür. Claude'da `claude -p --model …`, araçsız, MCP'siz, ayarsız, yani kullanıcının projesinde hook çalışmaz, oturum saklanmaz. Codex'te saklanmayan (`ephemeral`), `never` + read-only bir thread. Commit'e yalnızca sohbetin turlarının değiştirdiği dosyalar girer: her tur başlarken değişen dosyaların içerik izi alınır (`kiln_git::Fingerprint`), tur sonunda izi değişenler sohbetin dosyalarına eklenir; git artık değişik görmediği dosyalar (geri alınmış, başka yoldan commit edilmiş) projenin git durumu her okunduğunda (dosya değişince, kayıtta, tur sonunda) listeden düşer; şeritteki sayı turlar arasında da doğru kalır. Kullanıcının önceden kalmış düzenlemeleri dışarıda kalır. Diff'in ilk 30.000 karakteri gider. Mesaj kurallara uymazsa bir kez daha, sebebiyle istenir. (Kullanıcı onayladı, 2026-11-19; görev bitince ve düğme 2026-11-26.)
   - c) Kullanıcı düzenler.
3. **İnceleme kapısı** (`git.commit.review`, varsayılan açık): istek sohbetin üzerinde bir kartta bekler. Kartta alınacak dosyalar (durum harfi, +/−) ve düzenlenebilir mesaj, "Reject" ve "Commit" var; Cmd/Ctrl+Enter commit eder, Escape reddeder. Çağrı kullanıcı karar verene kadar bekler; ajana commit'in kısa hash'i (mesaj değiştiyse son hâliyle) ya da reddedildiği söylenir. Hook reddederse kart açık kalır, sebep kırmızıyla yazılır. Kapalıyken commit hemen yapılır. Kart yalnızca kullanıcı o sohbetteyse klavyeyi alır.
4. **Temizleme** (deterministik, `kiln_git::MessageRules`): `git.commit.strip` regex listesine uyan satırlar silinir, sondaki boşluklar ve art arda boş satırlar atılır. Varsayılanlar: `Co-Authored-By:` + Claude/Codex/OpenAI/Anthropic, `Generated with [Claude Code]`, `🤖 Generated with`. Aynı fonksiyon MR/PR gövdelerinde de çalışır.
5. **Direktifler ve doğrulama:** `git.commit.instructions` serbest metin direktif listesidir ("Write in English.", "Mention the ticket."). Hem yazıcı modelin talimatına hem Claude oturumunun `mcp__kiln__commit` notuna eklenir; deterministik kontrol edilmez. Deterministik kontroller: konu uzunluğu (`subject_max`, varsayılan 72, 0 sınırsız), Conventional Commits biçimi (`conventional`), konudan sonra boş satır, `require` (mesajın uyması gereken regex'ler) ve `forbid` (uymaması gerekenler). Uymayan mesaj ajana ya da yazıcıya sebebiyle geri döner; yazıcı bir kez daha denenir.
6. `git commit` kullanıcının kimliğiyle çalışır, böylece kullanıcının pre-commit hook'ları normal şekilde devreye girer.
7. **Push politikası** (`git.push.policy`): `never` (Kiln hiç push etmez, düğme de yok) / `manual` (varsayılan, Git sayfasında branch satırındaki ↑ düğmesi) / `after_commit` (ayrıca Kiln'in ajan için yaptığı her commit'ten sonra) / `session_end` (ayrıca commit yapılmış bir sohbet kapanınca). Upstream'i olmayan branch `git.push.remote`'a (varsayılan `origin`) aynı adla gönderilir ve onu izlemeye başlar. Korumalı branch'lere (`git.push.protected`, varsayılan main/master) Kiln kendiliğinden hiç push etmez; elle push "Push to main?" onayı ister. Otomatik push hatası loglanır; Git sayfasındaki ↑ sayısı push edilmeyeni gösterir.

**Ek önlemler:**
- Kiln'in araçları verilen Claude oturumuna `--settings` ile `attribution: {commit: "", pr: ""}` (eski CLI'lar için `includeCoAuthoredBy: false`) ve `Bash(git commit:*)`, `Bash(git push:*)` deny kuralları, `--append-system-prompt` ile de "commit için `mcp__kiln__commit`, push yok" notu verilir.
- Opsiyonel `commit-msg` hook'u, ajanın Kiln dışında commit attığı durumlar için Kiln'in oluşturduğu worktree'lere kurulur.

**Arayüz:** gpui-kit diff bileşeni ile yan yana / satır içi diff. Oturum diff'i (worktree vs base), commit diff'i, çalışma ağacı. Git paneli: status, stage, commit, kısa log, branch değiştirme ve oluşturma (§13, Git).

## 9. Forge (GitHub / GitLab)

- `gh` ve `glab` CLI'ları sarmalanır. Kimlik bilgilerini onlar yönetir, self-hosted GitLab desteklenir.
- Remote host'tan otomatik algılama; self-hosted için host → tür eşlemesi ayarda.
- Kapsam: MR/PR oluştur (başlık ve gövde commit hattıyla aynı şekilde üretilir ve temizlenir), açık MR'larım, branch'in pipeline/CI durumu, tarayıcıda aç. Kararlı çıktı için JSON çıktısı ve `gh api` / `glab api` kullanılır.
- MR, issue ve branch oluşturma her zaman arayüzde açık kullanıcı onayı ister.

## 10. Servisler

`.kiln/run.json` (commit'lenebilir):
```json
{
  "setup": ["cp -n .env.example .env", "npm ci"],
  "services": [
    {
      "name": "web",
      "command": "npm run dev",
      "cwd": "apps/web",
      "env": {},
      "readyPattern": "Local:\\s+http://",
      "url": "http://localhost:5173"
    }
  ]
}
```
- Çalıştır butonu, birden fazla servis, durum (starting / ready / exited(code)), aranabilir log görünümü, stop/restart, URL aç.
- Her servis kendi process group'unda (Unix: `setsid` + `killpg`, SIGTERM → bekleme → SIGKILL; Windows: Job Object) çalışır, böylece durdurma tüm alt ağacı öldürür.
- Ajan araçları: `kiln_service_list`, `kiln_service_start`, `kiln_service_stop`, `kiln_service_logs(name, tail, grep)`, `kiln_run_config_propose(...)`. Sonuncusu `setup` ve `services` için öneri yapar; Kiln öneriyi diff olarak gösterir, kaydetmek için kullanıcı onayı gerekir (aç/kapa). Kullanıcı akışı: ajana "başlatma komutlarını ekle" demek → öneriyi görmek → onaylamak → Run butonunu kullanmak.
- **Kiln dışında başlatılanlar:** Ajan `npm run dev &` ya da arka plan Bash ile süreç açarsa, bu süreç CLI'ın altında görünür.
  - Çalışma alanı süreç tablosunu (`kiln-services`, `sysinfo`) 2 sn'de bir, arka plan iş parçacığında okur. Çalışan ajan yokken okumaz. Okuma birkaç ms sürer; boştaki Kiln %0–0,1 CPU'da kalır.
  - Okumalar `kiln-chat`'teki paylaşılan `Processes` modeline yazılır. Bu model iki yerde gösterilir:
    - her sohbetin başlığında o sohbetin CLI süreci ve altındaki ağaç;
    - durum çubuğunun solunda, proje yolunun önünde tüm ajanların toplamı.
  - CPU bir çekirdeğin yüzdesidir, bellek RSS'tir.
  - Ölçere tıklamak süreçler sayfasını açar:
    - sohbetinki subagent görünümü gibi o sohbetin üzerinde, durum çubuğundaki tüm pencerede, başlık çubuğunun altında açılır;
    - her satırda komut satırı, PID, CPU ve bellek vardır;
    - tümünü gösteren sayfada satırlar sohbetlerinin başlığı altında ağaç sırasıyla durur. Başlıkta sohbetin toplamı yazar;
    - arama komut satırında, PID'de ve sohbet adında arar. Sayfa açılınca odak aramaya değil sayfaya gider, böylece Escape hemen çalışır;
    - sütun başlığına tıklamak o sütuna göre sıralar, ikinci tıklama yönü çevirir. CPU ve bellek büyükten, PID küçükten başlar. Sıralanınca satırlar sohbet başlıklarından çıkar ve bir "Chat" sütunu gösterir;
    - bir satıra tıklamak yanda ayrıntıları açar: üst süreç, durum, başlama zamanı, ne kadardır çalıştığı, çalışma dizini, çalıştırılabilir dosya ve komut satırının tamamı. Ayrıntılar ✕ düğmesiyle, aynı satıra yeniden tıklayarak ya da Escape ile kapanır. Escape önce ayrıntıları, sonra sayfayı kapatır.
  - CLI'ın altındaki her sürecin bir Stop düğmesi vardır. Stop, süreci ve altındakileri en derinden başlayarak SIGTERM ile durdurur. CLI'ın kendisi sohbet sekmesi kapatılarak durdurulur.
  - Dinlenen portlar henüz gösterilmiyor.

## 11. Editör ve görüntüleyici

- gpui-kit kod editörü, LSP kapalı, tree-sitter renklendirme. Düzenleme açık ama arayüz okuma öncelikli.
- Hızlı dosya açma (`ignore` ile gitignore'a uyumlu liste, `nucleo`), proje araması (ripgrep kütüphaneleri `grep-searcher`), satıra git, markdown ve görsel önizleme.
- Dosya değişince (`notify`) otomatik yeniden yükleme ve HEAD'e göre değişen satır işaretleri.
- Diller: gpui-kit highlighter'ın getirdikleri + gerekirse eklenen grammar crate'leri (S4).

## 12. Sohbet arayüzü

- Claude Code CLI'a benzer akış: katlanabilir tool kartları.
  - Bash: komut + çıktı.
  - Çıktı ve diff kutusu (`code_block`): en fazla 168px yüksekliğinde, dikeyde kayar; uzun satırlar kaydırılmaz, sarılır, kutu sohbetin genişliğini aşmaz. Her satırın metni bir `SelectableText`'tir ve pencerenin seçimine katılır: satırlar arasında sürükleyerek seçilir, kopyada satır numaraları olmaz. Kopyalama sırası için her satırın `document_order`'ı girdinin numarası (üst bitler) ve satır numarasıdır (alt 16 bit). İlk 200 satır gösterilir; fazlası için "N more lines not shown" yazar.
  - Edit, `MultiEdit`, Write: diff kutusu. Tool çalışınca CLI sonucun yanında yamayı da gönderir (`tool_use_result.structuredPatch`, kayıtta `toolUseResult`); kart onu `git diff` gibi gösterir: değişmeyen bağlam satırları, dosyadaki gerçek satır numaraları (silinen satır eski, eklenen yeni dosyadaki numarasıyla), satır başında `-`/`+` ve renkli arka plan, hunk'lar arasında `⋯`. Yama yoksa (yeni dosya, tool henüz çalışmadı, izin bekliyor) çağrının kendi girdisinden silinen ve eklenen satırlar gösterilir. Sözdizimi renklendirmesi henüz yok.
  - Read: yol + aralık.
  - Subagent düğmesi: composer şeridinde, ilk subagent başlamadan da görünür. Çalışan subagent sayısını (`Agents 2`) ve solunda bir nokta gösterir; nokta çalışan varken yeşil yanar. Genişliği toplam sayı arttıkça değişmez. Kaçının çalıştığı ve toplamı ipucunda yazar; tıklamak subagent görünümünü açar, hiç subagent yokken düğme pasiftir.
  - Subagent: `Agent` çağrısı, CLI `task_started` gönderince alt ajan kartına dönüşür. Kartta açıklama, alt ajan türü, model, durum ve `task_progress`'ten canlı ilerleme var (ne yaptığı, tool sayısı, token, süre). Bitince `task_notification`'dan Done / Failed / Stopped ve son kullanım. Alt ajanın kendi tool çağrıları ve metni (`--forward-subagent-text`, `parent_tool_use_id`) ana akışa karışmaz, kartın içinde toplanır; kart tıklanınca yerinde açılır ve adımları, ardından raporu gösterir. Her alt ajan `Transcript::subagents()` içinde ayrı bir kayıttır (kimlik, ebeveyn, adımlar); ebeveyni, onu başlatan çağrının `parent_tool_use_id`'sidir. Bir alt ajanın başlattığı alt ajan, ebeveyninin adımları arasında kendi kartıyla durur; kapalı kart altında kaç alt ajan olduğunu ve kaçının çalıştığını söyler. Kayıttan açılan sohbette de iç içe yüklenir (`.meta.json` içindeki `toolUseId`). Kesilen turda yalnızca bütün zinciri ön planda olanlar durmuş sayılır. Karttaki düğme o alt ajanı Agents görünümünde açar. Composer şeridindeki "N agents running" düğmesi, yalnızca o sohbetin panelini kaplayan bir görünüm açar; diğer paneller kullanılmaya devam eder. Görünümde solda konuşmadaki bütün alt ajanlar, başlatanın altında ağaç olarak (önce kendisi ya da altındaki çalışanlar, her satırda durum ve ilerleme; yukarı/aşağı ok gezer), sağda seçilenin kendi sohbeti; iç içe olanın üstünde onu başlatanlara dönen yol, adımlarında başlattıklarına giden satırlar var. Sohbetin üstünde durum, tür, model ve context var. Context, alt ajanın son model isteğinin prompt boyutu ve bunun ne kadarının cache'ten okunduğu; alt ajan mesajlarındaki `usage`'tan gelir. Altında ana ajanın ona verdiği prompt, adımları ve raporu yer alır. Listenin genişliği sürüklenir; son genişlik bütün sohbetlerde geçerlidir, yeniden başlatmada saklanmaz. Liste başlığındaki ikon Agent Map'i açar: solda konuşma, her ajanın sağında başlattıkları, aralarında çizgiler; izin bekleyen, çalışan ve başarısız olanlar renkleriyle ayrılır (izin isteğindeki `agent_id`'den). Düğüme tıklamak haritanın üstünde bir kart açar: durum, tür, model, kimin başlattığı, kullanım, şu an ne yaptığı, sonuç, açılıp kapanan prompt ve tool çağrıları, "Open transcript" ve "Stop agent"; konuşma düğümünün kartı ajanları duruma göre sayar. Escape ve geri düğmeleri bir adım geri gider (transcript → kart → harita → liste). "Stop agent" CLI'a `stop_task` gönderir; CLI ajanı ve altındakileri durdurup `stopped` bildirir. Biten ajanın bekleyen izin isteği kaldırılır. Görünüm canlı güncellenir; Escape ya da ✕ kapatır. "Show in chat" görünümü kapatıp akışta kartına gider ve kartı açar. Böylece aynı anda çalışan çok sayıda alt ajan akışı kaydırmadan izlenir. `SendMessage` ile sürdürülen bir alt ajan kendi kartında devam eder. CLI aynı `task_id` ile yeni bir `task_started` gönderir, ama `task_*` satırları `SendMessage` çağrısını gösterir. Alt ajanın mesajları ise hâlâ ilk `Agent` çağrısını gösterir. Çevirici `task_id`'yi ilk çağrıya eşler: kartta gönderilen mesaj bir adım olarak görünür ve kart yeniden "çalışıyor" olur. Yeni rapor eskisinin yerini alır; `SendMessage` satırı akışta "Sent" olarak kalır. Arka plandaki alt ajan bitince CLI kullanıcı mesajı olmadan yeni bir tur başlatır; ana konuşmada yeni bir model isteği (`message_start`) gelince sohbet yeniden "çalışıyor" olur. Kayıttan açılan sohbette kartlar CLI'ın sakladığı dosyalardan kurulur. Her alt ajanın konuşması `<oturum>/subagents/agent-<id>.jsonl` dosyasında, onu başlatan çağrı da yanındaki `.meta.json` dosyasında durur. Adımlar `Agent` çağrısının, `SendMessage` ile gelen mesajdan sonrakiler o çağrının ardına yerleştirilir; dosyadaki `coordinator` satırı ayrım noktasıdır. Ana konuşmadaki `<task-notification>` mesajları kullanıcı mesajı olarak gösterilmez; `task-id`'leriyle kartın raporu olur. Tur sürerken gelen rapor kullanıcı mesajı olarak değil, `queued_command` türünde bir `attachment` olarak saklanır; o da okunur. Hiç raporu saklanmamış bir alt ajan, `.meta.json` onu bitmiş (`finished`) gösteriyorsa son metniyle Done, göstermiyorsa Stopped olarak açılır; kayıttan açılan bir sohbette hiçbir arka plan ajanı çalışmaya devam etmez. Dosyaları okunamayan bir alt ajanın çağrısı sıradan tool satırı olarak kalır.
  - Düşünme: `Thought for 4.2s · ~214 tokens` satırı, tıklanınca düşünmenin özetini açar. Özet `--thinking-display summarized` ile gelir (CLI ham düşünmeyi vermez). Token sayısı CLI'ın `thinking_tokens` tahminlerinin toplamıdır, süre düşünme bloğunu taşıyan mesajın `thinking_duration_ms` alanıdır. Kayıttan açılan sohbette süre ve token gösterilmez.
  - Tur sonu: `✻ Brewed for 4m 36s · done 16:28` satırı. Süre sonuç satırının `duration_ms` alanıdır; kesilen turlarda ve CLI'ın kendi çalıştırdığı komutlarda gösterilmez. Kayıttan açılan sohbette yalnızca CLI'ın sakladığı `turn_duration` satırları gösterilir; `-p` modu bunları saklamaz.
  - Recap: pencere 3 dakika odakta değilken, en az üç turluk boştaki bir sohbete CLI'ın `/recap` komutu gönderilir (tur başına en çok bir kez); çıktısı `※ Recap: …` satırı olur. Komut mesaj olarak görünmez, sonucu bir turu bitirmez. Claude ayarında `awaySummaryEnabled: false` ise gönderilmez. Elle `/recap` de yazılabilir. Kayıttan açılan sohbette `/recap` çıktıları ve `away_summary` satırları gösterilir.
  - Kiln'in tuttuğu notlar: CLI konuşma dosyasına düşüncelerin çoğunu metinsiz yazar, düşünme süresini ve token'ını, turların bir kısmının süresini ve Kiln'in istediği recap'leri hiç yazmaz. Kiln bunları konuşma başına `<data>/notes/<oturum>.json`'da tutar (tur bitince ve recap gelince yazılır) ve konuşma yeniden açılınca yerine koyar: düşünce, ardından gelen bloğun ya da tool çağrısının önüne; süresi ve token'ı düşüncenin arkasına; tur süresi ve recap turun sonuna. CLI'ın kendi sakladığı süre ve recap tekrarlanmaz. Bunun için kayıtlı bloklar canlı akıştaki gibi model mesajının kimliği (`message.id`) ve `apiBlockIndex` ile anahtarlanır; ikisi olmayan eski satırlar satırın `uuid`'siyle.
- Markdown render, uzun sohbetler için virtual list.
- **Sohbet içi arama** (Ctrl+F, macOS'ta Cmd+F): sohbet başlığının altında bir arama çubuğu açılır. Kullanıcı mesajları, ajanın metni, düşünme, bildirimler, tool adı ve girdisi aranır; tool çıktısı yalnızca çubuktaki düğmeyle istenirse aranır. Büyük/küçük harf ayrımı yoktur. Eşleşme sayısı "2 of 5" biçiminde gösterilir, Enter ve Shift+Enter (ya da oklar) eşleşmeler arasında dolaşır ve geçerli eşleşmeye kaydırır. Mesajlarda ve ajanın metninde eşleşmeler metin içinde vurgulanır, geçerli olan daha parlaktır; bu satırlar arama sürerken kendi `TextViewState`'leri üzerinden çizilir. Tool çağrıları ve kısaltılmış uzun mesajlar gibi metin içi vurgu olmayan satırlar bütün olarak işaretlenir. Escape ya da kapatma düğmesi aramayı kapatır ve odağı mesaj kutusuna verir.
- **Composer:**
  - Çok satırlı yazma alanı.
  - Ekler: görsel ya da Finder'dan kopyalanan dosya yapıştırma, sohbetin üstüne dosya/klasör sürükle-bırak, şeritteki ataç düğmesiyle dosya seçme. Ekler mesaj kutusunun üstünde kart olarak durur, ✕ ile çıkarılır; metinsiz de gönderilebilir.
  - Görseller (png, jpg, gif, webp; 5 MB'a kadar) mesaja içerikleriyle girer: Claude'a metinden önce base64 `image` bloğu olarak gider (Codex: local image). Diğer dosyalar, klasörler ve büyük görseller adlarıyla gider: Claude'da metnin sonuna `@path` referansı eklenir, CLI `-p` modunda da bunu kendisi okuyup modele verir (boşluklu yol `@"…"`).
  - Gönderilen görseller ve dosyalar akıştaki mesajda görünür; kayıttan açılan sohbette mesajın görselleri de geri gelir. Bir tool'un döndürdüğü görsel (ör. Read ile okunan görsel dosyası) tool satırının ve alt ajan adımının altında görünür. Akıştaki, alt ajan görünümündeki ya da ek kartındaki bir görsele tıklamak onu tüm pencereyi kaplayarak açar (pencere tam ekrana geçmez); Escape, görselin köşesindeki ✕ ya da görselin dışına tıklamak kapatır.
  - Slash komutları (`initialize` cevabındaki liste).
  - Model, izin modu ve efor seçici, Stop (interrupt). Tur bitse de arka planda ajan çalışıyorsa ve kutu boşsa düğme Stop olarak kalır; `interrupt` o ajanları da durdurur.
  - Bypass permissions menüden, onay penceresiyle seçilir; Shift+Tab ona geçmez. Süreç `--allow-dangerously-skip-permissions` ile başlar: bu bayrak modu açmaz, yalnızca CLI'ın geçişi kabul etmesini sağlar. Mod saklanan düzene yazılmaz; yeniden açılan sohbet onu kendiliğinden kullanmaz.
  - Sıradaki mesaj tahmini (`--prompt-suggestions`): tur bitince CLI'ın önerdiği mesaj boş kutuda soluk görünür, Tab onu kutuya alır. Yazmak, göndermek ya da yeni bir turun başlaması öneriyi kaldırır. CLI her turda öneri göndermez.
  - Model listesi başta `initialize` cevabından gelir. Her tur sonunda ve `/model` seçicisi açılınca `list_models` ile yeniden sorulur, çünkü CLI sonradan başka bir liste öğrenmiş olabilir. Böylece aynı anda açık iki sohbet farklı listeler göstermez.
  - Efor menüsü yalnızca seçili modelin desteklediği seviyeleri sunar (`initialize` cevabındaki `supportedEffortLevels`). Seçim `apply_flag_settings` ile oturumun bayrak katmanına yazılır; ayar dosyalarına dokunulmaz. Gösterilen efor `get_settings` cevabındaki `applied.effort`'tur (CLI'ın modele göre çözdüğü değer). Hazır olunca, model değişince ve her tur sonunda yeniden sorulur, çünkü kullanıcı `/effort` ya da `/model` yazmış olabilir.
  - Cache düğmesinin solunda konuşmanın harcadığı toplam token; panelinde çıktı, cache'lenmeyen girdi, cache'e yazılan ve cache'ten okunan ayrı durur. Toplam, tur sonu `modelUsage`'tan gelir: alt ajanlar dahildir, `--resume` ile sürer, `/clear` ile sıfırlanır. Tur sürerken ana konuşmanın istekleri (`message_start`, `message_delta`) üstüne eklenir. Kayıttan açılan sohbette ilk istekten önce görünmez, önceki harcama ilk turun sonunda eklenir.
  - Context ölçerinin solunda prompt cache düğmesi: cache sıcakken kalan süreyi dakika olarak (`Cache 30m`, bir saatlik cache'te `Cache 1h`), süresi dolunca uyarı renginde `Cache expired` gösterir; düğme her dakika ve süre dolduğunda kendiliğinden yenilenir. Bitiş saati panelde yazar. Açtığı panelde son isteğin cache'ten okuduğu, cache'e yazdığı ve cache'lenmeyen token'lar, konuşmanın token'larının yüzde kaçının cache'ten okunduğu ve cache'in ne zamana kadar sıcak kalacağı. Değerler her isteğin `message_start` olayındaki `usage`'dan gelir; subagent istekleri ayrı cache'lendiği için sayılmaz. Saklama süresi (5 dakika ya da 1 saat) son yazan isteğin `cache_creation` ayrımından okunur; cache her kullanıldığında bu süre yeniden başlar. Yeniden açılan bir konuşmada düğme, CLI'ın sakladığı konuşma dosyasındaki son ana ajan yanıtının `usage` ve zamanından beslenir (`PastPromptTokens`); dosyayı Claude backend'i okur, arayüz yalnızca ajandan bağımsız olayı görür.
  - Yukarı/Aşağı ok, kutu boşken (ya da geri çağrılan mesaj değişmeden duruyorken) gönderilmiş mesajlar arasında gezer.
  - ESC çalışan turu keser. Mesaja henüz hiçbir cevap gelmediyse mesaj transcript'ten geri alınır ve kutuya döner; "Interrupted" satırı eklenmez. Açık bir panel varken ESC yalnızca paneli kapatır.
- Kullanıcı mesajları sağa yaslı, en fazla akışın %85'i genişliğinde ve Claude turuncusuyla renklendirilmiş bir kutuda gösterilir; böylece gri kutulardaki tool çıktılarıyla karışmaz. Kullanıcı mesajları seçilebilir ve kopyalanabilir. Metin markdown olarak yorumlanmaz: olduğu gibi gösterilir, satır sonları ve girinti korunur.
- **Hook paneli** (composer şeridindeki kanca düğmesi ya da `/hooks`):
  - Liste CLI'ın `get_hooks_listing` isteğinden gelir (terminaldeki `/hooks` menüsüyle aynı veri). Hook'lar olaya göre, olayların gerçekleşme sırasıyla gruplanır; her satırda komut, eşleştirici ve kaynak görünür.
  - Ekleme, düzenleme ve silme CLI'ın `claude edit-hook --json` alt komutuyla yapılır (VS Code eklentisinin kullandığı yol). Dosyayı CLI yazar ve düzenlemeyi kendisi doğrular; Kiln ayar dosyalarına dokunmaz.
  - Yeni hook komut türündedir. Düzenleme yalnızca komut hook'larında yapılır ve hook'u kendi dosyasında yerinde değiştirir (`replace`); formda görünmeyen ayarlar (ör. `timeout`) korunur. Üç yerden birine kaydedilir: bu proje (`.claude/settings.local.json`), paylaşılan proje (`.claude/settings.json`) ya da tüm projeler (`~/.claude/settings.json`). Eklenti ve politika hook'ları salt okunurdur. Kurum ayarları hook'ları kilitliyorsa panel düzenlemeye kapanır.
  - CLI ayar değişikliğini dosya izleyicisiyle birkaç saniye geç görür. Kiln düzenlemeyi hemen gösterir ve listeyi saniyede bir, değişiklik görünene kadar (en çok 12 kez) yeniden ister.
- **Başlık:** ajan ikonu, hesap rozeti, model, context ölçeri, kota ölçeri, worktree/branch.
- **Context ölçeri:** sayı CLI'ın `get_context_usage` cevabıdır (terminaldeki `/context` ile aynı). Hazır olunca, model değişince, `/clear` sonrası, tur bitince ve tur sürerken ana konuşmanın her model isteği başladığında (`message_start`) yeniden sorulur. CLI tur sürerken de cevap verir. CLI 2.1.311'de ölçüldü: cevaptaki toplam, aynı anda başlayan isteğin API'nin saydığı prompt boyutundan %1–7 farklıdır. İlk istekten önceki sayı düşüktür (MCP sunucuları henüz bağlanmamıştır). Alt ajanın context'i, son isteğinin API'nin saydığı prompt boyutudur; yüzdesi gösterilmez.

## 13. Yerleşim

```
┌ Title bar: project ──────────────────────────────────────────────────────────┐
├────┬─────────────────────────────────────────────────────────────────────────┤
│ 💬 │                                                                         │
│ 🗂 │  Seçilen sayfa pencerenin tamamını kaplar:                               │
│ ⎇  │  Chat (sohbet sekmeleri) · Files (ağaç + editör) · Git (değişiklikler)  │
├────┴─────────────────────────────────────────────────────────────────────────┤
└ Status bar: süreç ölçeri │ proje yolu │ açık dosya ─────────────────────────────┘
```
- **Çalışma klasörü session'a aittir, pencereye değil.** Editörlerdeki gibi ilk pencere son bırakılan hali açar: önceki sekmeler her biri kendi klasöründe geri gelir, henüz konuşulmamış sekmeler de klasörüyle boş olarak. Hiç sekme yoksa son projeler ve "Open Folder…" gösterilir (`projects.json`). "New Window" (Ctrl+Shift+N, macOS'ta Cmd+Shift+N; komut paletinde ve başlık menüsünde de var) yeni bir pencereyi bu seçim ekranıyla açar. Yerleşimi ilk pencere tutar, sonraki pencerelerin sekmeleri kaydedilmez. Son projeler listesi pencereler arasında ortaktır. Komutlar son odaklanan pencereye gider; son pencere kapanınca Kiln kapanır. Başlık çubuğundaki klasör adı bir menü açar: penceredeki session'lar (birine tıklamak ona geçer), son klasörler, "Open Folder…" ve "New Window". `kiln <klasör>` o klasörde bir sekme yoksa açar, varsa onu gösterir. Sohbet başlığındaki proje adı proje seçimini açar: boş session oraya taşınır, konuşulmuş olan yerinde kalır ve yeni sekme açılır. Aynı klasördeki session'lar dosya ağacını, editörü, git durumunu ve konuşma listesini paylaşır (`Project`); pencere açık kaldıkça bırakılmaz, çünkü editörde kaydedilmemiş değişiklik olabilir.
- **Yerleşim:** en solda aktivite çubuğu, yanında aktif session'ın klasörünü gösteren **tek** yan panel (Files, Search, Changes), ortada sohbetler, sağda tarayıcı paneli. Tarayıcıyı başlık çubuğundaki küre düğmesi ya da "Toggle Browser" açar. Sohbetler bölününce de panel tektir ve son çalışılan sohbeti izler; aynı editör iki yerde çizilmesin diye sohbet başına panel yoktur. Panel başlığından liste katlanır, panel pencereyi kaplar ya da kapanır; açık sayfanın ikonuna tekrar basmak da kapatır. Pencereyi kaplarken kapatılan panel yine kaplayarak açılır (yeniden başlatmada saklanmaz). Komutlar: "Show Chat" (Ctrl+Alt+I, macOS'ta Ctrl+Cmd+I), "Show Files" (Ctrl+Shift+E, macOS'ta Cmd+Shift+E), "Show Changes" (Ctrl+Shift+G), "Search in Files" (Ctrl+Shift+F, macOS'ta Cmd+Shift+F).
- **Tarayıcı:** Kiln tarayıcı taşımaz; kurulu Chromium tabanlı tarayıcıyı (Chrome, Edge, Brave, Chromium, Vivaldi) penceresiz çalıştırır ve `kiln-browser` üzerinden Chrome DevTools Protocol ile sürer. Bağlantı sürecin kendi borularıdır, port açılmaz. Profil Kiln'indir (veri klasöründe `browser`), kullanıcının profiline dokunulmaz; çerezler session'lar arasında ortaktır. Uygulama başına tek tarayıcı, session başına tek sayfa vardır; tarayıcı ilk adres açılınca başlar, son sayfa kapanınca biter. Sayfa JPEG kareler olarak gelir, arka planda çözülür ve çizilir; kareler pencere boyandıkça onaylanır, bu yüzden örtülü pencere için çizim yapılmaz. Panel kapanınca kareler durur, 10 dakika görülmeyen sayfa kapanır ve gösterilince yeniden açılır. Telefon boyutu dokunmatik girişi ve telefon kimliğini de kullanır. Yeni pencere isteyen bağlantılar aynı panelde açılır. Aynı anda birden fazla Kiln çalışırsa her biri kendi profilini kilitler (`browser`, `browser-2`, …); ilkinden sonrakiler girişleri paylaşmaz ve bunu söyler. Karelerde görünmeyen şeyleri her belgede, sayfanın erişemediği ayrı bir dünyada çalışan `guest.js` bildirir: imleç, sağ tık hedefi ve sistemin çizeceği standart `<select>` listesi, ki onu Kiln çizer; sayfanın kendi çizdiği listelere karışılmaz. Sayfa diyalogları panelin içinde sorulur ve cevap bekler. Dosya alanları sistemin dosya penceresini açar. İndirmeler önce profilde toplanır, bitince İndirilenler'e var olanın üzerine yazmadan taşınır ve panelin altında listelenir. Sayfa kendi izniyle pano okuyamaz: kopyala ve yapıştır Kiln üzerinden geçer; metin ve resim yapıştırılır. Sayfa sohbete de verilir: seçilen öğe (seçicisi, HTML'i, çerçevenin bildirdiği bileşen ve kaynak dosyası) ve resmi, sayfanın ekran görüntüsü ve konsol hataları, gönderilecek mesaja eklenir. Sayfanın yazdığı metin, ajana bunun talimat değil veri olduğu söylenerek verilir. Sayfanın altında geliştirici araçları paneli vardır: Console (sayfanın yazdıkları ve hataları), Network (`Network` alanından gelen istekler; adı, durumu, türü, boyutu, süresi, istek ve cevap başlıkları ve tarayıcı hâlâ tutuyorsa cevabın kendisi; yeni belgede temizlenir), Storage (sitenin çerezleri, local ve session storage'ı ve `IndexedDB` depolarının ilk kayıtları; tek tek silinir ya da sitenin bütün verisi temizlenir) ve Usage (sayfanın script belleği, öğe ve dinleyici sayısı; tarayıcının bütün süreçlerinin işlemci ve belleği, `kiln-services`'in süreç ağacından). Tarayıcı bütün sayfalar için tektir, bu yüzden süreç kullanımı sayfaya değil tarayıcıya aittir. Usage yalnızca görünürken ölçülür. Sohbetin ajanı tarayıcıyı Kiln'in araçlarıyla kendisi kullanır: `browser_navigate`, `browser_snapshot` (sayfanın erişilebilirlik ağacından çıkan, denetimlere `ref` veren taslak), `browser_click`, `browser_type`, `browser_press_key`, `browser_screenshot`, `browser_console`, `browser_evaluate`, `browser_questions` ve `browser_answer`. Tıklama ve yazma gerçek fare ve klavye olaylarıdır. Ajan tarayıcıyı kullanınca, sohbet açık olan sohbetse panel kendiliğinden açılır; klavye kullanıcıda kalır. Ajanın yaptığı panelde görünür: kullanacağı öğe, kullanmadan kısa bir süre önce çerçevelenir; tıkladığı yerde bir halka açılır; sayfanın altındaki satır ne yaptığını söyler ("Clicking “Save”"). Bunlar Kiln'in sayfanın üstüne çizdiğidir, sayfaya bir şey eklenmez. Web Bluetooth'un cihaz seçimi tarayıcının kendi penceresidir ve görünmez tarayıcıda çizilmez; Chrome'un `DeviceAccess` bildirimiyle gelen liste sayfanın üstünde bir kartta gösterilir, bulunan cihazlar eklendikçe büyür, seçim ya da iptal tarayıcıya geri verilir. Test, Chrome'un `BluetoothEmulation`'ının tanıttığı sahte bir cihazla yapılır. WebUSB, WebHID ve Web Serial'in ise görünmez tarayıcıda ne seçicisi ne cihazı vardır; bu yüzden Kiln cihazlara kendisi bağlanır. Sayfanın kendi dünyasına konan `native.js`, `navigator.usb`, `navigator.hid` ve `navigator.serial`'ı aynı API'yi veren nesnelerle değiştirir ve her çağrıyı Kiln'e iletir. Kiln cihazları `kiln-devices` ile listeler (USB için `nusb`, HID için `hidapi`, seri port için `serialport`), seçimi Bluetooth'unkiyle aynı kartta sorar, seçilen cihazı o site için Kiln kapanana kadar hatırlar ve çağrıları UI dışında yürütür: USB'de yapılandırma ve arayüz seçimi, kontrol, bulk ve interrupt aktarımları, sıfırlama; HID'de rapor tanımlayıcısından çıkan koleksiyonlar, gelen raporlar, çıkış ve feature raporları; seri portta ayarlar, okunur ve yazılır akışlar ve sinyaller. Tarayıcının sayfalardan sakladıkları burada da saklanır (korumalı USB sınıfları, klavyeler ve güvenlik anahtarları). Seçici açıkken liste saniyede bir yenilenir, sonradan takılan cihaz eklenir. Cihazın Kiln'deki kimliği yeniden takılınca da aynı kalır: seri numarası varsa onunla (ve HID ile seri portta arayüzüyle), yoksa takılı olduğu portla; böylece site cihazı yeniden seçmeden kullanmaya devam eder. Verilen cihazlar birkaç saniyede bir aranır ve takılıp çıkarılmaları sayfaya bildirilir; belge değişince açık cihazlar kapanır. İzokron aktarımlar desteklenmez; Windows'ta arayüz almadan kontrol aktarımı yapılamaz; seri numarası olmayan bir cihaz başka bir porta takılırsa yeniden seçilmelidir. Kamera, mikrofon, konum, bildirim ve MIDI izinlerini görünmez tarayıcı kimseye sormadan reddeder. Bu yüzden sayfanın kendi dünyasına, sayfanın scriptlerinden önce bir script (`permission.js`) konur: izin isteyen çağrılar (`getUserMedia`, `geolocation`, `Notification.requestPermission`, `requestMIDIAccess`) önce Kiln'e sorar. Kiln kullanıcıya sitenin adıyla sorar, cevabı `Browser.setPermission` ile tarayıcıya verir ve çağrı sonra sayfanın yaptığı gibi devam eder. Tarayıcı bu izinleri kapanınca unutur; Kiln cevapları profil klasöründeki `kiln-permissions.json`'da tutar ve her sayfa açılışında yeniden verir. Cevaplanmış bir site için adres çubuğunun yanında bir kilit belirir; oradan izinler görülür ve sıfırlanır. Görünmez tarayıcı bildirim göstermez, sayfanın bildirimlerini Kiln kendisi gösterir (sayfanın ve service worker kaydının açtıkları; arka planda push ile gelenler henüz değil). Kamera ve mikrofonu macOS da sorar; izin, Kiln'i başlatan uygulamaya verilir. Ajan da bu soruları okur ve kullanıcı yerine cevaplar: `browser_questions` açık diyaloğu, cihaz seçicisini (Bluetooth, USB, HID ya da seri) ve bulunan cihazları ve bekleyen izin isteğini söyler; `browser_answer` diyaloğu kabul eder ya da kapatır, cihazı id'si ya da adıyla seçer, izni verir ya da reddeder. Ajanın verdiği izin yalnızca o istek içindir, site için kalıcı olan kullanıcının cevabıdır. Sayfa bir şey sorarken bekler; ajanın soruyu doğuran tıklaması ya da scripti, soru gelince cevap beklemeden döner ve soruyu söyler. Bu makinedeki sayfalar (localhost, 127.x, `*.localhost`, dosyalar) serbesttir; başka bir site için panelde bir kez izin sorulur, verilen izin sohbet kapanana kadar geçerlidir, cevap gelene kadar çağrı bekler. Ajana giden sayfa metni, talimat değil veri olduğu belirtilerek verilir. Henüz yok: tarih ve renk seçicilerin açılan pencereleri, pano okuma ve ekran paylaşımı izinleri, farklı siteden gömülü çerçevelerde `<select>`, imleç ve sağ tık. Windows'ta henüz çalışmaz.
- **Files:** solda projenin dosya ağacı (gitignore'a uyar, boş klasörler de gösterilir), sağda editör. Dosyaya tıklamak onu editörde açar; dosya yokken "Select a file to open it." yazar. Hızlı dosya açma, ayar dosyaları ve tool kartındaki dosya yolu dosyayı burada açar ve yan panelde Files'ı gösterir; ağaçta dosyanın klasörleri açılır ve dosya seçilir.
- **Aktivite çubuğu:** varsayılan 48 px; kenarından 40–96 px arası genişler, düğme ve ikonlar onunla ölçeklenir, çift tık varsayılana döndürür. Ctrl+B / Cmd+B ya da başlık çubuğundaki düğme çubuğu ve paneli birlikte gizler. Genişlikler, sayfa ve gizlilik veri klasöründeki `side-panel.json`'da tutulur.
- **Dosya ağacı:** üstte New File, New Folder, Refresh, Collapse All; sağ tık menüsünde ayrıca Reveal in Finder, Copy Path, Copy Relative Path, Rename, Delete (çöpe taşır). F2, Delete ve Enter kısayolları vardır. Editörde sekme değişince dosya ağaçta seçilir.
- **Editör:** son commit'e (HEAD) göre değişen satırlar satır numarası yanında, kaydırma çubuğunda ve minimap'te işaretlenir; sekme çubuğundaki düğme dosyayı diff olarak gösterir. Minimap `editor_pane/minimap`'te çizilir (gpui-kit'te yok) ve `editor.minimap.enabled` ile kapatılır. Kaydedilmemiş olma, metnin diskteki haliyle karşılaştırılmasıdır; geri almak işareti kaldırır. Kaydedilmemiş sekme kapanırken Save / Don't Save / Cancel sorulur. Görseller (png, jpg, gif, webp, svg, bmp, ico, tiff) metin olarak okunmaz, resim olarak gösterilir; kaydedilmez. Markdown dosyalarında sekme çubuğundaki "Open Preview" düğmesi kaynak ile işlenmiş görünüm arasında geçer. Görünüm yazdıkça güncellenir; göreli yoldaki görseller dosyanın klasöründen yüklenir. Diskte değişen, kaydedilmemiş değişikliği olmayan dosyalar yeniden okunur.
- **Git:** `kiln-git` üzerinden kullanıcının kendi `git`'iyle okunur. Solda branch, upstream farkı, toplam değişiklik ve katlanabilir bölümler: "Merge Conflicts", "Staged", "Not Staged", "Untracked". Her satırda durum harfi, dosya adı, klasörü ve satır sayıları (+/−) bulunur; bölüm başına en fazla 300 satır listelenir. Sağda seçilen dosyanın yolu, durumu ve diff'i (gpui-kit `Diff`, Unified/Split), önceki/sonraki değişiklik ve "Open in Files" düğmeleri vardır. Liste odaktayken yukarı/aşağı değişiklikler arasında gezer, Enter dosyayı Files'ta açar. Hem stage'li hem sonradan değişmiş bir dosya iki bölümde ayrı görünür. Branch adı bir menü açar: yerel branch'ler en son commit alanı önce listelenir, tıklanan branch'e `git switch` ile geçilir; çalışma ağacındaki değişiklikler gelir, aynı dosyaları değiştiren bir branch'e geçişi git reddeder ve sebebi gösterilir. "Create Branch…" bir isim kutusu açar; Enter branch'i mevcut commit'ten oluşturup ona geçer (`git switch --create`), Escape kapatır. Elle stage ve commit: satırın üzerine gelince (ya da satır seçiliyken) çıkan düğme dosyayı stage'ler, stage'li olanı geri alır, çakışmayı çözüldü işaretler; bölüm başlıklarında hepsi için aynı düğme vardır. Branch satırının altında commit mesajı kutusu ve "Commit" düğmesi bulunur; Cmd/Ctrl+Enter de commit eder. Commit kullanıcının `git`'iyle (`git commit --file=-`) çalışır, kullanıcının hook'ları ve kimliği geçerlidir; hook reddederse çıktısı kırmızıyla gösterilir. Yazma işlemleri sırayla, arka planda çalışır ve sonra durum yeniden okunur. Mesaj önce commit hattının temizleme ve doğrulama adımlarından geçer (§8, ayarlarda `git.commit`: `strip`, `subject_max`, `conventional`); kurala uymayan mesaj commit edilmez, sebebi kırmızıyla yazılır. Okuma `--no-optional-locks` ile yapılır, böylece aynı checkout'ta çalışan ajanın `git` komutları `index.lock` yüzünden başarısız olmaz. Proje reponun alt klasörü olabilir. Klasör bir repo değilse bunu söyler. Listenin altında "History" son 200 commit'i (konu, kısa hash, yazar, zaman) gösterir; bir commit seçilince sağda o commit'in ilk ebeveynine göre diff'i açılır.
- **Search:** projede metin arar (`kiln_project::search`). Files listesindeki dosyalara bakar, yani `.gitignore` ve `files.exclude` geçerlidir; 2 MB'tan büyük ve ikili dosyalar atlanır, en fazla 2000 sonuç gösterilir. Varsayılan büyük/küçük harfe duyarsızdır, "Aa" duyarlı yapar. Yazmak 250 ms sonra arar ve önceki aramayı durdurur. Sonuçlar dosya başlıkları altında gruplanır; bir sonuca tıklamak dosyayı yan editörde o satırda açar.
- **Durum çubuğu:** solda tüm ajanların süreç ölçeri ve proje yolu, sağda açık dosya ve Claude Code sürümü. Sürüm `claude --version`'dan okunur. En yeni sürüm, CLI'ın `claude update` komutunun baktığı yerden açılışta ve saatte bir `curl` ile sorulur (Kiln bir HTTP istemcisi taşımaz): Homebrew cask'ı için `formulae.brew.sh`'daki formülü, diğer kurulumlar için `downloads.claude.ai/claude-code-releases/<kanal>` (`autoUpdatesChannel`, varsayılan `latest`). Kurulum türü, `PATH`'teki `claude`'un gerçek yolundan anlaşılır (`Caskroom/<cask>`, `node_modules`, `share/claude/versions`). Sürüm güncelse nokta yeşil, daha yenisi varsa sarıdır. Tıklayınca açılan panelde kurulu sürüm, en yeni sürüm, kurulum türü ve "Update Claude Code" düğmesi vardır. Düğme CLI'ın önerdiği komutu arka planda çalıştırır: Homebrew'da cask'ı kuran `brew` ile `brew upgrade <cask>`, diğerlerinde `claude update`. Yeni sohbetler yeni sürümü kullanır. Her sohbetin süreci hangi sürümle çalıştığını `init` mesajındaki `claude_code_version` ile söyler; kurulu sürümden farklı olan açık sohbet varsa panel "Restart Chats on This Version" düğmesini gösterir. Düğme bu sohbetlerin sürecini yeniden başlatır ve her biri `--resume` ile konuşmasına devam eder; tur süren sohbet tur bitince yeniden başlar. Kurulum türü anlaşılamazsa düğme yerine kurulduğu yolla güncellenmesi söylenir.
- **Ortak durum:** Git okuması `RepoStatus`'ta tutulur. Files ağacı da bunu kullanır: değişen dosya renkli ve durum harfiyle, içinde değişiklik olan klasör noktayla görünür. Durum ve dosya listesi Files veya Git açıldığında, pencere öne geldiğinde ve bir tur bittiğinde yenilenir; editörde kaydetmek de durumu yeniler. Listeleme açık klasörleri korur.
- Aynı checkout'ta çalışan iki oturumun değişiklikleri git açısından tek bir çalışma ağacıdır ve ayrılamaz; oturum başına diff, oturum başına worktree ile gelir (§7).
- Sohbetler gpui-kit dock'unun orta alanındadır: her sohbet bir sekme ve kendi CLI süreci; sekmeler gruplar arasında sürüklenir, gruplar yan yana ya da alt alta bölünür. Gösterilmeyen sekmenin süreci çalışmaya devam eder. Uyku modu: `chat.sleep_after_minutes` (varsayılan 30, 0 kapalı) kadar boşta kalan sohbetin süreci kapanır; sıradaki mesaj süreci aynı konuşmayı `--resume` ile sürdürerek yeniden başlatır. Süre turun bitişinden, ya da son arka plan görevinin bitişinden sayılır. Çalışan tur, subagent, bekleyen izin sorusu ya da arka planda çalışan görev (CLI `background_tasks_changed` ile bildirir, `AgentEvent::BackgroundTasks`) sohbeti uyanık tutar, çünkü süreçle birlikte biterlerdi.
  - Sekme çubuğunda "New session" (aynı grupta yeni sekme) ve "Split right" (yanda yeni grup) düğmeleri var. Her sekme kapatılabilir, sonuncusu da: dock son sekmeye kapatma düğmesi koymadığı için o sekme başlığında kendi ✕ düğmesini çizer ve Kiln paneli dock'tan kaldırır. Sekme kalmayınca pencere son projeleri ve "Open Folder…"u gösterir.
  - Yeni sekme, son çalışılan sohbetin izin modunda başlar (bypass hariç); kullanıcının ayarındaki mod yalnızca ilk sekmede ya da mod bilinmiyorsa geçerlidir.
  - Yeni sekme ve bölme, kullanıcının en son tıkladığı (metin ya da odak almayan düğmeler dahil) ya da klavyeyle odakladığı sohbetin yanına açılır. Dock'un düzeni kurarken her grubun sekmesini göstermesi bunu değiştirmez; açılışta geri yüklenen düzende bu sohbet ilk gruptakidir.
  - Sohbet başlığındaki konuşma adına tıklanınca, subagent ve işlemler sayfaları gibi sohbetin üstünde bir "Conversations" sayfası açılır: projede saklanan konuşmalar en yeniden eskiye, gün başlıkları altında, ada göre aranabilir; turu süren ve başka pencerede açık olanlar işaretlidir. Bir konuşmaya tıklamak onu bu sohbette açar, sohbetin önceki konuşması yerini ona bırakır. Konuşma başka bir sekmede açıksa o sekmeye geçilir, böylece bir konuşmayı iki süreç sürdürmez. Sohbette tur sürüyorsa konuşması korunur, seçilen yanında yeni sekmede açılır. Gösterilen konuşmaya tıklamak, Escape ya da kapatma düğmesi sayfayı kapatır. Sayfa açılınca arama kutusuna değil sayfaya odaklanır. Sohbet başlığı ve sekme, konuşmanın bu listedeki adını gösterir (kullanıcının verdiği ad, CLI'nin yazdığı başlık ya da son prompt); liste her tur sonunda yenilendiğinde ad da güncellenir. Henüz saklanmamış yeni bir konuşma ilk mesajıyla adlanır.
  - Sohbet yerleşimi pencere başına tek kayıt olarak `<data>/layouts.json`'da tutulur: her sekmenin klasörü, gruplar, bölmelerin yönü ve boyu, her gruptaki konuşmalar, her sekmenin hesabı, izin modu ve eforu, gösterilen sekme. Kaldırılmış bir hesabın sekmesi ilk eklenen hesapla açılır. İzin modu ya da eforu kaydedilmemiş bir sekme kullanıcının kendi ayarındakiyle açılır. Dock her değiştiğinde, bir sohbetin konuşması, hesabı, modu ya da eforu değiştiğinde yazılır. Boş sekmeler konuşmasız tutulur ve klasörlerinde boş açılır.
  - Açılışta yerleşim saklanan konuşmaların listesi yüklenince kurulur: artık saklanmayan konuşmalar atlanır, başka bir süreçte açık olan kopya olarak sürer. Her sekme kendi sürecini başlatır. Hiçbir konuşma kalmazsa tek boş sohbetle başlanır.

## 14. Ayarlar, tuş atamaları, tema

- **Ayarlar:** JSONC (Zed tarzı iç içe snake_case anahtarlar), katmanlı (gömülü varsayılanlar → kullanıcı → proje → `.kiln/local.json` [gitignore] → sohbet). Hot-reload ve şema doğrulaması var; hatalı alan uyarı verir, uygulama çökmez. Her projenin katmanları ayrı okunur ve izlenir (`config::settings_at`), yani bir sohbet gösterilen projenin değil kendi projesinin ayarlarıyla çalışır. Sohbet katmanı bellekte tutulur ve sohbetle birlikte yerleşim dosyasına (`SavedChat.settings`) yazılır.
- **Commit ayarları sayfası** (`commit_settings`, sheet): sohbet sekmesindeki commit düğmesinden ve şeritteki dişliden "This chat", Git sayfasındaki dişliden "This project", komut paletinden (`kiln.openCommitSettings`) etkin sohbetin sekmesiyle açılır; sekmeler "This chat / This project / All projects". Her satır değerin hangi katmandan geldiğini ("Set here", "From this project", "Default") ve o katmanda ayarlıysa "Reset" düğmesini gösterir. Değişiklik hemen kaydedilir: genel ve proje katmanında jsonc-parser CST ile dosyanın yorumları korunarak (`kiln_settings::set_in_file`), sohbette bellekte. Bir katman yalnızca kendi yazdığını ezer; daha dar katman kazanır (sohbet > proje > genel).
  Örnek anahtarlar (nokta iç içe nesneyi gösterir): `ui.font_size`, `ui.font_family`, `editor.font_size`, `editor.font_family`, `editor.tab_size`, `theme`, `agents.claude.binary`, `agents.codex.binary`, `git.commit.trigger`, `git.commit.review`, `git.commit.writer`, `git.commit.instructions`, `git.commit.require`, `git.commit.forbid`, `git.commit.strip`, `git.push.policy`, `permissions.rules`, `sessions.hibernate_after`.
- **Tuş atamaları:** VSCode `keybindings.json` formatı birebir (`key`, `command`, `when`, `-command` ile kaldırma).
  - Komut id'leri karşılığı olan yerlerde VSCode isimlerini kullanır (`workbench.action.quickOpen`, `workbench.action.showCommands`, `workbench.action.toggleAuxiliaryBar` …). Kiln'e özgü komutlar `kiln.*` altındadır.
  - Varsayılanlar VSCode'un varsayılanlarıdır (macOS'ta Cmd). VSCode kullanıcı dosyası tek tıkla içe aktarılır.
- **Tema:** gpui-kit'in tema JSON formatı (`ThemeSet`) doğrudan kullanılır; ayrı bir format ve dönüştürücü yazılmaz. UI renkleri ve sözdizimi renkleri (tree-sitter capture'ları) aynı dosyadadır. gpui-kit'in Zed'den uyarlanmış hazır temaları da aynı formatta olduğu için ileride içe aktarılabilir.
  - Görünüm: Zed'in boşluk ve tipografisi + VSCode Dark/Light Modern paleti.
  - Boyut standardı (`kiln_chat::scale`; 14 px arayüz yazısında, hepsi yazıyla birlikte ölçeklenir):
    - Çubuk, composer şeridi ve diyalog düğmeleri (`control*`): 24,5 px yükseklik, 16 px ikon, `text_sm`.
    - Satır ve panel içi düğmeler (`row*`): 21 px yükseklik, 14 px ikon; metin yanındaki ikon da 14 px (`in_row`).
    - İkincil bilgi `text_caption` (11,5 px); `text_xs` ve `xsmall` kullanılmaz.
    - Panellerin üst çubukları dock'un sekme çubuğuyla aynı hizada biter (`BAR_HEIGHT`). Dock'un kendi düğmeleri kütüphanenin boyutundadır. Aktivite çubuğu kendi ölçeğindedir.
  - İki tema: **Kiln Dark**, **Kiln Light**, bir de `system` seçeneği.

## 15. Ajan bağlamı paneli

- Proje ve hesap bazında listelenenler: CLAUDE.md / AGENTS.md zinciri (kullanıcı, proje, alt dizinler), skill'ler (ad, açıklama, kaynak), subagent tanımları, MCP sunucuları (init olayındaki bağlantı durumuyla), hook'lar, izin kuralları.
- "Summarize" butonu: küçük modelle özet; içerik hash'iyle önbelleğe alınır.
- Öğeye tıklamak dosyayı editörde açar.

## 16. Performans bütçesi

| Metrik | Hedef |
|---|---|
| İlk kare | < 300 ms |
| Boştaki CPU | ~%0 (iş yokken polling yok) |
| Kiln RSS (1 proje, 3 oturum, CLI süreçleri hariç) | < 150 MB |
| 10k mesajlı sohbette kaydırma | 120 fps, virtual list |

Referans: boş gpui-kit penceresi ≈ 93 MB RSS (Linux/Wayland). Yani 150 MB hedefinde uygulamanın kendisine ≈ 55 MB kalır. M0 sonunda ölçülür; aşılırsa önce gpui-kit'in kullanılmayan feature'ları kapatılır.

CLI süreçleri Kiln'in kontrolünde değildir. Onlar için çözüm uyku modudur (§4).

## 17. Kod kalitesi kapıları

Aşağıdaki lint ve `clippy.toml` konfigürasyonu rustc/clippy 1.100.0 üzerinde denendi: bilinmeyen lint veya anahtar uyarısı yok. `cargo-deny`, `cargo-machete` ve `rust-code-analysis-cli` bu makinede kurulu değil; M0'da kurulup doğrulanacak.

Tek komut: **`cargo xtask check`**. CI (GitHub Actions; Linux + macOS, Windows sadece derleme) tam halini çalıştırır. Git pre-commit hook'u (`cargo xtask install-hooks` ile kurulur) ve Kiln'i geliştiren ajanların Stop hook'u hızlı alt kümeyi (`--fast`: kilit, özel kontroller, fmt, clippy) çalıştırır. Testler ve `cargo deny` / `cargo machete` tam kontrolde koşar. UI davranışı gpui-kit `test-support` ile headless pencerede gerçek tuş olaylarıyla test edilir (`crates/kiln-workspace/tests/ui.rs`). Lint setleri sürümler arasında değiştiği için toolchain `rust-toolchain.toml` ile sabitlenir.

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo deny check`: lisans izin listesi Apache-2.0 / MIT / BSD / ISC / Zlib / Unicode / MPL-2.0. GPL yasak, böylece Zed'in GPL kodu kazara giremez.
5. `cargo machete` (kullanılmayan bağımlılıklar)
6. Cyclomatic complexity: `syn` tabanlı xtask kontrolü, fonksiyon başına ≤ 10. Dış araca bağımlı değildir, kural seti testlerle sabitlenmiştir.
7. Dosya başına ≤ 400 satır.
8. Atlatma kontrolleri (aşağıda).

**Workspace lint'leri** (`Cargo.toml`):
```toml
[workspace.lints.rust]
unsafe_code = "deny"            # sadece kiln-sys içinde #[expect] ile açılır

[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "deny", priority = -1 }
cognitive_complexity = "deny"
excessive_nesting = "deny"
allow_attributes = "deny"                  # #[allow] yasak → #[expect] zorunlu (gereksizse kendisi hata verir)
allow_attributes_without_reason = "deny"
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
print_stdout = "deny"
print_stderr = "deny"
missing_errors_doc = "allow"               # uygulama kodu, public API değil
missing_panics_doc = "allow"
module_name_repetitions = "allow"
```

**`clippy.toml`:**
```toml
cognitive-complexity-threshold = 15
too-many-lines-threshold = 60
too-many-arguments-threshold = 5
type-complexity-threshold = 250
excessive-nesting-threshold = 4
max-fn-params-bools = 2
allow-unwrap-in-tests = true
allow-expect-in-tests = true
```

**Atlatmaya karşı önlemler (deterministik):**
- Her crate'te `[lints] workspace = true` zorunludur; xtask kontrol eder.
- Crate seviyesinde `#![allow]` ya da `#![expect]` yasaktır.
- `#[expect(...)]` sayısı `quality/expect-baseline.txt` ile karşılaştırılır. Artış, baseline aynı değişiklikte güncellenmedikçe hata verir; böylece her istisna diff'te görünür.
- `unsafe` sadece `kiln-sys` içinde olabilir.
- Lint ve kalite konfigürasyonunun (`[workspace.lints]`, `clippy.toml`, `deny.toml`, `rustfmt.toml`, `rust-toolchain.toml`, `quality/expect-baseline.txt`, xtask kaynakları, CI workflow'u, pre-commit hook'u, `.claude/settings.json`) hash'i `quality/config.lock` dosyasında tutulur. Değişiklik kilidin de bilinçli olarak güncellenmesini gerektirir (`cargo xtask lock-config`). Ajanlara bu komut ve bu dosyalara yazma `.claude/settings.json` ile yasaktır.
- Kiln'i geliştiren ajanlar için Claude Code hook'ları: bu dosyalara Edit/Write reddedilir, `--no-verify` reddedilir.
- `#[ignore]` sebep belirtilmeden kullanılamaz.

**Test stratejisi:**
- Mantık crate'leri birim testleriyle test edilir.
- Agent backend'leri gerçek fixture'ların tekrar oynatıldığı sözleşme testleriyle test edilir.
- UI, gpui-kit'in headless test desteğiyle duman testinden geçer.

## 18. Kilometre taşları

| | Kapsam | Çıktı |
|---|---|---|
| **M0 Temel** | Workspace, kalite kapıları, CI. GPUI pencere + dock, ayarlar + hot-reload, iki tema, VSCode keymap motoru. Dosya ağacı + renkli görüntüleyici, hızlı dosya açma | Boş ama sağlam iskelet |
| **M1 Claude oturumları** | Hesap ekleme / giriş / rozet, eşzamanlı oturumlar, tool kartları, izin istekleri + kurallar. Subagent ağacı + model/context, kota ölçeri, sohbet içi arama, görsel/dosya yapıştırma, interrupt, resume | **İlk kullanılabilir sürüm** |
| **M2 Git** | Diff görünümü, MCP köprüsü + `commit` aracı, commit hattı (politika, inceleme, temizleme), push politikası. Oturum başına worktree + merge/discard, uyku modu | Tamam (2026-11-26). Worktree isteğe bağlı; kurulum komutları M3'te, MR/PR M5'te, `commit-msg` hook'u ve "görev bitince" tetikleyicisi yok |
| **M3 Servisler** | Run konfigleri, denetleyici, loglar, ajan süreç izleyici, `kiln_service_*`, yapay zekayla konfig tanımlama | |
| **M4 Codex + koordinasyon** | Codex backend, `kiln_sessions_list`, `kiln_message_send` | Codex backend, hesapları, geçmiş/resume, kota, commit yazıcısı tamam (2026-11-26). Codex'e Kiln araçları (MCP köprüsü), skill/hook listesi, Codex CLI sürüm kontrolü ve `agents.codex.binary` ayarı yok; `kiln_sessions_list`, `kiln_message_send` yok |
| **M5 Cila** | gh/glab, ajan bağlamı paneli + özetler, bildirimler, Windows | |

**M1 dilimleri:** M1 tek seferde değil, kullanılabilir dilimlerle gelir.
1. **M1a (ilk kullanılabilir):** Varsayılan hesapla tek Claude oturumu. Akan metin, tool kartları, izin kartı (Allow/Deny), composer, Stop.
2. **M1b:** Hesaplar ve rozet, izin kuralları, subagent ağacı, kota ölçeri, sohbet içi arama, görsel/dosya yapıştırma, resume, eşzamanlı oturumlar.

## 19. Spike'lar ve riskler

| # | Konu | Ne zaman |
|---|---|---|
| S0 | ✅ gpui-kit 0.10 boş pencere: Linux (Wayland, RSS ≈ 93 MB) ve macOS 27, günlük kullanımda. Eski macOS sürümleri hedeflenmez | M0 başı |
| S1 | ✅ macOS'ta kimlik deposu başına ayrı Keychain kaydı; ortak `oauthAccount` son girişi gösterir, Kiln ona dayanmaz (§0, §5) | M1 başı |
| S2 | ✅ Claude: kontrol protokolüyle terminalsiz giriş (§5). ✅ Codex: app-server `account/login/start` (§5) | M1 başı |
| S3 | ✅ Claude: context sayısı CLI'ın kendi hesabıdır ve tur sürerken de güncellenir (§12, Context ölçeri). Codex: `thread/tokenUsage/updated` yalnızca toplam verir, kırılım yok | M1 |
| S4 | gpui-kit editörü: LSP tamamen kapalı mı, salt okunur mod, tema eşlemesi, diff bileşeni yeterli mi? Çok büyük dosyalar öncelikli değil | M0 |
| S5 | Codex app-server "experimental": olay akışı Codex CLI 0.172.0'da canlı denendi, fixture'lar `kiln-agent-codex/tests/fixtures`'ta. Tipler `schema/generate.py` ile kurulu sürümün şemasından üretilir; tanınmayan varyant `Unrecognized`'a düşer. Sürüm aralığı sabitleme açık | M4 |
| S6 | Claude stream-json/control protokolü resmi kararlı API değil → izolasyon + fixture testleri + sürüm kontrolü | M1 |
| S7 | Çalışan bir Claude oturumuna tur ortasında mesaj kuyruklama (sonuçta `queued_turn_count` alanı var) | M4 |
| S8 | ✅ Kullanıcı birden fazla aboneliği yıllardır sorunsuz kullanıyor; açık değil | M1 öncesi |

## 20. Veri konumları

- Yapılandırma: `directories` crate'iyle platform dizini (Linux `~/.config/kiln/`): `settings.json`, `keybindings.json`, `themes/`.
- Veri: oturum veritabanı, olay günlükleri, hesap dizinleri, worktree'ler, sohbet yerleşimleri (`layouts.json`).
- Proje: `.kiln/settings.json`, `.kiln/run.json` (commit'lenebilir), `.kiln/local.json` (gitignore).
