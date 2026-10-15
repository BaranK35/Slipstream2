import { useEffect, useState, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./styles.css";
import logoSvg from "./assets/logo.svg";

type CustomPlatform = {
  id: string;
  name: string;
  category: "app" | "game";
  icon: string;
  domains: string[];
};

type Cfg = {
  isp: string;
  strategy: string | null;
  enabled_apps: string[];
  custom_platforms: CustomPlatform[];
  custom_domains: string[];
  custom_exes: string[];
  manage_dns?: boolean;

  // System & Startup
  auto_start_pc: boolean;
  start_minimized: boolean;
  auto_connect: boolean;
  minimize_to_tray: boolean;
  hide_on_blur: boolean;

  // Advanced Network & Gaming
  doh_provider: string;
  gaming_mode: boolean;
  auto_healing: boolean;
  auto_update?: boolean;
};

type AppProfile = {
  id: string;
  name: string;
  category: "app" | "game";
  tag: string;
  icon: string;
  domains: string[];
  voice: boolean;
  probe: string;
};

type Catalog = {
  isps: { id: string; name: string; order?: string[] }[];
  apps: AppProfile[];
  strategies: { id: string; name: string }[];
};

type DohBenchmarkResult = {
  provider: string;
  name: string;
  latency: number | null;
};

type DiscordDiagnosis = {
  installed: boolean;
  patched: boolean;
  running: boolean;
  relay_ok: boolean;
};

type NetworkHealth = {
  proxy_clean: boolean;
  doh_ok: boolean;
  discord_relay_ok: boolean;
  windivert_driver_ok: boolean;
  active_doh_provider: string;
};

function playTone(type: "on" | "off") {
  try {
    const AudioContextClass = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    if (!AudioContextClass) return;
    const ctx = new AudioContextClass();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.connect(gain);
    gain.connect(ctx.destination);
    const now = ctx.currentTime;
    gain.gain.setValueAtTime(0.03, now);

    if (type === "on") {
      osc.type = "sine";
      osc.frequency.setValueAtTime(340, now);
      osc.frequency.exponentialRampToValueAtTime(680, now + 0.12);
      osc.frequency.exponentialRampToValueAtTime(1020, now + 0.22);
      gain.gain.exponentialRampToValueAtTime(0.001, now + 0.35);
      osc.start(now);
      osc.stop(now + 0.35);
    } else {
      osc.type = "sine";
      osc.frequency.setValueAtTime(750, now);
      osc.frequency.exponentialRampToValueAtTime(400, now + 0.18);
      gain.gain.exponentialRampToValueAtTime(0.001, now + 0.3);
      osc.start(now);
      osc.stop(now + 0.3);
    }
  } catch {
    // AudioContext fallback
  }
}

type LatencyStats = {
  discord: number | null;
  cloudflare: number | null;
  google: number | null;
};

export default function App() {
  const [cat, setCat] = useState<Catalog | null>(null);
  const [cfg, setCfg] = useState<Cfg | null>(null);
  const [on, setOn] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [showLog, setShowLog] = useState(false);
  const [activeTab, setActiveTab] = useState<"services" | "settings" | "rules" | "preferences">("services");
  const [platformFilter, setPlatformFilter] = useState<"all" | "apps" | "games">("all");
  const [searchQuery, setSearchQuery] = useState("");

  // Appearance & Audio
  const [theme, setTheme] = useState<"cyber" | "amoled" | "emerald">(() => {
    try {
      return (localStorage.getItem("slipstream_theme") as "cyber" | "amoled" | "emerald") || "cyber";
    } catch {
      return "cyber";
    }
  });
  const [soundEnabled, setSoundEnabled] = useState<boolean>(() => {
    try {
      return localStorage.getItem("slipstream_sound") !== "false";
    } catch {
      return true;
    }
  });

  // Diagnostics & Benchmarks
  const [benchmarkingDoh, setBenchmarkingDoh] = useState(false);
  const [dohBenchmarks, setDohBenchmarks] = useState<DohBenchmarkResult[]>([]);
  const [discordDiag, setDiscordDiag] = useState<DiscordDiagnosis | null>(null);
  const [repairingDiscord, setRepairingDiscord] = useState(false);
  const [showHealthModal, setShowHealthModal] = useState(false);
  const [networkHealth, setNetworkHealth] = useState<NetworkHealth | null>(null);
  const [checkingHealth, setCheckingHealth] = useState(false);


  // Live Latency Radar
  const [latency, setLatency] = useState<LatencyStats>({
    discord: null,
    cloudflare: null,
    google: null,
  });

  // Custom domain in Tab 3
  const [domainDraft, setDomainDraft] = useState("");

  // Add Platform Modal state
  const [showAddModal, setShowAddModal] = useState(false);
  const [addName, setAddName] = useState("");
  const [addCategory, setAddCategory] = useState<"app" | "game">("app");
  const [addUrl, setAddUrl] = useState("");
  const [expandedPreview, setExpandedPreview] = useState<string[]>([]);

  const logEndRef = useRef<HTMLDivElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const prevNetworkKeyRef = useRef<string | null>(null);

  // Update and EULA state
  const [updateInfo, setUpdateInfo] = useState<{
    has_update: boolean;
    current_version: string;
    latest_version: string;
    release_notes: string;
    download_url: string;
  } | null>(null);
  const [updating, setUpdating] = useState(false);
  const updatingRef = useRef(false);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [showEulaModal, setShowEulaModal] = useState(false);

  // Theme synchronization
  useEffect(() => {
    try {
      document.documentElement.dataset.theme = theme;
      localStorage.setItem("slipstream_theme", theme);
    } catch {}
  }, [theme]);

  useEffect(() => {
    try {
      localStorage.setItem("slipstream_sound", String(soundEnabled));
    } catch {}
  }, [soundEnabled]);

  // Auto-dismiss alerts
  useEffect(() => {
    if (err) {
      const timer = setTimeout(() => setErr(null), 5000);
      return () => clearTimeout(timer);
    }
  }, [err]);

  useEffect(() => {
    if (successMsg) {
      const timer = setTimeout(() => setSuccessMsg(null), 3500);
      return () => clearTimeout(timer);
    }
  }, [successMsg]);

  // Reset file dialog lock on window focus
  useEffect(() => {
    const handleFocus = () => {
      invoke("set_file_dialog_active", { active: false }).catch(() => {});
    };
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, []);

  // Initial Load
  useEffect(() => {
    (async () => {
      try {
        const [c, loadedCfg, engineState] = await Promise.all([
          invoke<Catalog>("catalog"),
          invoke<Cfg>("load_config"),
          invoke<boolean>("is_engine_on"),
        ]);
        setCat(c);
        const initialCfg: Cfg = {
          ...loadedCfg,
          custom_platforms: loadedCfg.custom_platforms || [],
          auto_start_pc: loadedCfg.auto_start_pc ?? true,
          start_minimized: loadedCfg.start_minimized ?? true,
          auto_connect: loadedCfg.auto_connect ?? true,
          minimize_to_tray: loadedCfg.minimize_to_tray ?? true,
          hide_on_blur: loadedCfg.hide_on_blur ?? true,
          doh_provider: loadedCfg.doh_provider || "cloudflare",
          gaming_mode: loadedCfg.gaming_mode ?? false,
          auto_healing: loadedCfg.auto_healing ?? true,
          auto_update: loadedCfg.auto_update ?? true,
        };
        setCfg(initialCfg);
        setOn(engineState);

        // Check for updates on boot immediately
        invoke<{
          has_update: boolean;
          current_version: string;
          latest_version: string;
          release_notes: string;
          download_url: string;
        }>("check_for_updates")
          .then((info) => {
            setUpdateInfo(info);
            if (info.has_update && initialCfg.auto_update && info.download_url) {
              handleApplyUpdate(info.download_url);
            }
          })
          .catch(() => {});
      } catch (e) {
        setErr(String(e));
      }
    })();

    const unlistenAutotune = listen<string>("autotune", (e) => {
      setLog((l) => [...l.slice(-40), e.payload]);
    });

    const unlistenWatchdog = listen<string>("watchdog_recovery", (e) => {
      setSuccessMsg(`🩺 ${e.payload}`);
    });

    const unlistenState = listen<boolean>("engine_state_changed", (e) => {
      setOn(e.payload);
      if (soundEnabled) playTone(e.payload ? "on" : "off");
    });

    const unlistenDns = listen("dns_flushed", () => {
      setSuccessMsg("DNS önbelleği başarıyla temizlendi.");
    });

    // Check initial Discord diagnosis
    invoke<DiscordDiagnosis>("diagnose_discord").then(setDiscordDiag).catch(() => {});

    // Check for updates periodically in the background every 60 seconds (1 minute)
    const updateTimer = setInterval(() => {
      invoke<{
        has_update: boolean;
        current_version: string;
        latest_version: string;
        release_notes: string;
        download_url: string;
      }>("check_for_updates")
        .then((info) => {
          setUpdateInfo(info);
          if (info.has_update) {
            invoke<Cfg>("get_config").then((currentCfg) => {
              if (currentCfg.auto_update && info.download_url) {
                handleApplyUpdate(info.download_url);
              }
            });
          }
        })
        .catch(() => {});
    }, 60 * 1000);

    return () => {
      clearInterval(updateTimer);
      unlistenAutotune.then((f) => f());
      unlistenWatchdog.then((f) => f());
      unlistenState.then((f) => f());
      unlistenDns.then((f) => f());
    };
  }, []);

  // Periodic Latency Ping Radar
  useEffect(() => {
    let active = true;
    const fetchLatency = async () => {
      try {
        const res = await invoke<LatencyStats>("check_latency");
        if (active) {
          setLatency(res);
        }
      } catch {
        // silent ping failure
      }
    };

    fetchLatency();
    const interval = setInterval(fetchLatency, 6000);
    return () => {
      active = false;
      clearInterval(interval);
    };
  }, [on]);

  useEffect(() => {
    if (showLog && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [log, showLog]);

  // Auto-save configuration on change
  useEffect(() => {
    if (!cfg) return;
    invoke("save_config", { cfg });
    if (!on) {
      prevNetworkKeyRef.current = null;
      return;
    }
    const currentNetworkKey = JSON.stringify([
      cfg.isp,
      cfg.strategy,
      cfg.enabled_apps,
      cfg.custom_platforms,
      cfg.custom_domains,
      cfg.custom_exes,
      cfg.manage_dns,
      cfg.doh_provider,
      cfg.gaming_mode,
    ]);
    if (prevNetworkKeyRef.current === null) {
      prevNetworkKeyRef.current = currentNetworkKey;
      return;
    }
    if (prevNetworkKeyRef.current === currentNetworkKey) {
      return;
    }
    prevNetworkKeyRef.current = currentNetworkKey;
    const t = setTimeout(() => {
      invoke("start", { cfg }).catch((e) => setErr(String(e)));
    }, 400);
    return () => clearTimeout(t);
  }, [cfg, on]);

  // When adding URL, dynamically expand domain
  useEffect(() => {
    if (!addUrl.trim()) {
      setExpandedPreview([]);
      return;
    }
    const t = setTimeout(async () => {
      try {
        const res = await invoke<string[]>("expand_url", { raw: addUrl.trim() });
        setExpandedPreview(res);
      } catch {
        setExpandedPreview([]);
      }
    }, 200);
    return () => clearTimeout(t);
  }, [addUrl]);

  // Combine built-in apps and custom user platforms
  const allPlatforms = useMemo(() => {
    if (!cat || !cfg) return [];
    return [
      ...cat.apps.map((a) => ({
        id: a.id,
        name: a.name,
        category: a.category,
        tag: a.tag,
        icon: a.icon,
        domains: a.domains,
        probe: a.probe,
        isCustom: false,
      })),
      ...(cfg.custom_platforms || []).map((cp) => ({
        id: cp.id,
        name: cp.name,
        category: cp.category,
        tag: `${cp.domains.length} alan adı & web sitesi`,
        icon: cp.icon || (cp.category === "game" ? "🎮" : "🌐"),
        domains: cp.domains,
        probe: cp.domains[0] ? `https://${cp.domains[0]}` : "https://google.com",
        isCustom: true,
      })),
    ];
  }, [cat, cfg]);

  const filteredPlatforms = useMemo(() => {
    return allPlatforms.filter((p) => {
      if (platformFilter === "apps" && p.category !== "app") return false;
      if (platformFilter === "games" && p.category !== "game") return false;
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        return p.name.toLowerCase().includes(q) || p.tag.toLowerCase().includes(q);
      }
      return true;
    });
  }, [allPlatforms, platformFilter, searchQuery]);

  if (!cat || !cfg) {
    return (
      <div className="window-frame">
        <div className="titlebar" data-tauri-drag-region>
          <div className="brand" data-tauri-drag-region>
            <img src={logoSvg} alt="Slipstream" className="brand-logo" />
            <div className="status-indicator busy" />
            <span className="brand-name" data-tauri-drag-region>Slipstream</span>
            <span className="version-pill">v{updateInfo?.current_version || "0.1.1"}</span>
          </div>
          <div className="window-controls">
            <button className="win-btn minimize" title="Tepsiye Küçült" onClick={() => invoke("hide_window")}>
              <svg viewBox="0 0 10 10">
                <path d="M1 5h8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
              </svg>
            </button>
          </div>
        </div>
        <div className="boot-container">
          <div className="spinner" />
          <p>Slipstream Başlatılıyor...</p>
          {err && (
            <p style={{ color: "#ff5252", fontSize: 11, padding: "0 16px", textAlign: "center" }}>
              {err}
            </p>
          )}
        </div>
      </div>
    );
  }

  const patch = (p: Partial<Cfg>) => {
    setCfg((prev) => (prev ? { ...prev, ...p } : null));
  };

  const toggleApp = (id: string) => {
    const list = cfg.enabled_apps.includes(id)
      ? cfg.enabled_apps.filter((x) => x !== id)
      : [...cfg.enabled_apps, id];
    patch({ enabled_apps: list });
  };

  const toggleSelectAll = (select: boolean) => {
    if (select) {
      const allIds = [
        ...cat.apps.map((a) => a.id),
        ...cfg.custom_platforms.map((cp) => cp.id),
      ];
      patch({ enabled_apps: Array.from(new Set(allIds)) });
    } else {
      patch({ enabled_apps: [] });
    }
  };

  const selectPopular = () => {
    patch({ enabled_apps: ["discord", "youtube", "roblox"] });
    setSuccessMsg("Popüler platformlar (Discord, YouTube, Roblox) seçildi.");
  };

  const applyPreset = (preset: "gamer" | "social" | "media" | "all") => {
    if (!cfg || !cat) return;
    let selected: string[] = [];
    if (preset === "gamer") {
      selected = ["roblox", "steam", "epicgames", "riotgames", "minecraft", "rockstar_blizzard_ea", "discord"];
      patch({ enabled_apps: selected, gaming_mode: true });
      setSuccessMsg("🎮 Oyun Paketi aktif edildi (Roblox, Riot, Steam, Epic ve Düşük Gecikme Modu).");
    } else if (preset === "social") {
      selected = ["discord", "x", "meta", "telegram", "reddit", "tiktok"];
      patch({ enabled_apps: selected });
      setSuccessMsg("💬 Sosyal Medya & İletişim Paketi seçildi.");
    } else if (preset === "media") {
      selected = ["youtube", "twitch", "kick", "spotify"];
      patch({ enabled_apps: selected });
      setSuccessMsg("📺 Yayın & Medya Paketi seçildi.");
    } else if (preset === "all") {
      const allIds = [
        ...cat.apps.map((a) => a.id),
        ...cfg.custom_platforms.map((cp) => cp.id),
      ];
      patch({ enabled_apps: Array.from(new Set(allIds)) });
      setSuccessMsg("🌍 Tüm platformlar seçildi.");
    }
  };

  async function handleToggleAutostart() {
    if (!cfg) return;
    const newVal = !cfg.auto_start_pc;
    try {
      await invoke("set_autostart", { enable: newVal });
      patch({ auto_start_pc: newVal });
      setSuccessMsg(
        newVal
          ? "Slipstream Windows başlangıcına eklendi (Bilgisayar açılınca otomatik çalışacak)."
          : "Slipstream Windows başlangıcından kaldırıldı."
      );
    } catch (e) {
      setErr("Başlangıç ayarı değiştirilemedi: " + String(e));
    }
  }

  async function handleBenchDoh() {
    setBenchmarkingDoh(true);
    setErr(null);
    try {
      const results = await invoke<DohBenchmarkResult[]>("bench_doh_providers");
      setDohBenchmarks(results);
      let fastest: DohBenchmarkResult | null = null;
      for (const r of results) {
        if (r.latency !== null) {
          if (!fastest || (fastest.latency !== null && r.latency < fastest.latency)) {
            fastest = r;
          }
        }
      }
      if (fastest) {
        patch({ doh_provider: fastest.provider });
        setSuccessMsg(`⚡ En hızlı DNS seçildi: ${fastest.name} (${fastest.latency}ms)!`);
      }
    } catch (e) {
      setErr("DNS benchmark hatası: " + String(e));
    }
    setBenchmarkingDoh(false);
  }

  async function handleRepairDiscord() {
    setRepairingDiscord(true);
    setErr(null);
    try {
      await invoke("repair_discord");
      setSuccessMsg("Discord ayarları onarıldı, DNS temizlendi ve temiz başlatıldı!");
      const diag = await invoke<DiscordDiagnosis>("diagnose_discord");
      setDiscordDiag(diag);
    } catch (e) {
      setErr("Discord onarılamadı: " + String(e));
    }
    setRepairingDiscord(false);
  }

  async function handleRunHealthCheck() {
    setCheckingHealth(true);
    setShowHealthModal(true);
    try {
      const res = await invoke<NetworkHealth>("check_network_health", {
        provider: cfg?.doh_provider || "cloudflare",
      });
      setNetworkHealth(res);
    } catch (e) {
      setErr("Sağlık teşhisi yapılamadı: " + String(e));
    }
    setCheckingHealth(false);
  }

  async function handlePower() {
    setBusy(true);
    setErr(null);
    setSuccessMsg(null);
    try {
      if (on) {
        await invoke("stop");
        setOn(false);
        if (soundEnabled) playTone("off");
      } else {
        await invoke("start", { cfg });
        setOn(true);
        if (soundEnabled) playTone("on");
      }
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  }

  async function handleAutoTune() {
    setBusy(true);
    setErr(null);
    setSuccessMsg(null);
    setLog([]);
    setShowLog(true);
    try {
      const best = await invoke<string | null>("autotune", { cfg });
      if (best) {
        patch({ strategy: best });
        setOn(true);
        setSuccessMsg(`En iyi strateji bulundu ve aktif edildi: ${best}`);
      } else {
        setErr("Hiçbir strateji hedefe ulaşamadı. Log detaylarını inceleyebilirsiniz.");
      }
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  }

  function handleSaveCustomPlatform() {
    const name = addName.trim();
    if (!name || expandedPreview.length === 0) {
      setErr("Lütfen bir isim ve geçerli bir web sitesi / alan adı girin.");
      return;
    }

    const id = "custom_" + Date.now().toString(36);
    const icon = addCategory === "game" ? "🎮" : "🌐";
    const newPlatform: CustomPlatform = {
      id,
      name,
      category: addCategory,
      icon,
      domains: expandedPreview,
    };

    patch({
      custom_platforms: [...(cfg?.custom_platforms || []), newPlatform],
      enabled_apps: [...(cfg?.enabled_apps || []), id],
    });

    setAddName("");
    setAddUrl("");
    setExpandedPreview([]);
    setShowAddModal(false);
    setSuccessMsg(`"${name}" eklendi ve tüm bağlantıları aktif edildi!`);
  }

  async function handleRestartDiscord() {
    try {
      await invoke("restart_discord");
      setSuccessMsg("Discord relay üzerinden temiz şekilde başlatılıyor...");
    } catch (e) {
      setErr("Discord başlatılamadı: " + String(e));
    }
  }



  async function handleFlushDns() {
    try {
      await invoke("flush_dns_cache");
      setSuccessMsg("DNS önbelleği başarıyla temizlendi (FlushDNS tamamlandı).");
    } catch (e) {
      setErr("DNS temizlenemedi: " + String(e));
    }
  }

  async function handleResetAdapterDns() {
    try {
      await invoke("reset_dns");
      setSuccessMsg("Ağ kartlarının DNS ayarları varsayılana (otomatik DHCP) sıfırlandı!");
    } catch (e) {
      setErr("DNS sıfırlanamadı: " + String(e));
    }
  }

  async function handleCheckUpdates(manual: boolean = false) {
    if (manual) setCheckingUpdates(true);
    try {
      const info = await invoke<{
        has_update: boolean;
        current_version: string;
        latest_version: string;
        release_notes: string;
        download_url: string;
      }>("check_for_updates");
      setUpdateInfo(info);
      if (info.has_update) {
        if (cfg?.auto_update && info.download_url) {
          handleApplyUpdate(info.download_url);
        } else if (manual) {
          setSuccessMsg(`🚀 Yeni sürüm bulundu: v${info.latest_version}`);
        }
      } else if (manual) {
        setSuccessMsg(`✅ En son sürümü (v${info.current_version}) kullanıyorsunuz. (GitHub ile senkronize)`);
      }
    } catch {
      if (manual) setErr("Güncelleme sunucusuna (GitHub) erişilemedi.");
    }
    if (manual) setCheckingUpdates(false);
  }

  async function handleApplyUpdate(downloadUrl?: string) {
    if (updatingRef.current) return;
    const url = downloadUrl || updateInfo?.download_url;
    if (!url) return;
    updatingRef.current = true;
    setUpdating(true);
    setSuccessMsg("Güncelleme indiriliyor ve kuruluyor, uygulama yeniden başlatılacak...");
    try {
      await invoke("apply_update", { downloadUrl: url });
    } catch (e) {
      setErr("Güncelleme yüklenemedi: " + String(e));
      setUpdating(false);
      updatingRef.current = false;
    }
  }

  // Export config to JSON file
  function handleExportConfig() {
    if (!cfg) return;
    const jsonStr = "data:text/json;charset=utf-8," + encodeURIComponent(JSON.stringify(cfg, null, 2));
    const dlAnchor = document.createElement("a");
    dlAnchor.setAttribute("href", jsonStr);
    dlAnchor.setAttribute("download", `slipstream_config_${Date.now()}.json`);
    document.body.appendChild(dlAnchor);
    dlAnchor.click();
    dlAnchor.remove();
    setSuccessMsg("Ayarlarınız JSON dosyası olarak dışa aktarıldı.");
  }

  // Import config from JSON file
  function handleImportConfig(e: React.ChangeEvent<HTMLInputElement>) {
    invoke("set_file_dialog_active", { active: false }).catch(() => {});
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (event) => {
      try {
        const imported = JSON.parse(event.target?.result as string);
        if (imported && typeof imported === "object") {
          patch({
            ...imported,
            custom_platforms: imported.custom_platforms || [],
            custom_domains: imported.custom_domains || [],
            hide_on_blur: imported.hide_on_blur ?? true,
          });
          setSuccessMsg("Ayarlar başarıyla içe aktarıldı.");
        }
      } catch {
        setErr("Geçersiz JSON yapılandırma dosyası.");
      }
    };
    reader.readAsText(file);
    if (fileInputRef.current) fileInputRef.current.value = "";
  }

  function removeCustomPlatform(id: string) {
    if (!cfg) return;
    patch({
      custom_platforms: cfg.custom_platforms.filter((p) => p.id !== id),
      enabled_apps: cfg.enabled_apps.filter((x) => x !== id),
    });
  }

  function addRule() {
    const v = domainDraft.trim();
    if (!v || !cfg) return;
    patch({
      custom_domains: Array.from(new Set([...cfg.custom_domains, v])),
    });
    setDomainDraft("");
  }

  function removeRule(domain: string) {
    if (!cfg) return;
    patch({
      custom_domains: cfg.custom_domains.filter((x) => x !== domain),
    });
  }


  const appCount = allPlatforms.filter((p) => p.category === "app").length;
  const gameCount = allPlatforms.filter((p) => p.category === "game").length;

  const currentStrategy =
    cat.strategies.find((s) => s.id === cfg.strategy)?.name ||
    cat.strategies.find((s) => s.id === "fake-split-md5sig")?.name ||
    "Otomatik (Fake + Split2 md5sig)";

  return (
    <div className="window-frame">
      {/* Title Bar for System Tray Flyout */}
      <div className="titlebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <img src={logoSvg} alt="Slipstream" className="brand-logo" />
          <div className={`status-indicator ${busy ? "busy" : on ? "active" : "inactive"}`} />
          <span className="brand-name" data-tauri-drag-region>Slipstream</span>
          <span className="version-pill">v{updateInfo?.current_version || "0.1.1"}</span>
        </div>
        <div className="window-controls">
          <button
            className="win-btn minimize"
            title="Tepsiye Küçült"
            onClick={() => invoke("hide_window")}
          >
            <svg viewBox="0 0 10 10">
              <path d="M1 5h8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
            </svg>
          </button>
        </div>
      </div>

      <div className="content-scroll">
        {/* Update Notification Banner */}
        {updateInfo?.has_update && (
          <div className="update-banner">
            <div className="update-content">
              <span className="update-badge">
                {cfg?.auto_update ? "⚡ OTOMATİK GÜNCELLEME" : "YENİ SÜRÜM MEVCUT"}
              </span>
              <p className="update-text">
                <b>v{updateInfo.latest_version}</b> hazır!{" "}
                {cfg?.auto_update
                  ? (updating
                      ? "Arka planda indiriliyor ve kuruluyor, uygulama yeniden başlatılacak..."
                      : "Arka planda otomatik kuruluyor...")
                  : (updateInfo.release_notes || "Gelişmiş stabilite ve yeni optimizasyonlar.")}
              </p>
            </div>
            {!cfg?.auto_update ? (
              <button
                className="update-action-btn"
                onClick={() => handleApplyUpdate()}
                disabled={updating}
              >
                {updating ? "İndiriliyor..." : "Şimdi Güncelle"}
              </button>
            ) : (
              <span className="update-auto-tag">
                {updating ? "⏳ İndiriliyor..." : "⚡ Otomatik"}
              </span>
            )}
          </div>
        )}

        {/* Hero Power Section */}
        <section className="hero-card">
          <div className="power-wrapper">
            <button
              className={`power-button ${busy ? "busy" : on ? "on" : "off"}`}
              onClick={handlePower}
              disabled={busy}
              aria-label={on ? "Bypass'ı Kapat" : "Bypass'ı Aç"}
            >
              <svg viewBox="0 0 100 100" className="power-svg">
                <circle cx="50" cy="50" r="42" className="power-ring" />
                <path d="M50 24v24M34 35a22 22 0 1 0 32 0" className="power-glyph" />
              </svg>
            </button>
          </div>

          <div className="hero-status">
            <div className={`state-badge ${busy ? "busy" : on ? "on" : "off"}`}>
              <span className={`status-dot ${busy ? "busy" : on ? "on" : "off"}`} />
              {busy ? "Ayarlanıyor..." : on ? "BYPASS AKTİF" : "KORUMA KAPALI"}
            </div>
            <p className="hero-subtitle">
              {on
                ? "Sıfır Gecikme • Doğrudan UDP Ses & Kernel Desync"
                : "Seçili servisler için tek tıkla korumayı başlatın."}
            </p>
          </div>

          {/* Real-time Ping Radar Badge */}
          <div className="latency-radar-bar">
            <div className="latency-badge" title="Discord bağlantı gecikmesi">
              <span className="latency-dot discord" />
              <span className="latency-label">Discord:</span>
              <span className="latency-val">
                {latency.discord !== null ? `${latency.discord}ms` : "—"}
              </span>
            </div>
            <div className="latency-badge" title="Cloudflare DoH gecikmesi">
              <span className="latency-dot cloudflare" />
              <span className="latency-label">DoH DNS:</span>
              <span className="latency-val">
                {latency.cloudflare !== null ? `${latency.cloudflare}ms` : "—"}
              </span>
            </div>
            <div className="latency-badge zero-overhead" title="Yerel Proxy Yok - Doğrudan Hat Erişimi">
              <span className="latency-dot zero" />
              <span className="latency-label">Ek Gecikme:</span>
              <span className="latency-val zero">0 ms</span>
            </div>
          </div>
        </section>

        {/* Navigation Tabs */}
        <div className="tab-bar">
          <button
            className={`tab-item ${activeTab === "services" ? "active" : ""}`}
            onClick={() => setActiveTab("services")}
          >
            Platformlar ({cfg.enabled_apps.length})
          </button>
          <button
            className={`tab-item ${activeTab === "settings" ? "active" : ""}`}
            onClick={() => setActiveTab("settings")}
          >
            ISS & Strateji
          </button>
          <button
            className={`tab-item ${activeTab === "rules" ? "active" : ""}`}
            onClick={() => setActiveTab("rules")}
          >
            Özel ({cfg.custom_domains.length})
          </button>
          <button
            className={`tab-item ${activeTab === "preferences" ? "active" : ""}`}
            onClick={() => setActiveTab("preferences")}
          >
            ⚙️ Ayarlar
          </button>
        </div>

        {/* TAB 1: SERVICES (Filtered + Search + Bulk Actions + Quick Ping) */}
        {activeTab === "services" && (
          <section className="tab-pane">
            {/* Search + Add Platform Row */}
            <div className="search-toolbar">
              <div className="search-bar">
                <input
                  className="search-input"
                  placeholder="🔍 Platform veya oyun ara..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                />
                {searchQuery && (
                  <button className="search-clear" onClick={() => setSearchQuery("")}>
                    ✕
                  </button>
                )}
              </div>
              <button
                className="add-platform-btn"
                onClick={() => setShowAddModal(true)}
                title="Yeni uygulama veya oyun ekle"
              >
                + Ekle
              </button>
            </div>

            {/* Category Filter Pills */}
            <div className="category-pills">
              <button
                className={`pill-btn ${platformFilter === "all" ? "active" : ""}`}
                onClick={() => setPlatformFilter("all")}
              >
                Tümü ({allPlatforms.length})
              </button>
              <button
                className={`pill-btn ${platformFilter === "apps" ? "active" : ""}`}
                onClick={() => setPlatformFilter("apps")}
              >
                Uygulama ({appCount})
              </button>
              <button
                className={`pill-btn ${platformFilter === "games" ? "active" : ""}`}
                onClick={() => setPlatformFilter("games")}
              >
                Oyun ({gameCount})
              </button>
            </div>

            {/* Quick Actions & Ready Preset Chips */}
            <div className="quick-actions-bar">
              <button
                className="quick-chip"
                onClick={selectPopular}
                title="Discord, YouTube, Roblox"
              >
                ⭐ Popüler
              </button>
              <button
                className="quick-chip"
                onClick={() => applyPreset("gamer")}
                title="Roblox, Steam, Epic Games, Riot, Minecraft"
              >
                🎮 Oyun
              </button>
              <button
                className="quick-chip"
                onClick={() => applyPreset("social")}
                title="Discord, X, Meta, Telegram, Reddit, TikTok"
              >
                💬 Sosyal
              </button>
              <button
                className="quick-chip"
                onClick={() => applyPreset("media")}
                title="YouTube, Twitch, Kick, Spotify"
              >
                📺 Medya
              </button>
              <button
                className="quick-chip secondary"
                onClick={() => toggleSelectAll(true)}
                title="Tüm platformları seç"
              >
                Tümünü Seç
              </button>
              <button
                className="quick-chip secondary"
                onClick={() => toggleSelectAll(false)}
                title="Seçimleri kaldır"
              >
                Temizle
              </button>
            </div>

            {/* Platform List */}
            <div className="app-list">
              {filteredPlatforms.length === 0 ? (
                <div className="empty-state">
                  <p>Aramanızla eşleşen platform bulunamadı.</p>
                </div>
              ) : (
                filteredPlatforms.map((a) => {
                  const isChecked = cfg.enabled_apps.includes(a.id);

                  return (
                    <div
                      key={a.id}
                      className={`app-card ${isChecked ? "active" : ""}`}
                      onClick={() => toggleApp(a.id)}
                    >
                      <div className="app-card-left">
                        <div className="app-icon">{a.icon}</div>
                        <div className="app-meta">
                          <div className="app-title-row">
                            <span className="app-title">{a.name}</span>
                            {a.isCustom && <span className="custom-badge">Özel</span>}
                          </div>
                          <span className="app-desc">{a.tag}</span>
                        </div>
                      </div>

                      <div className="app-card-right">

                        {a.id === "discord" && isChecked && (
                          <button
                            className="discord-restart-pill"
                            title="Discord'u relay üzerinden temiz başlat"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleRestartDiscord();
                            }}
                          >
                            🔄 Yeniden Başlat
                          </button>
                        )}
                        {a.isCustom && (
                          <button
                            className="delete-custom-btn"
                            title="Bu özel platformu sil"
                            onClick={(e) => {
                              e.stopPropagation();
                              removeCustomPlatform(a.id);
                            }}
                          >
                            ✕
                          </button>
                        )}
                        <div className={`switch ${isChecked ? "checked" : ""}`}>
                          <div className="switch-thumb" />
                        </div>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </section>
        )}

        {/* TAB 2: SETTINGS & STRATEGY */}
        {activeTab === "settings" && (
          <section className="tab-pane">
            <div className="field-group">
              <label className="field-label">İnternet Servis Sağlayıcınız</label>
              <select
                className="select-input"
                value={cfg.isp}
                onChange={(e) => {
                  const newIsp = e.target.value;
                  const firstStrat = cat.isps.find((i) => i.id === newIsp)?.order?.[0] || "fake-split-md5sig";
                  patch({ isp: newIsp, strategy: firstStrat });
                  const ispName = cat.isps.find((i) => i.id === newIsp)?.name || newIsp;
                  setSuccessMsg(`Sağlayıcı ${ispName} seçildi. Önerilen strateji (${firstStrat}) uygulandı.`);
                }}
              >
                {cat.isps.map((i) => (
                  <option key={i.id} value={i.id}>
                    {i.name}
                  </option>
                ))}
              </select>
            </div>

            <div className="field-group">
              <label className="field-label">Kullanılan Bypass Stratejisi</label>
              <select
                className="select-input"
                value={cfg.strategy || "fake-split-md5sig"}
                onChange={(e) => {
                  const newStrat = e.target.value;
                  patch({ strategy: newStrat });
                  const stratName = cat.strategies.find((s) => s.id === newStrat)?.name || newStrat;
                  setSuccessMsg(`Bypass stratejisi uygulandı: ${stratName}`);
                }}
              >
                {cat.strategies.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
              <span className="field-hint">
                Aktif: <b>{currentStrategy}</b>
              </span>
            </div>

            {/* Auto-Tune Box */}
            <div className="autotune-box">
              <button
                className="primary-btn autotune-btn"
                onClick={handleAutoTune}
                disabled={busy}
              >
                {busy ? "Stratejiler Test Ediliyor..." : "⚡ En İyi Ayarı Otomatik Bul"}
              </button>
              <button
                className="text-btn"
                onClick={() => setShowLog(!showLog)}
              >
                {showLog ? "Log Detaylarını Gizle ▲" : "Log Detaylarını Göster ▼"}
              </button>

              {showLog && (
                <div className="log-container">
                  {log.length === 0 ? (
                    <p className="empty-log">Henüz test çalıştırılmadı.</p>
                  ) : (
                    log.map((line, idx) => (
                      <div key={idx} className="log-line">
                        {line}
                      </div>
                    ))
                  )}
                  <div ref={logEndRef} />
                </div>
              )}
            </div>
          </section>
        )}

        {/* TAB 3: CUSTOM RULES */}
        {activeTab === "rules" && (
          <section className="tab-pane">
            <div className="field-group">
              <label className="field-label">Özel Alan Adı Ekle</label>
              <div className="add-row">
                <input
                  className="text-input"
                  value={domainDraft}
                  placeholder="örn: pastebin.com"
                  onChange={(e) => setDomainDraft(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && addRule()}
                />
                <button className="primary-btn add-btn" onClick={addRule}>
                  Ekle
                </button>
              </div>
            </div>

            <div className="rules-list">
              {cfg.custom_domains.length === 0 ? (
                <p className="empty-state">Özel alan adı eklenmedi.</p>
              ) : (
                cfg.custom_domains.map((dom) => (
                  <div key={dom} className="rule-chip">
                    <span className="chip-text">{dom}</span>
                    <button
                      className="chip-remove"
                      onClick={() => removeRule(dom)}
                      title="Sil"
                    >
                      ×
                    </button>
                  </div>
                ))
              )}
            </div>
          </section>
        )}

        {/* TAB 4: ADVANCED PREFERENCES (Ayarlar & Sistem) */}
        {activeTab === "preferences" && (
          <section className="tab-pane">
            {/* Group 1: Sistem ve Başlangıç */}
            <div className="pref-section-title">💻 Sistem & Başlangıç</div>
            <div className="pref-card">
              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Windows Açılınca Başlat</span>
                  <span className="pref-desc">Bilgisayarınız açıldığında Slipstream arka planda otomatik başlar.</span>
                </div>
                <div
                  className={`switch ${cfg.auto_start_pc ? "checked" : ""}`}
                  onClick={handleToggleAutostart}
                >
                  <div className="switch-thumb" />
                </div>
              </div>

              <div className="pref-divider" />

              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Açılışta Otomatik Koruma</span>
                  <span className="pref-desc">Uygulama açılır açılmaz bypass korumasını kendiliğinden devreye sokar.</span>
                </div>
                <div
                  className={`switch ${cfg.auto_connect ? "checked" : ""}`}
                  onClick={() => patch({ auto_connect: !cfg.auto_connect })}
                >
                  <div className="switch-thumb" />
                </div>
              </div>

              <div className="pref-divider" />

              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Başlangıçta Tepsiye Gizlen</span>
                  <span className="pref-desc">Açılışta ekranda pencere göstermez, doğrudan görev çubuğu tepsisine yerleşir.</span>
                </div>
                <div
                  className={`switch ${cfg.start_minimized ? "checked" : ""}`}
                  onClick={() => patch({ start_minimized: !cfg.start_minimized })}
                >
                  <div className="switch-thumb" />
                </div>
              </div>

              <div className="pref-divider" />

              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Odak Kaybında Otomatik Gizle</span>
                  <span className="pref-desc">Başka bir pencereye veya masaüstüne tıklandığında pencereyi otomatik sistem tepsisine küçültür.</span>
                </div>
                <div
                  className={`switch ${cfg.hide_on_blur ? "checked" : ""}`}
                  onClick={() => {
                    const nextVal = !cfg.hide_on_blur;
                    patch({ hide_on_blur: nextVal });
                    invoke("set_hide_on_blur", { enabled: nextVal }).catch(() => {});
                  }}
                >
                  <div className="switch-thumb" />
                </div>
              </div>
            </div>

            {/* Group 2: Güncelleme Yönetimi */}
            <div className="pref-section-title">🔄 Güncelleme Yönetimi</div>
            <div className="pref-card">
              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Otomatik Güncelleme</span>
                  <span className="pref-desc">Yeni sürüm çıktığında kullanıcıyı rahatsız etmeden arka planda indirip kurar.</span>
                </div>
                <div
                  className={`switch ${cfg.auto_update ? "checked" : ""}`}
                  onClick={() => patch({ auto_update: !cfg.auto_update })}
                >
                  <div className="switch-thumb" />
                </div>
              </div>

              <div className="pref-divider" />

              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Sürüm Kontrolü</span>
                  <span className="pref-desc">
                    Mevcut: v{updateInfo?.current_version || "0.1.1"}
                    {updateInfo?.has_update ? (
                      <span style={{ color: "var(--cyan)", fontWeight: 600, marginLeft: 6 }}>
                        (🚀 v{updateInfo.latest_version} Yayında)
                      </span>
                    ) : (
                      <span style={{ color: "var(--text-muted)", marginLeft: 6 }}>
                        (Son Sürüm)
                      </span>
                    )}
                  </span>
                </div>
                <div style={{ display: "flex", gap: 6 }}>
                  <button
                    className="secondary-btn"
                    style={{ padding: "6px 10px", fontSize: 11 }}
                    onClick={() => invoke("open_url", { url: "https://github.com/Baran-K35/Slipstream/releases" })}
                    title="GitHub Releases sayfasını aç"
                  >
                    GitHub
                  </button>
                  <button
                    className="secondary-btn"
                    style={{ padding: "6px 12px", fontSize: 11 }}
                    onClick={() => handleCheckUpdates(true)}
                    disabled={checkingUpdates}
                  >
                    {checkingUpdates ? "Kontrol Ediliyor..." : "Güncellemeleri Denetle"}
                  </button>
                </div>
              </div>
            </div>

            {/* Group 3: Görünüm & Ses Deneyimi */}
            <div className="pref-section-title">🎨 Görünüm & Deneyim</div>
            <div className="pref-card">
              <div className="field-group" style={{ marginBottom: 8 }}>
                <label className="field-label">Görsel Tema</label>
                <div className="theme-selector">
                  <button
                    className={`theme-btn ${theme === "cyber" ? "active" : ""}`}
                    onClick={() => setTheme("cyber")}
                  >
                    🌙 Siber Karanlık
                  </button>
                  <button
                    className={`theme-btn ${theme === "amoled" ? "active" : ""}`}
                    onClick={() => setTheme("amoled")}
                  >
                    🌌 OLED Saf Siyah
                  </button>
                  <button
                    className={`theme-btn ${theme === "emerald" ? "active" : ""}`}
                    onClick={() => setTheme("emerald")}
                  >
                    🌲 Zümrüt Oyuncu
                  </button>
                </div>
              </div>

              <div className="pref-divider" />

              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Ses Efektleri</span>
                  <span className="pref-desc">Bypass açılıp kapandığında hafif futuristik akustik sinyal verir.</span>
                </div>
                <div
                  className={`switch ${soundEnabled ? "checked" : ""}`}
                  onClick={() => setSoundEnabled(!soundEnabled)}
                >
                  <div className="switch-thumb" />
                </div>
              </div>
            </div>

            {/* Group 3: Güvenli Ağ & DoH Benchmark */}
            <div className="pref-section-title">🌐 Güvenli Ağ & DNS (DoH)</div>
            <div className="pref-card">
              <div className="field-group" style={{ marginBottom: 6 }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 4 }}>
                  <label className="field-label" style={{ marginBottom: 0 }}>DoH DNS Sağlayıcısı</label>
                  <button
                    className="tool-btn"
                    style={{ flex: "none", padding: "3px 8px", fontSize: 10 }}
                    onClick={handleBenchDoh}
                    disabled={benchmarkingDoh}
                  >
                    {benchmarkingDoh ? "Test Ediliyor..." : "⚡ En Hızlı DNS'i Test Et & Seç"}
                  </button>
                </div>
                <select
                  className="select-input"
                  value={cfg.doh_provider}
                  onChange={(e) => patch({ doh_provider: e.target.value })}
                >
                  <option value="cloudflare">Cloudflare (1.1.1.1 - En Hızlı)</option>
                  <option value="google">Google DNS (dns.google - Maksimum Kararlılık)</option>
                  <option value="quad9">Quad9 (9.9.9.9 - Gizlilik ve Güvenlik)</option>
                  <option value="adguard">AdGuard (dns.adguard-dns.com - Reklam Engelleyici)</option>
                </select>
              </div>

              {/* 4-provider benchmark grid */}
              {dohBenchmarks.length > 0 && (
                <div className="doh-bench-grid">
                  {dohBenchmarks.map((b) => {
                    const isSelected = cfg.doh_provider === b.provider;
                    return (
                      <div
                        key={b.provider}
                        className={`doh-bench-card ${isSelected ? "selected" : ""}`}
                        onClick={() => patch({ doh_provider: b.provider })}
                      >
                        <div className="doh-card-header">
                          <span className="doh-card-title">{b.provider.toUpperCase()}</span>
                          <span
                            className={`doh-card-ping ${
                              b.latency === null
                                ? "error"
                                : b.latency < 100
                                ? "fast"
                                : "slow"
                            }`}
                          >
                            {b.latency !== null ? `${b.latency}ms` : "Hata"}
                          </span>
                        </div>
                        {isSelected && <span className="doh-best-tag">Aktif</span>}
                      </div>
                    );
                  })}
                </div>
              )}

              <div className="tool-buttons-row">
                <button className="tool-btn" onClick={handleFlushDns} title="Windows DNS önbelleğini boşalt">
                  🔄 DNS Önbelleğini Temizle
                </button>
                <button className="tool-btn" onClick={handleResetAdapterDns} title="Ağ kartlarını otomatik DHCP'ye döndür">
                  🔁 Ağ Kartlarını Sıfırla
                </button>
                <button className="tool-btn" onClick={handleRunHealthCheck} title="Ağ ve bypass durumunu kontrol et">
                  🩺 Ağ Teşhisi
                </button>
              </div>
            </div>

            {/* Group 4: Discord Onarım Merkezi */}
            <div className="pref-section-title">💬 Discord Onarım & Durum Merkezi</div>
            <div className="pref-card">
              <div className="discord-diag-card">
                <div className="diag-row">
                  <span className="diag-label">18809 Yerel Aktarıcı:</span>
                  <span className={`diag-val ${discordDiag?.relay_ok ? "ok" : "warn"}`}>
                    {discordDiag?.relay_ok ? "🟢 Hazır ve Dinliyor" : "⚪ Beklemede"}
                  </span>
                </div>
                <div className="diag-row">
                  <span className="diag-label">Discord settings.json:</span>
                  <span className={`diag-val ${discordDiag?.patched ? "ok" : "warn"}`}>
                    {discordDiag?.patched ? "🟢 NEW_UPDATE_ENDPOINT Entegre" : "⚪ Standart"}
                  </span>
                </div>
                <div className="diag-row">
                  <span className="diag-label">Discord Uygulama Süreci:</span>
                  <span className={`diag-val ${discordDiag?.running ? "ok" : "warn"}`}>
                    {discordDiag?.running ? "🟢 Çalışıyor" : "⚪ Kapalı"}
                  </span>
                </div>
              </div>

              <div className="tool-buttons-row" style={{ marginTop: 6 }}>
                <button
                  className="tool-btn"
                  onClick={handleRepairDiscord}
                  disabled={repairingDiscord}
                  title="Discord güncelleme kilitlerini kaldırır, relay ayarını pekiştirir ve temiz başlatır"
                >
                  {repairingDiscord ? "Onarılıyor..." : "🔄 Discord'u Sıfırla ve Temiz Başlat"}
                </button>
              </div>
            </div>

            {/* Group 5: Performans & Oyun Modu */}
            <div className="pref-section-title">🎮 Performans & Oyun Modu</div>
            <div className="pref-card">
              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Düşük Gecikmeli Oyun & Ses Modu</span>
                  <span className="pref-desc">Valorant, CS2, Roblox ve Discord ses paketlerine işlemci önceliği vererek ping dalgalanmasını önler.</span>
                </div>
                <div
                  className={`switch ${cfg.gaming_mode ? "checked" : ""}`}
                  onClick={() => patch({ gaming_mode: !cfg.gaming_mode })}
                >
                  <div className="switch-thumb" />
                </div>
              </div>

              <div className="pref-divider" />

              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Kendi Kendini Onaran Bekçi (Watchdog)</span>
                  <span className="pref-desc">DPI motoru bir sebeple kapanırsa saniyeler içinde arka planda sessizce yeniden ayağa kaldırır.</span>
                </div>
                <div
                  className={`switch ${cfg.auto_healing ? "checked" : ""}`}
                  onClick={() => patch({ auto_healing: !cfg.auto_healing })}
                >
                  <div className="switch-thumb" />
                </div>
              </div>
            </div>

            {/* Group 6: Yedekleme & Dışa/İçe Aktarma */}
            <div className="pref-section-title">💾 Yedekleme & Yönetim</div>
            <div className="pref-card">
              <div className="tool-buttons-row">
                <button className="tool-btn" onClick={handleExportConfig}>
                  📥 Ayarları Dışa Aktar (JSON)
                </button>
                <button
                  className="tool-btn"
                  onClick={async () => {
                    try {
                      await invoke("set_file_dialog_active", { active: true });
                    } catch {}
                    fileInputRef.current?.click();
                    setTimeout(() => {
                      invoke("set_file_dialog_active", { active: false }).catch(() => {});
                    }, 15000);
                  }}
                >
                  📤 Ayarları İçe Aktar (JSON)
                </button>
                <input
                  type="file"
                  ref={fileInputRef}
                  style={{ display: "none" }}
                  accept=".json"
                  onChange={handleImportConfig}
                />
              </div>
            </div>

            {/* Group 7: Yasal Şartlar & Gizlilik */}
            <div className="pref-section-title">📜 Yasal Şartlar & Gizlilik</div>
            <div className="pref-card">
              <div className="pref-row">
                <div className="pref-meta">
                  <span className="pref-name">Kullanım Şartları & Gizlilik (EULA)</span>
                  <span className="pref-desc">Veri toplamama ilkesi, yerel çalışma prensibi ve açık kaynak kullanım kuralları.</span>
                </div>
                <button
                  className="secondary-btn"
                  style={{ padding: "6px 12px", fontSize: 11 }}
                  onClick={() => setShowEulaModal(true)}
                >
                  Metni Oku
                </button>
              </div>
            </div>
          </section>
        )}

        {/* Floating Notification Toasts */}
        {err && (
          <div className="alert-toast alert-error" role="alert">
            <span className="alert-icon">⚠️</span>
            <span className="alert-text">{err}</span>
            <button className="toast-close" onClick={() => setErr(null)}>✕</button>
          </div>
        )}

        {successMsg && (
          <div className="alert-toast alert-success" role="alert">
            <span className="alert-icon">✅</span>
            <span className="alert-text">{successMsg}</span>
            <button className="toast-close" onClick={() => setSuccessMsg(null)}>✕</button>
          </div>
        )}
      </div>

      {/* Add Platform Modal */}
      {showAddModal && (
        <div className="modal-backdrop" onClick={() => setShowAddModal(false)}>
          <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3 className="modal-title">Yeni Platform / Oyun Ekle</h3>
              <button className="modal-close" onClick={() => setShowAddModal(false)}>
                ✕
              </button>
            </div>

            <div className="modal-body">
              <div className="field-group">
                <label className="field-label">Kategori Seçin</label>
                <div className="modal-segmented">
                  <button
                    className={`segment-btn ${addCategory === "app" ? "active" : ""}`}
                    onClick={() => setAddCategory("app")}
                  >
                    📱 Uygulama
                  </button>
                  <button
                    className={`segment-btn ${addCategory === "game" ? "active" : ""}`}
                    onClick={() => setAddCategory("game")}
                  >
                    🎮 Oyun
                  </button>
                </div>
              </div>

              <div className="field-group">
                <label className="field-label">Platform / Oyun Adı</label>
                <input
                  className="text-input"
                  placeholder="örn: Kick, Wattpad, Valorant..."
                  value={addName}
                  onChange={(e) => setAddName(e.target.value)}
                />
              </div>

              <div className="field-group">
                <label className="field-label">Web Sitesi veya Ana Alan Adı</label>
                <input
                  className="text-input"
                  placeholder="örn: kick.com veya https://kick.com"
                  value={addUrl}
                  onChange={(e) => setAddUrl(e.target.value)}
                />
                <span className="field-hint">
                  Girdiğiniz adrese göre web sitesi, API, CDN ve oturum sunucuları otomatik algılanır.
                </span>
              </div>

              {expandedPreview.length > 0 && (
                <div className="expanded-preview-box">
                  <span className="preview-label">
                    ✨ Otomatik Algılanan URL ve Alan Adları ({expandedPreview.length}):
                  </span>
                  <div className="preview-chips">
                    {expandedPreview.map((d) => (
                      <span key={d} className="preview-chip">
                        {d}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>

            <div className="modal-footer">
              <button className="secondary-btn" onClick={() => setShowAddModal(false)}>
                İptal
              </button>
              <button
                className="primary-btn"
                onClick={handleSaveCustomPlatform}
                disabled={!addName.trim() || expandedPreview.length === 0}
              >
                Kaydet ve Aktif Et
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Health Check Modal */}
      {showHealthModal && (
        <div className="modal-backdrop" onClick={() => setShowHealthModal(false)}>
          <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3 className="modal-title">🩺 Sistem & Ağ Sağlık Raporu</h3>
              <button className="modal-close" onClick={() => setShowHealthModal(false)}>
                ✕
              </button>
            </div>

            <div className="modal-body">
              {checkingHealth ? (
                <div className="boot-container" style={{ height: 160 }}>
                  <div className="spinner" />
                  <p>Ağ ve çekirdek katmanları test ediliyor...</p>
                </div>
              ) : networkHealth ? (
                <div className="health-list">
                  <div className="health-item">
                    <div className="health-item-left">
                      <div>
                        <div className="health-item-title">WinDivert Sürücüsü</div>
                        <div className="health-item-desc">Kernel seviyesi paket desync motoru</div>
                      </div>
                    </div>
                    <span className={`health-status-badge ${networkHealth.windivert_driver_ok ? "ok" : "fail"}`}>
                      {networkHealth.windivert_driver_ok ? "Aktif & Hazır" : "Hata"}
                    </span>
                  </div>

                  <div className="health-item">
                    <div className="health-item-left">
                      <div>
                        <div className="health-item-title">Sistem Proxy Durumu</div>
                        <div className="health-item-desc">Windows ayarlarında proxy kapalı, sıfır lag</div>
                      </div>
                    </div>
                    <span className={`health-status-badge ${networkHealth.proxy_clean ? "ok" : "fail"}`}>
                      {networkHealth.proxy_clean ? "Temiz (0 Ping)" : "Proxy Algılandı"}
                    </span>
                  </div>

                  <div className="health-item">
                    <div className="health-item-left">
                      <div>
                        <div className="health-item-title">DoH Şifreli DNS</div>
                        <div className="health-item-desc">TLS 1.3 ({networkHealth.active_doh_provider})</div>
                      </div>
                    </div>
                    <span className={`health-status-badge ${networkHealth.doh_ok ? "ok" : "fail"}`}>
                      {networkHealth.doh_ok ? "Bağlantı Başarılı" : "Hata"}
                    </span>
                  </div>

                  <div className="health-item">
                    <div className="health-item-left">
                      <div>
                        <div className="health-item-title">Discord Güncelleyici Tüneli</div>
                        <div className="health-item-desc">127.0.0.1:18809 yerel aktarıcı</div>
                      </div>
                    </div>
                    <span className={`health-status-badge ${networkHealth.discord_relay_ok ? "ok" : "fail"}`}>
                      {networkHealth.discord_relay_ok ? "Dinliyor" : "Kapalı"}
                    </span>
                  </div>
                </div>
              ) : (
                <p className="empty-state">Teşhis verisi alınamadı.</p>
              )}
            </div>

            <div className="modal-footer">
              <button className="primary-btn" onClick={() => setShowHealthModal(false)}>
                Tamam
              </button>
            </div>
          </div>
        </div>
      )}

      {/* EULA & Kullanım Şartları Modal */}
      {showEulaModal && (
        <div className="modal-overlay" onClick={() => setShowEulaModal(false)}>
          <div className="modal-card" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 440 }}>
            <div className="modal-header">
              <span className="modal-title">📜 Kullanım Şartları ve Gizlilik (EULA)</span>
              <button className="modal-close-btn" onClick={() => setShowEulaModal(false)}>✕</button>
            </div>
            <div className="modal-body">
              <div className="eula-modal-body">
                <h4>1. Gizlilik ve Sıfır Veri Toplama Güvencesi</h4>
                <p>
                  Slipstream, tamamen bilgisayarınızda yerel (client-side) olarak çalışan bağımsız bir masaüstü aracıdır.
                  Uygulamamız hiçbir kişisel verinizi, IP adresinizi, tarama geçmişinizi veya kimlik bilginizi toplamaz,
                  kaydetmez ve hiçbir uzak sunucuya aktarmaz.
                </p>

                <h4>2. Yerel Ağ ve Paket İşleme İlkeleri</h4>
                <p>
                  Slipstream, harici bir VPN veya uzak proxy sunucusu değildir. İnternet trafiğinizi üçüncü parti sunuculardan
                  geçirmez; bu sayede ping ve hız kaybı yaşatmaz. Ağ paketleri yalnızca sizin cihazınız ile hedef sunucu arasında
                  desenkronize edilir.
                </p>

                <h4>3. Sorumluluk Reddi (Disclaimer)</h4>
                <p>
                  Slipstream, eğitim, araştırma ve sansürsüz özgür internet erişimi amacıyla geliştirilmiş açık kaynaklı bir yazılımdır.
                  Yazılım "olduğu gibi" (AS-IS) sunulmakta olup; kullanıcının ziyaret ettiği web siteleri ve internet faaliyetlerinden
                  tamamen kullanıcının kendisi sorumludur.
                </p>

                <h4>4. Lisans ve Kapsam</h4>
                <p>
                  Bu yazılım MIT Açık Kaynak Lisansı kapsamında sunulmaktadır. Sadece kişisel bilgisayar ortamında kullanılmak üzere
                  tasarlanmış olup, mobil veya sunucu platformları için ticari veri toplama yükümlülüklerine tabi değildir.
                </p>
              </div>
            </div>
            <div className="modal-footer">
              <button className="primary-btn" onClick={() => setShowEulaModal(false)}>
                Anladım ve Kabul Ediyorum
              </button>
            </div>
          </div>
        </div>
      )}



      {/* Flyout Footer */}
      <footer className="footer">
        <span className="footer-status">
          {on ? "🟢 Koruma Devrede (0 Ping)" : "⚪ Servis Kapalı"}
        </span>
        <button
          className="quit-btn"
          title="Uygulamadan ve arka plan servisinden tamamen çık"
          onClick={() => invoke("close_app")}
        >
          Tamamen Kapat
        </button>
      </footer>
    </div>
  );
}
