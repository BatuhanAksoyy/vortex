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
| Hesap izolasyonu (Claude) | ✅ Linux / ❓ macOS | `CLAUDE_CONFIG_DIR=<boş dizin> claude auth status` → `loggedIn:false`, ana giriş etkilenmiyor. macOS Keychain çakışması doğrulanmadı (S1) |
| Hesap izolasyonu (Codex) | ✅ | `CODEX_HOME=<boş dizin> codex login status` → `Not logged in`. Codex, `CODEX_HOME` `/tmp` altındaysa yardımcı binary oluşturmayı reddediyor → profil dizinleri kalıcı veri dizininde olmalı |
| Co-author kapatma | ⚠️ kısmen | `includeCoAuthoredBy` ayar adı Claude binary'sinde var, ama `--settings` ile verilip denenmedi; attribution başka bir anahtarla da yönetiliyor olabilir. Codex için ayar aranmadı. Garanti IDE tarafı temizlemedir (§8) |
| Codex yapılandırılmış I/O | ⚠️ sadece şema | `codex app-server` stdio üzerinden JSON-RPC (deneysel). Olay akışı canlı denenmedi (S5). `codex app-server generate-json-schema` şema üretiyor: `CommandExecutionRequestApproval`, `FileChangeRequestApproval`, `PermissionsRequestApproval`, `tokenUsage`, `RateLimitSnapshot`, `SubAgentActivity`, `collabAgentToolCall`, `SubagentStart/Stop` |
| UI çatısı | ✅ Linux | `gpui-kit 0.10.0` (Apache-2.0) `gpui-pre =0.3.12`'yi sabitliyor. İçinde dock, sekme, tree, tree-sitter'lı kod editörü (200K satır), diff, markdown, virtual list, mesaj kaydırıcı ve UI test desteği var. Boş pencere Wayland'da açıldı ve temiz kapandı. İlk release derlemesi 4 dk 37 sn, binary 51 MB (strip'siz), boş pencere RSS ≈ 93 MB |

## 1. Hedefler ve hedef olmayanlar

**Hedefler**
- Birden fazla Claude Code ve Codex aboneliğiyle aynı anda oturum çalıştırmak. Her oturum ve subagent hangi abonelikte çalıştığını göstersin.
- Ajanlar kendi resmi CLI'larıyla, abonelik üzerinden çalışsın; API anahtarı yok.
- Çok düşük kaynak kullanımı: GPU ile çizilen yerel arayüz, Chromium yok.
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
- Ajanlara Kiln'in araçları **MCP köprüsü** üzerinden verilir. `kiln mcp` aynı binary'nin bir alt komutudur, MCP stdio konuşur, isteği yerel soketle ana sürece iletir. Port açılmaz; oturum token'ı ortam değişkeniyle geçer. Claude'a `--mcp-config`, Codex'e `-c mcp_servers.kiln...` ile eklenir.
- Tek async çalışma zamanı: GPUI executor'ları + `async-process`/`async-channel` (executor-bağımsız). Tokio sadece `kiln-bridge` içinde (`rmcp` için) kullanılır.

## 3. Workspace yapısı

```
kiln/
├── assets/themes/kiln.json  Kiln Dark / Kiln Light (gpui-kit tema formatı)
├── crates/
│   ├── kiln/               bin: uygulama kabuğu, alt komutlar (`kiln mcp`)
│   ├── kiln-core/          domain tipleri: id'ler, Project, Account, AgentEvent
│   ├── kiln-settings/      JSONC katmanlı ayarlar, hot-reload
│   ├── kiln-keymap/        VSCode keybindings.json formatı
│   ├── kiln-project/       dosya listeleme (gitignore), dil tespiti, bulanık arama
│   ├── kiln-workspace/     GPUI: pencere yerleşimi, dosya ağacı, editör, paletler
│   ├── kiln-agent/         AgentBackend trait'i, oturum aktörü, izin motoru
│   ├── kiln-agent-claude/  stream-json + control protokolü
│   ├── kiln-agent-codex/   app-server JSON-RPC (şemadan üretilmiş tipler)
│   ├── kiln-accounts/      profiller, giriş akışı, kota anlık görüntüsü
│   ├── kiln-bridge/        ajanlara açılan MCP araçları + IPC
│   ├── kiln-services/      çalıştırma konfigleri, süreç denetleyici, süreç ağacı izleme
│   ├── kiln-git/           git CLI sarmalayıcı, worktree, commit hattı, trailer temizleme
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

**Oturum kalıcılığı:** Kiln kendi indeksini SQLite'ta tutar (oturum, proje, hesap, ajan, worktree, başlık). Ham olay günlüğü oturum başına JSONL. Devam ettirme: Claude `--resume <id>`, Codex thread resume.

**Uyku modu (kaynak tasarrufu):** Kiln'in kendisi hafiftir; RAM'in çoğunu CLI süreçleri harcar. Belirli bir süre boşta kalan oturumun süreci kapatılır, sıradaki mesajda `--resume` ile şeffafça geri açılır. Süre ayarlanabilir.

**Limit dolunca:** Oturum "Rate limited until HH:MM" durumuna geçer, sohbet görünür kalır, gönderme kapalıdır, sıfırlanınca açılır. Hesap başına kota çubuğu `rate_limit_event` / Codex `rateLimits` verisinden gelir.

## 5. Hesaplar

- Profil: `{id, label, color, agent, dir}`. Dizinler platform veri dizininde: `<data>/accounts/<id>/`.
- Giriş: Kiln `claude auth login` / `codex login` komutunu profil ortamıyla çalıştırır, tarayıcıyı açar, çıktıyı küçük bir diyalogda gösterir, kod yapıştırmak gerekirse stdin'e yazar (S2).
- **Paylaşılan katman:** Her `CLAUDE_CONFIG_DIR` kendi `settings.json`, `CLAUDE.md`, skill, agent ve MCP tanımlarına sahiptir; yani ikinci hesapta global skill'lerin görünmez. Kiln, ana dizindeki `skills/`, `agents/`, `commands/`, `CLAUDE.md`, `settings.json` dosyalarını profil başına seçmeli olarak symlink'ler (kimlik bilgileri ve oturum durumu hariç). Codex için `config.toml`, `AGENTS.md`, skill'ler aynı şekilde.
- Rozet: Hesap etiketi + rengi oturum sekmesinde, sohbet başlığında ve her subagent kartında görünür.

## 6. İzinler

- Katmanlar: **editör (global)** `settings.json` → **proje** `.kiln/settings.json` → **oturum** (geçici "bu oturum için izin ver").
- Ajan-bağımsız kural formatı:
  ```json
  { "tool": "shell", "pattern": "npm run *", "action": "allow" }
  ```
  Araç kategorileri: `shell`, `edit`, `read`, `web`, `mcp:<server>/<tool>`.
- Değerlendirme: herhangi bir `deny` eşleşirse ret → `allow` eşleşirse izin → aksi halde kullanıcıya sor. Her karar oturum günlüğüne yazılır.
- Kurallar tek yerde, Kiln'de değerlendirilir. Claude `can_use_tool`, Codex `*RequestApproval` istekleri aynı motordan geçer.
- İzin kartındaki "Always allow" butonu kuralı proje ya da global dosyaya yazar. Claude'un `permission_suggestions` alanı öneri olarak gösterilir.

## 7. Worktree'ler ve oturumlar arası koordinasyon

- Oturum açarken: **yeni worktree (varsayılan)** / ana checkout / var olan worktree.
- Konum: `<data>/worktrees/<repo-hash>/<slug>`, branch `kiln/<slug>`.
- Worktree kurulum komutları `.kiln/run.json` dosyasının `setup` alanındadır (ör. `.env` kopyala, `npm ci`). Bu adım olmadan Node projelerinde worktree işe yaramaz. Kullanıcı bu komutları elle yazmaz; ajana "bu proje için başlatma komutlarını ekle" der, ajan `kiln_run_config_propose` aracıyla önerir, kullanıcı Kiln'de görüp onaylar (§10).
- Kapanış: "Merge into base", "Open MR/PR", "Discard".
- Ajanlara MCP araçları:
  - `kiln_sessions_list` → eş oturumlar: id, hesap, ajan, branch, worktree, durum, başlık, değişen dosyalar.
  - `kiln_message_send(to, text)` → mesaj hedef oturumun turu bitince `[from session X]` etiketiyle iletilir. Token harcadığı için ayar: otomatik / kullanıcı onaylı / kapalı.
- Ajan bilgilendirmesi kısa bir ek sistem promptuyla yapılır (Claude `--append-system-prompt`, Codex developer instructions): uzun süreli süreçler için `kiln_service_*`, commit için `kiln_commit`, eşler için `kiln_sessions_list`.

## 8. Git

**Commit hattı (Kiln'e ait):**
1. **Tetikleyici** (proje bazında): manuel / her tur sonunda / ajan `kiln_commit` çağırınca / görev bitince.
2. **Mesaj kaynağı:**
   - a) Ajan `kiln_commit({message, paths?})` çağırır. En ucuzu bu, çünkü ajan ne yaptığını zaten biliyor ve git komutlarıyla uğraşmıyor.
   - b) Tur sonu tetikleyicisinde ajan çağırmadıysa: aynı hesapta tek seferlik küçük model çağrısı (`claude -p --model haiku`) diff + tur özetiyle. Model ve hesap ayarlanabilir. (Kullanıcı onayladı, 2026-11-19.)
   - c) Kullanıcı düzenler.
3. **İnceleme kapısı** (aç/kapa): diff + düzenlenebilir mesaj, Approve / Reject.
4. **Temizleme** (deterministik): ayarlanabilir regex listesine uyan satırlar silinir. Varsayılanlar: `Co-Authored-By:` + Claude/Codex/OpenAI/Anthropic, `Generated with [Claude Code]`, `🤖 Generated with`. Aynı fonksiyon MR/PR gövdelerinde de çalışır.
5. **Doğrulama:** Konu uzunluğu, Conventional Commits regex'i ve dil gibi kurallar deterministik kontrol edilir; geçmezse yeniden üretilir.
6. `git commit` kullanıcının kimliğiyle çalışır, böylece kullanıcının pre-commit hook'ları normal şekilde devreye girer.
7. **Push politikası:** asla / commit sonrası / manuel / oturum sonunda. Korumalı branch'lere (main/master) açık izin olmadan push yok.

**Ek önlemler:**
- Claude'a `--settings` ile `includeCoAuthoredBy: false` verilir.
- Politika IDE'ye aitken varsayılan izin kuralları ajanın `git commit*`, `git push*`, `--no-verify` komutlarını "use kiln_commit" mesajıyla reddeder.
- Opsiyonel `commit-msg` hook'u, ajanın Kiln dışında commit attığı durumlar için Kiln'in oluşturduğu worktree'lere kurulur.

**Arayüz:** gpui-kit diff bileşeni ile yan yana / satır içi diff. Oturum diff'i (worktree vs base), commit diff'i, çalışma ağacı. Git paneli: status, stage, kısa log, branch değiştirme.

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
- **Kiln dışında başlatılanlar:** Ajan `npm run dev &` ya da arka plan Bash ile süreç açarsa, süreç ağacı izleyici (`sysinfo`, sadece aktif oturum varken 1 sn aralıkla) her CLI sürecinin uzun ömürlü torunlarını "Agent processes" altında komut satırı, PID ve dinlenen portlarıyla listeler ve Kill butonu sunar.

## 11. Editör ve görüntüleyici

- gpui-kit kod editörü, LSP kapalı, tree-sitter renklendirme. Düzenleme açık ama arayüz okuma öncelikli.
- Dosya ağacı (`ignore` ile gitignore'a uyumlu), hızlı dosya açma (`nucleo`), proje araması (ripgrep kütüphaneleri `grep-searcher`), satıra git, markdown ve görsel önizleme.
- Dosya değişince (`notify`) otomatik yeniden yükleme ve HEAD'e göre değişen satır işaretleri.
- Diller: gpui-kit highlighter'ın getirdikleri + gerekirse eklenen grammar crate'leri (S4).

## 12. Sohbet arayüzü

- Claude Code CLI'a benzer akış: katlanabilir tool kartları.
  - Bash: komut + çıktı.
  - Edit: satır içi mini diff.
  - Read: yol + aralık.
  - Subagent: model, durum, token sayacı; tıklayınca subagent transcript'i ayrı panelde açılır.
- Markdown render, uzun sohbetler için virtual list.
- **Sohbet içi arama** (Ctrl+F): metin, tool girdileri ve isteğe bağlı tool çıktıları; önceki/sonraki ve vurgulama.
- **Composer:**
  - Çok satırlı yazma alanı.
  - Görsel yapıştırma (Claude image block, Codex local image).
  - Dosya/klasör sürükle-bırak → `@path` referansı.
  - Slash komutları (`initialize` cevabındaki liste).
  - Model ve izin modu seçici, Stop (interrupt).
- **Başlık:** ajan ikonu, hesap rozeti, model, context ölçeri, kota ölçeri, worktree/branch.

## 13. Yerleşim

```
┌ Title bar: project ▾ │ branch ▾ │ services ●● │ quota ▮▮▯ ──────────────────────┐
├────┬──────────────────┬─────────────────────────────┬──────────────────────────┤
│Act │ Sessions │ Files │  Chat (oturum sekmeleri)    │  Editor / Diff sekmeleri │
│bar │ Git │ Context    │                             │  (isteğe bağlı açılır)   │
├────┴──────────────────┴─────────────────────────────┴──────────────────────────┤
│ Alt panel (aç/kapa): Services · Agent processes                                │
└ Status bar ────────────────────────────────────────────────────────────────────┘
```
- Sohbet merkezde. Tool kartında bir dosyaya tıklamak onu sağda açar.
- "Focus chat" ve "Focus code" komutları yan panelleri gizler.
- M0'da yerleşim gpui-kit `h_resizable` panelleriyle kurulur. Sürüklenebilir sekmeler ve serileştirilebilir yerleşim gerekirse gpui-kit dock'a geçilir; dock, panel kaydı ve serileştirme gerektirdiği için ilk aşamada gereksiz karmaşıklık.

## 14. Ayarlar, tuş atamaları, tema

- **Ayarlar:** JSONC (Zed tarzı iç içe snake_case anahtarlar), katmanlı (gömülü varsayılanlar → kullanıcı → proje → `.kiln/local.json` [gitignore]). Hot-reload ve şema doğrulaması var; hatalı alan uyarı verir, uygulama çökmez.
  Örnek anahtarlar (nokta iç içe nesneyi gösterir): `ui.font_size`, `ui.font_family`, `editor.font_size`, `editor.font_family`, `editor.tab_size`, `theme`, `agents.claude.binary`, `agents.codex.binary`, `git.commit.trigger`, `git.commit.review`, `git.commit.strip_patterns`, `git.push`, `permissions.rules`, `sessions.hibernate_after`.
- **Tuş atamaları:** VSCode `keybindings.json` formatı birebir (`key`, `command`, `when`, `-command` ile kaldırma).
  - Komut id'leri karşılığı olan yerlerde VSCode isimlerini kullanır (`workbench.action.quickOpen`, `workbench.action.showCommands`, `workbench.action.toggleSidebarVisibility` …). Kiln'e özgü komutlar `kiln.*` altındadır.
  - Varsayılanlar VSCode'un varsayılanlarıdır (macOS'ta Cmd). VSCode kullanıcı dosyası tek tıkla içe aktarılır.
- **Tema:** gpui-kit'in tema JSON formatı (`ThemeSet`) doğrudan kullanılır; ayrı bir format ve dönüştürücü yazılmaz. UI renkleri ve sözdizimi renkleri (tree-sitter capture'ları) aynı dosyadadır. gpui-kit'in Zed'den uyarlanmış hazır temaları da aynı formatta olduğu için ileride içe aktarılabilir.
  - Görünüm: Zed'in boşluk ve tipografisi + VSCode Dark/Light Modern paleti.
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
| **M2 Git** | Diff görünümü, MCP köprüsü + `kiln_commit`, commit hattı (politika, inceleme, temizleme), push politikası. Oturum başına worktree + merge/discard, uyku modu | |
| **M3 Servisler** | Run konfigleri, denetleyici, loglar, ajan süreç izleyici, `kiln_service_*`, yapay zekayla konfig tanımlama | |
| **M4 Codex + koordinasyon** | Codex backend, `kiln_sessions_list`, `kiln_message_send` | |
| **M5 Cila** | gh/glab, ajan bağlamı paneli + özetler, bildirimler, Windows | |

**M1 dilimleri:** M1 tek seferde değil, kullanılabilir dilimlerle gelir.
1. **M1a (ilk kullanılabilir):** Varsayılan hesapla tek Claude oturumu. Akan metin, tool kartları, izin kartı (Allow/Deny), composer, Stop.
2. **M1b:** Hesaplar ve rozet, izin kuralları, subagent ağacı, kota ölçeri, sohbet içi arama, görsel/dosya yapıştırma, resume, eşzamanlı oturumlar.

## 19. Spike'lar ve riskler

| # | Konu | Ne zaman |
|---|---|---|
| S0 | gpui-kit 0.10 boş pencere: Linux ✅ (Wayland, RSS ≈ 93 MB). macOS doğrulanmadı | M0 başı |
| S1 | macOS'ta `CLAUDE_CONFIG_DIR` başına Keychain kaydı ayrışıyor mu? | M1 başı |
| S2 | `claude auth login` / `codex login` TTY'siz akışı | M1 başı |
| S3 | Ana ajan ve subagent için güvenilir context boyutu hesabı (Claude + Codex) | M1 |
| S4 | gpui-kit editörü: LSP tamamen kapalı mı, salt okunur mod, 50k satır performansı, tema eşlemesi, diff bileşeni yeterli mi? | M0 |
| S5 | Codex app-server "experimental": protokol değişimi, sürüm aralığı sabitleme | M4 |
| S6 | Claude stream-json/control protokolü resmi kararlı API değil → izolasyon + fixture testleri + sürüm kontrolü | M1 |
| S7 | Çalışan bir Claude oturumuna tur ortasında mesaj kuyruklama (sonuçta `queued_turn_count` alanı var) | M4 |
| S8 | Birden fazla aboneliği kişisel bir araçla aynı anda kullanmanın güncel kullanım şartlarına uygunluğu | M1 öncesi |

## 20. Veri konumları

- Yapılandırma: `directories` crate'iyle platform dizini (Linux `~/.config/kiln/`): `settings.json`, `keybindings.json`, `themes/`.
- Veri: oturum veritabanı, olay günlükleri, hesap dizinleri, worktree'ler.
- Proje: `.kiln/settings.json`, `.kiln/run.json` (commit'lenebilir), `.kiln/local.json` (gitignore).
