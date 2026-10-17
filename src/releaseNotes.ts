export interface ReleaseHighlight {
  tr: string[];
  en: string[];
}

export const RELEASE_HIGHLIGHTS: Record<string, ReleaseHighlight> = {
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
