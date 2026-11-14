export interface ReleaseHighlight {
  tr: string[];
  en: string[];
}

export const RELEASE_HIGHLIGHTS: Record<string, ReleaseHighlight> = {
  "0.3.9": {
    tr: [
      "Steam & Akamai CDN Uyumluluk Düzeltmesi: Steam'in web mağazası ve topluluk sayfalarında yaşanan gecikme, beyaz ekran ve eksik arayüz (CSS/görsel) sorunu giderildi; engelsiz servisler doğrudan saf kernel hızında iletilir.",
      "Kesin Hedefli Paket İzolasyonu (Targeted Desync): WinWS paket desync işlemi yalnızca kullanıcının seçtiği veya eklediği alan adlarına uygulanır; genel internet trafiğine, bankacılık ve oyun servislerine asla müdahale edilmez.",
      "Tek Tıkla Özel Domainleri Temizleme Butonu: Özel domain ekleme bölümüne eklenen tüm domainleri tek tuşla silebilen 'Tümünü Temizle' butonu ve onay mekanizması eklendi.",
      "Dinamik DoH ile DNS Zehirlemesi Koruması: Eklenen özel alan adları ve platformlar için gerçek IP'ler Cloudflare DoH ile anında çözülüp hosts seviyesinde İSS zehirlemesinden (BTK uyarısı) arındırılır.",
      "Canlı Süreç ve Oyun Algılayıcı (Watchdog): Arka planda çalışan oyun ve uygulamaları dinamik algılayıp arayüzde anında 'Canlı' rozetiyle gösteren nöbetçi döngü eklendi.",
      "Tam Sessiz Başlangıç (Stealth Startup): 'Başlangıçta Tepsiye Gizlen' aktifken açılışta ekranda hiçbir pencere veya bildirim gösterilmeden doğrudan tepsiye yerleşmesi sağlandı."
    ],
    en: [
      "Steam & Akamai CDN Compatibility Fix: Resolved loading stalls, blank white screens, and broken CSS/images on Steam web pages; unblocked services pass through at native line speed.",
      "Strict Targeted Packet Isolation: WinWS packet desync is strictly scoped to user-selected or custom-added domains; normal internet traffic, banking, and gaming services are never touched.",
      "One-Click Clear All Custom Domains: Added a 'Clear All' button with confirmation to delete all custom domains with a single click.",
      "Dynamic DoH DNS Poisoning Immunity: Custom domains are resolved via Cloudflare DoH and injected at the hosts level to defeat ISP DNS poisoning and ministry block warnings.",
      "Real-Time Process Watchdog: Added a background watchdog dynamically detecting running games/apps with live UI status badges.",
      "Zero-Popup Stealth Startup: Completely silent tray launch without on-screen windows or notifications when 'Start Minimized to Tray' is enabled."
    ]
  },
  "0.3.8": {
    tr: [
      "Güncelleme Sistemi Doğrulama ve Kararlılık: Canlı güncelleme akışı, indirme motoru ve NSIS kurulum süreçleri tamamen doğrulandı.",
      "Anlık Senkronizasyon İyileştirmesi: Uygulama açıkken gelen yeni sürüm bildirimleri ve arayüz tepkiselliği güçlendirildi."
    ],
    en: [
      "Update Engine Verification & Reliability: Validated the end-to-end update flow, download handling, and NSIS installation process.",
      "Real-time Notification Polish: Enhanced update banner responsiveness and live status handling."
    ]
  },
  "0.3.7": {
    tr: [
      "Güncelleme Motoru ve Otomatik İndirme Düzeltmesi: Windows sistem ve oyun servislerinin (GamingServices vb.) oyun olarak algılanıp güncellemeleri engellemesi sorunu giderildi.",
      "Güncelleme Bildirim Çubuğu Yenilendi: Kafa karıştıran 'OTOMATİK' yazısı yerine durum bazlı 'GÜNCELLENİYOR' ve doğrudan tıklanabilir 'Şimdi Güncelle' butonu getirildi.",
      "Anlık İlerleme Durumu: Güncelleme indirilirken ve kurulurken arayüzde canlı 'Güncelleniyor...' animasyonu ve durum senkronizasyonu eklendi.",
      "Kurulum Mutex Güvenliği: Güncelleme kurulurken Slipstream tekil örnek kilidi (mutex) otomatik bırakılarak kurulum kilitlenmeleri önlendi."
    ],
    en: [
      "Update Engine & Auto-Download Fix: Fixed Windows system services (e.g. GamingServices) falsely detected as running games which blocked updates.",
      "Revamped Update Banner: Replaced static 'AUTOMATIC' tag with stateful 'UPDATING' badge and interactive 'Update Now' button.",
      "Live Progress Synchronization: Real-time 'Updating...' pulse animations and status synchronization during downloads and installations.",
      "Installer Mutex Safety: App single-instance mutex is released before launching installer to prevent file locking."
    ]
  },
  "0.3.6": {
    tr: [
      "Başlangıçta Tepsiye Gizlenme İyileştirmesi: 'Başlangıçta Tepsiye Gizlen' seçeneği açıkken Windows açılışında pencerenin ekranda belirme sorunu kesin olarak giderildi.",
      "Tek Örnek (Single-Instance) Uyanma İzolasyonu: Windows başlangıcında eşzamanlı tetiklenen arka plan işlemlerinin ana pencereyi ekrana çağırması önlendi.",
      "Discord Otomatik İyileştirme & Watchdog: Askıda kalan veya takılı kalan Discord oturumlarını otomatik algılama ve arka planda onarma mekanizması eklendi.",
      "Sistem Tepsisi Doğrudan Discord Onarımı: Sistem tepsisi menüsüne 'Discord'u Onar ve Yeniden Başlat' seçeneği eklendi."
    ],
    en: [
      "Startup Tray Minimization Fix: Resolved issue where the window briefly appeared on screen during Windows boot when 'Start Minimized to Tray' was enabled.",
      "Single-Instance Wakeup Isolation: Prevented duplicate background autostart processes from triggering the wakeup event and opening the window.",
      "Discord Watchdog & Self-Healing: Added automatic detection and recovery for stalled Discord sessions (e.g. INVALID_SESSION loops).",
      "Direct Discord Repair Tray Option: Added 'Repair and Restart Discord' directly into the system tray context menu."
    ]
  },
  "0.3.5": {
    tr: [
      "Pencere Kontrolleri İzolasyonu: Güncelleme bildirimi veya pencereler açıkken çizgi (-) ve çarpı (X) tuşlarının kilitlenmesi kesin olarak önlendi.",
      "Güncelleme İndirme İyileştirmesi: 'Şimdi Güncelle' butonuna basıldığında indirme tetikleme güvenliği ve geri bildirim sistemi güçlendirildi.",
      "İşlem Kilidi Güvenliği: Arka plan güncellemesi başlatılmadan önce uygulama süreç kilidi (mutex) otomatik serbest bırakılarak kesintisiz kurulum sağlandı.",
      "Çift Katmanlı Kapatma Fallback'i: Uygulama kapatma ve gizleme mekanizması bağımsız sistem çağrılarıyla desteklendi."
    ],
    en: [
      "Window Control Isolation: Fixed minimize (-) and close (X) buttons getting unresponsive during update prompts.",
      "Reliable Update Triggering: Hardened 'Update Now' action with automatic fallback fetching and instant visual feedback.",
      "Mutex Release on Update: Single-instance mutex is released prior to running NSIS installer for 100% reliable execution.",
      "Dual Layer Close Fallback: App close and hide logic backed by direct window handles."
    ]
  },
  "0.3.4": {
    tr: [
      "Discord QR Kod ile Giriş Desteği: remote-auth-gateway.discord.gg ve latency.discord.media alan adları eklenerek QR kodun yüklenmeme ve bağlantı kopma sorunu giderildi.",
      "DNS Zehirleme Baypası: QR kod gateway adresi temiz Cloudflare Anycast IP havuzuna eklendi.",
      "Kararlılık ve Ses İyileştirmesi: Discord ses ve ağ gecikme sunucuları desync filtre kapsamına alındı."
    ],
    en: [
      "Discord QR Code Login Support: Added remote-auth-gateway.discord.gg and latency.discord.media to resolve stuck QR login.",
      "DNS Poisoning Bypass: Added clean Cloudflare Anycast IP mapping for remote auth gateway.",
      "Stability Improvements: Enhanced latency and audio telemetry bypass rules."
    ]
  },
  "0.3.3": {
    tr: [
      "Sessiz Arka Plan Güncellemesi: Oyun veya uygulamalar açıkken arka planda modal engeli olmadan kesintisiz güncelleme desteği.",
      "Gömülü Sürüm Senkronizasyonu: Motor ve arayüz sürüm tanımları tam uyumlu hale getirildi.",
      "Steam & Oyun Bant Genişliği Koruması: Oyun ve indirme trafiği DPI kapsamı dışında tutularak maksimum indirme hızı sağlandı.",
      "Hızlı Kapanma: Kapatma esnasında tepsi simgesi ve arayüz anında gizlenerek sistem kilitlenmeleri önlendi."
    ],
    en: [
      "Silent Background Update: Seamless background updates without modal blocking even while playing games.",
      "Embedded Version Sync: Unified engine and UI version declarations.",
      "Steam & Game Bandwidth Protection: Gaming and download CDN traffic remain completely unthrottled.",
      "Fast Exit: Window and tray icon vanish instantly upon shutdown to prevent hanging."
    ]
  },
  "0.3.2": {
    tr: [
      "Sistem Tepsisi (Tray) & Gizli Bölme Entegrasyonu: Görev çubuğu bildirim alanında simgenin görünmeme veya kaybolma sorunu giderildi; simge doğrudan işletim sistemi kaynaklarından kaydedilir.",
      "Gelişmiş Başlatma & Görünürlük: Uygulama kısayoldan açıldığında gizli kalma sorunu engellendi; ana pencere sağ altta açılır ve odağı alır. (Yalnızca Windows başlangıcında arka planda başlar).",
      "Tekil Örnek Uyanması (Wakeup): Uygulama arka plandayken kısayola tekrar tıklandığında mevcut oturum anında öne getirilir.",
      "Temiz Kapanma & Zombi Süreç Koruması: Uygulamadan çıkıldığında WinDivert filtre sürücüsü ve winws.exe alt süreçleri anında ve eksiksiz sonlandırılır.",
      "Anında Güncelleme Bildirimi: Yeni güncellemeler GitHub API üzerinden anında algılanır ve masaüstü bildirimleri ile gecikmesiz iletilir."
    ],
    en: [
      "System Tray Integration: Fixed system tray icon not appearing in taskbar notification overflow area; icon is loaded directly from OS binary resources.",
      "Improved Launch & Visibility: Resolved app starting invisible on manual launch; window appears directly at bottom-right. (Starts in tray only on Windows startup).",
      "Single-Instance Wakeup: Clicking shortcut while app is running in tray immediately restores and focuses the window.",
      "Clean Shutdown & Process Safety: Stopping or closing the app cleanly stops WinDivert driver and all winws.exe child processes immediately.",
      "Instant Update Detection: New releases are detected instantly via GitHub API with real-time notifications."
    ]
  },
  "0.3.1": {
    tr: [
      "Tepsiye Küçültme ve Başlık Çubuğu Tıklama İzolasyonu: Başlık çubuğu kontrolleri pencere sürükleme bölgesinden ayrıştırıldı; küçültme butonu pencereyi anında sistem tepsisine gizler.",
      "Gelişmiş Odak Kaybında Otomatik Gizlenme: Hem webview hem de işletim sistemi düzeyinde çift katmanlı odak dinleyicisi ile pencere dışına tıklandığında anında tepsiye gizlenme sağlandı.",
      "Doğrudan Win32 SW_HIDE Entegrasyonu: Pencerenin ekranda asılı kalması işletim sistemi çekirdeği düzeyinde kesin gizleme çağrısıyla engellendi.",
      "Sistem Tepsisi ve Tekil Örnek (Single-Instance) Güvenilirliği: Uygulama tepside çalışırken ikinci kez açıldığında sürecin sonlandırılması önlendi; mevcut pencere güvenle öne getirilir.",
      "Güncelleme Yükleyicisi Sağlamlaştırması: Otomatik güncelleme sırasında süreç kilitlemeleri ve donmalar giderildi."
    ],
    en: [
      "Minimize to Tray & Titlebar Click Isolation: Titlebar controls isolated from window drag regions with WebKit app-region; minimize button reliably hides to system tray.",
      "Robust Auto-Hide on Focus Loss: Dual-layer blur detection (WebView DOM blur & native OS window focus) guarantees instant hiding when clicking outside.",
      "Direct Win32 SW_HIDE Integration: Eliminated hanging window artifacts by enforcing immediate OS-level window concealment.",
      "System Tray & Single-Instance Reliability: Fixed issue where launching a second instance terminated the background tray process; existing instance is safely focused.",
      "Hardened Update Installer: Resolved process locking and freeze conditions during automated updates."
    ]
  },
  "0.3.0": {
    tr: [
      "Canlı Sinyal (SSE Push) Anında Güncelleme Mimarisi: Yeni sürüm çıktığı salise arka planda bekleyen tüm uygulamalara gecikmesiz sinyal gönderilir.",
      "Açılışta Öncelikli Güncelleme: Otomatik güncelleme açıkken uygulama başlatıldığı an yeni sürüm kontrol edilip doğrudan indirilir ve uygulanır.",
      "Koşulsuz Bildirim Garantisi: Otomatik güncelleme açık veya kapalı fark etmeksizin hem açılışta hem de çalışırken bildirim anında masaüstüne fırlar.",
      "Manuel Mod Kontrolü: Otomatik güncelleme kapalıyken anında gelen bildirim veya arayüzdeki 'Şimdi Güncelle' butonuyla tek tıkla güncelleme imkanı.",
      "Arka Planda Sıfır İşlemci Tüketimi: Pasif soket mimarisi sayesinde %0 CPU ve sıfır pil tüketimiyle canlı sinyal dinleme."
    ],
    en: [
      "Live SSE Push Instant Update Architecture: Millisecond update signals dispatched directly to background applications when a release goes live.",
      "Priority Startup Auto-Update: When auto-update is enabled, checks immediately on boot and applies updates seamlessly.",
      "Unconditional Notification Guarantee: Instant desktop tray and UI notifications pop up whether auto-update is enabled or disabled.",
      "Instant Manual Control: One-click update via instant notification or 'Update Now' button when auto-update is off.",
      "Zero CPU Background Footprint: Passive socket listener maintains zero CPU and zero battery drain while waiting for push events."
    ]
  },
  "0.2.9": {
    tr: [
      "Otomatik Strateji Tarama (Autotune) arayüz kayma ve bug sorunu giderildi: Test logları akarken başlık çubuğunun ekran dışına taşması tamamen önlendi.",
      "Anında Güncelleme Bildirimi: Otomatik güncelleme açık veya kapalı fark etmeksizin yeni sürüm çıktığında sistem tepsisi bildirimi anında gösterilir.",
      "Sıfır Arka Plan Kaynak Tüketimi: Sistem tepsisinde gizliyken gereksiz GitHub yoklama döngüleri engellenerek %0 CPU ve sıfır ağ yükü sağlandı.",
      "Pencere koordinat ve kaydırma sınırları donanım ve ekran ölçeklerine karşı sağlamlaştırıldı."
    ],
    en: [
      "Fixed autotune UI shifting bug: Prevented titlebar and top controls from moving off-screen during strategy scans.",
      "Instant Update Notifications: Windows system tray notification is triggered immediately regardless of whether auto-updates is toggled on or off.",
      "Zero Background Resource Consumption: Eliminated polling loops while minimized to tray for 0% CPU and zero network overhead.",
      "Window coordinate and layout clamping hardened across display scaling levels."
    ]
  },
  "0.2.8": {
    tr: [
      "Orijinal kararlı winws.exe motor mimarisi ve WinDivert kütüphanesi tam entegre edildi.",
      "Windows Çekirdek İş Nesnesi (Job Object) ile yetim süreç ve sürücü sızıntıları önlendi.",
      "Tek dosya NSIS kurulum standardı güncellendi."
    ],
    en: [
      "Restored original stable winws.exe engine architecture and WinDivert library.",
      "Windows Kernel Job Object integration to prevent orphaned driver or engine leaks.",
      "Single NSIS Setup installation standard updated."
    ]
  },
  "0.2.7": {
    tr: [
      "Windows Çekirdek İş Nesnesi (Windows Kernel Job Object - KILL_ON_JOB_CLOSE) tam entegrasyonu: winws.exe paket motoru işletim sistemi çekirdeği düzeyinde Slipstream'e bağlandı.",
      "Slipstream kapandığında, çöktüğünde veya Görev Yöneticisinden sonlandırıldığında motor süreçleri anında Windows çekirdeği tarafından otomatik kapatılır.",
      "Açılışta ve kapanışta yetim winws süreçleri ve WinDivert sürücüleri eksiksiz temizleniyor.",
      "Güncelleme açıklamaları ve sürüm yenilikleri penceresi (Release Highlights) çevrimdışı ve güncel sürüm dahil her zaman eksiksiz görüntülenecek şekilde düzeltildi.",
      "Masaüstü kısayolları ve Windows başlangıç kayıt defteri çakışmaları giderildi."
    ],
    en: [
      "Windows Kernel Job Object (KILL_ON_JOB_CLOSE) full integration: winws.exe engine is tied to Slipstream at the OS kernel level.",
      "When Slipstream exits, restarts, crashes, or is killed via Task Manager, engine processes are instantly terminated by Windows kernel.",
      "Orphaned winws processes and WinDivert drivers are automatically swept on startup and shutdown.",
      "Release Highlights modal fixed to reliably display notes even when offline or already up to date.",
      "Desktop shortcuts and Windows startup registry conflicts resolved."
    ]
  },
  "0.2.6": {
    tr: [
      "Windows Çekirdek İş Nesnesi (Windows Kernel Job Object) entegrasyonu sağlandı.",
      "Slipstream kapatıldığında winws.exe paket motorunun arka planda açık kalması engellendi.",
      "Arka plan süreçleri ve WinDivert sürücüsü için süpürücü (sweeper) eklendi.",
      "Kurulum ve kaldırma işlemleri sırasında çalışan motor süreçleri güvenle sonlandırılıyor."
    ],
    en: [
      "Windows Kernel Job Object integration implemented.",
      "Prevented winws.exe packet engine from lingering in the background after Slipstream closes.",
      "Added sweeper for background processes and WinDivert driver.",
      "Installer and uninstaller now safely shut down active engine processes."
    ]
  },
  "0.2.5": {
    tr: [
      "Kullanıcı oyun oynarken arka plan güncellemeleri bekletilir.",
      "Otomatik güncelleme açıkken arka planda sessiz indirme ve kurma desteği.",
      "Uygulama açıldığında güncellenen sürüm bildirimi ve yenilikler paneli."
    ],
    en: [
      "Background updates are paused while games are running.",
      "Silent download and installation in background when auto-update is enabled.",
      "Update success notification and highlights shown on application launch."
    ]
  },
  "0.2.4": {
    tr: [
      "Gelişmiş paket filtreleri ve ağ performans optimizasyonları.",
      "Windows başlangıcında otomatik başlatma geliştirmeleri.",
      "Genişletilmiş çoklu dil desteği."
    ],
    en: [
      "Enhanced packet filters and network performance optimizations.",
      "Autostart on boot improvements.",
      "Extended multi-language support."
    ]
  }
};

export function getReleaseHighlights(version: string, lang: string): string[] {
  const normVer = version.replace(/^v/, "").trim();
  const entry = RELEASE_HIGHLIGHTS[normVer];
  if (entry) {
    return lang === "tr" ? entry.tr : entry.en;
  }
  return lang === "tr"
    ? [
        "Hata düzeltmeleri ve kararlılık geliştirmeleri yapıldı.",
        "Performans optimizasyonları ve çekirdek bağlantı güncellemeleri uygulandı."
      ]
    : [
        "Bug fixes and stability enhancements.",
        "Performance optimizations and core connection updates."
      ];
}
