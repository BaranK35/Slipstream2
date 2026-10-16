import { useEffect, useState, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./styles.css";
import logoSvg from "./assets/logo.svg";
import { translations, LANGUAGES, type LangCode } from "./translations";

type CustomPlatform = {
  id: string;
  name: string;
  category: "app" | "game";
  icon: string;
  domains: string[];
  exe_name?: string;
  exe_path?: string;
};

type ScannedApp = {
  id: string;
  name: string;
  exe_name: string;
  exe_path: string | null;
  category: "game" | "app";
  is_running: boolean;
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
  game_priority_boost?: boolean;
  auto_healing: boolean;
  auto_update?: boolean;
  process_detection_mode?: boolean;
  app_defaults_version?: number;
};

type DomainSuggestionResult = {
  input: string;
  base_domain: string;
  display_name: string;
  suggestions: string[];
  explanation: string;
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

function playTone(type: "on" | "off" | "click" | "toggle") {
  invoke("play_sound", { sound: type }).catch(() => {});
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

  const [lang, setLang] = useState<LangCode>(() => {
    try {
      const saved = localStorage.getItem("slipstream_lang") as LangCode;
      if (saved && LANGUAGES.some((l) => l.code === saved)) {
        return saved;
      }
      return "tr";
    } catch {
      return "tr";
    }
  });

  const isRtl = LANGUAGES.find((l) => l.code === lang)?.rtl ?? false;

  useEffect(() => {
    try {
      localStorage.setItem("slipstream_lang", lang);
    } catch {}
    if (typeof document !== "undefined") {
      document.documentElement.dir = isRtl ? "rtl" : "ltr";
      document.documentElement.lang = lang;
    }
  }, [lang, isRtl]);

  const t = translations[lang] || translations.tr;

  // Appearance, Themes & Personalization
  type ThemeId = "cyber" | "erkek" | "amoled" | "emerald" | "crimson" | "violet" | "frost";

  const THEMES: { id: ThemeId; name: string; accent: string; bg: string; dot: string }[] = [
    { id: "cyber", name: t.theme_cyber, accent: "#6366f1", bg: "#0f1424", dot: "#6366f1" },
    { id: "erkek", name: t.theme_erkek, accent: "#f43f85", bg: "#170713", dot: "#f43f85" },
    { id: "amoled", name: t.theme_amoled, accent: "#3b82f6", bg: "#050505", dot: "#3b82f6" },
    { id: "emerald", name: t.theme_emerald, accent: "#10b981", bg: "#06150e", dot: "#10b981" },
    { id: "crimson", name: t.theme_crimson, accent: "#f43f5e", bg: "#16070a", dot: "#f43f5e" },
    { id: "violet", name: t.theme_violet, accent: "#a855f7", bg: "#12091f", dot: "#a855f7" },
    { id: "frost", name: t.theme_frost, accent: "#06b6d4", bg: "#07131d", dot: "#06b6d4" },
  ];

  const [theme, setTheme] = useState<ThemeId>(() => {
    try {
      return (localStorage.getItem("slipstream_theme") as ThemeId) || "cyber";
    } catch {
      return "cyber";
    }
  });

  const [prefSubTab, setPrefSubTab] = useState<"genel" | "gelismis" | "kisisellestir">("genel");

  const [cardDensity, setCardDensity] = useState<"comfortable" | "compact">(() => {
    try {
      return (localStorage.getItem("slipstream_density") as "comfortable" | "compact") || "compact";
    } catch {
      return "compact";
    }
  });

  const [ambientGlow, setAmbientGlow] = useState<boolean>(() => {
    try {
      const v = localStorage.getItem("slipstream_glow");
      return v === null ? true : v === "true";
    } catch {
      return true;
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
  const [domainSuggestion, setDomainSuggestion] = useState<DomainSuggestionResult | null>(null);
  const [selectedSuggestions, setSelectedSuggestions] = useState<string[]>([]);
  const [showSuggestionModal, setShowSuggestionModal] = useState(false);
  const [suggestingDomain, setSuggestingDomain] = useState(false);

  // Portable & Live process detection
  const [isPortable, setIsPortable] = useState(false);
  const [detectedProcesses, setDetectedProcesses] = useState<string[]>([]);
  const [isWindowVisible, setIsWindowVisible] = useState<boolean>(!document.hidden);
  const lastUpdateAttemptRef = useRef<number>(0);
  const busyRef = useRef<boolean>(false);

  // Add Platform Modal state
  const [showAddModal, setShowAddModal] = useState(false);
  const [addModalTab, setAddModalTab] = useState<"scan" | "manual">("scan");
  const [scannedApps, setScannedApps] = useState<ScannedApp[]>([]);
  const [loadingScannedApps, setLoadingScannedApps] = useState(false);
  const [scannedSearch, setScannedSearch] = useState("");
  const [scannedFilter, setScannedFilter] = useState<"all" | "games" | "apps" | "running" | "stopped">("all");
  const [addName, setAddName] = useState("");
  const [addCategory, setAddCategory] = useState<"app" | "game">("app");
  const [addUrl, setAddUrl] = useState("");
  const [expandedPreview, setExpandedPreview] = useState<string[]>([]);

  const logEndRef = useRef<HTMLDivElement>(null);
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
  const [showReleaseNotesModal, setShowReleaseNotesModal] = useState(false);

  // Theme & Personalization synchronization
  useEffect(() => {
    try {
      document.documentElement.dataset.theme = theme;
      localStorage.setItem("slipstream_theme", theme);
    } catch {}
  }, [theme]);

  useEffect(() => {
    try {
      localStorage.setItem("slipstream_density", cardDensity);
    } catch {}
  }, [cardDensity]);

  useEffect(() => {
    try {
      localStorage.setItem("slipstream_glow", String(ambientGlow));
      if (ambientGlow) {
        document.documentElement.removeAttribute("data-glow");
      } else {
        document.documentElement.setAttribute("data-glow", "minimal");
      }
    } catch {}
  }, [ambientGlow]);

  useEffect(() => {
    try {
      localStorage.setItem("slipstream_sound", String(soundEnabled));
    } catch {}
  }, [soundEnabled]);

  const getDisplayVersion = (rawVer?: string) => {
    const v = (rawVer || "0.2.1").replace(/^v/, "");
    const parts = v.split(/[.+]/);
    if (parts.length > 3) {
      return `${parts[0]}.${parts[1]}.${parts[2]}`;
    }
    return v;
  };

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
          game_priority_boost: loadedCfg.game_priority_boost ?? false,
          auto_healing: loadedCfg.auto_healing ?? true,
          auto_update: loadedCfg.auto_update ?? true,
          process_detection_mode: loadedCfg.process_detection_mode ?? false,
          app_defaults_version: loadedCfg.app_defaults_version ?? 1,
        };
        setCfg(initialCfg);
        setOn(engineState);

        // Check if running in portable mode
        invoke<boolean>("is_portable").then(setIsPortable).catch(() => {});

        // Fetch initial active detected processes
        invoke<string[]>("get_detected_active_apps", { cfg: initialCfg })
          .then(setDetectedProcesses)
          .catch(() => {});

        // Check for updates on boot immediately with cooldown protection
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
              const now = Date.now();
              if (now - lastUpdateAttemptRef.current > 15 * 60 * 1000) {
                handleApplyUpdate(info.download_url);
              }
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
      if (soundEnabled && !busyRef.current) playTone(e.payload ? "on" : "off");
    });

    const unlistenDns = listen("dns_flushed", () => {
      setSuccessMsg(t.suc_dnsFlushed);
    });

    const unlistenDetected = listen<string[]>("detected_processes_changed", (e) => {
      setDetectedProcesses(e.payload || []);
    });

    const unlistenAutoBypass = listen<string>("auto_bypass_recovered", (e) => {
      setSuccessMsg(e.payload);
    });

    const unlistenStrategy = listen<string>("strategy_changed", (e) => {
      setCfg((prev) => (prev ? { ...prev, strategy: e.payload } : null));
    });

    const unlistenVis = listen<boolean>("app_window_visibility", (e) => {
      const vis = Boolean(e.payload);
      setIsWindowVisible(vis);
      if (vis) {
        document.documentElement.classList.remove("window-hidden");
      } else {
        document.documentElement.classList.add("window-hidden");
      }
    });

    const onVisibilityChange = () => {
      const vis = !document.hidden;
      setIsWindowVisible(vis);
      if (vis) {
        document.documentElement.classList.remove("window-hidden");
      } else {
        document.documentElement.classList.add("window-hidden");
      }
    };
    document.addEventListener("visibilitychange", onVisibilityChange);

    // Sync initial visibility directly with backend window state
    invoke<boolean>("is_window_visible")
      .then((vis) => {
        setIsWindowVisible(vis);
        if (vis) {
          document.documentElement.classList.remove("window-hidden");
        } else {
          document.documentElement.classList.add("window-hidden");
        }
      })
      .catch(() => {});

    // Check initial Discord diagnosis
    invoke<DiscordDiagnosis>("diagnose_discord").then(setDiscordDiag).catch(() => {});

    return () => {
      document.removeEventListener("visibilitychange", onVisibilityChange);
      unlistenVis.then((f) => f());
      unlistenAutotune.then((f) => f());
      unlistenWatchdog.then((f) => f());
      unlistenState.then((f) => f());
      unlistenDns.then((f) => f());
      unlistenDetected.then((f) => f());
      unlistenAutoBypass.then((f) => f());
      unlistenStrategy.then((f) => f());
    };
  }, []);

  // Latency Ping Radar - only checks once on window display, zero continuous polling
  useEffect(() => {
    if (!isWindowVisible) return;
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
    return () => {
      active = false;
    };
  }, [on, isWindowVisible]);

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

  // When adding URL or Platform Name, dynamically predict and expand domains
  useEffect(() => {
    const rawTarget = addUrl.trim() || addName.trim();
    if (!rawTarget) {
      setExpandedPreview([]);
      return;
    }
    const t = setTimeout(async () => {
      try {
        let res = await invoke<string[]>("expand_url", { raw: rawTarget });
        // If both addUrl and addName are provided and distinct, merge predictions
        if (addUrl.trim() && addName.trim() && addUrl.trim() !== addName.trim()) {
          try {
            const res2 = await invoke<string[]>("expand_url", { raw: addName.trim() });
            res = Array.from(new Set([...res, ...res2])).sort();
          } catch {}
        }
        setExpandedPreview(res);
      } catch {
        setExpandedPreview([]);
      }
    }, 200);
    return () => clearTimeout(t);
  }, [addUrl, addName]);

  useEffect(() => {
    if (showAddModal) {
      invoke("set_window_expanded", { expanded: true }).catch(() => {});
    } else {
      invoke("set_window_expanded", { expanded: false }).catch(() => {});
    }
  }, [showAddModal]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape" && showAddModal) {
        setShowAddModal(false);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [showAddModal]);

  async function loadScannedApps() {
    setLoadingScannedApps(true);
    try {
      const list = await invoke<ScannedApp[]>("scan_apps");
      setScannedApps(list || []);
    } catch (e) {
      console.error("Failed to scan apps:", e);
    } finally {
      setLoadingScannedApps(false);
    }
  }

  const filteredScannedApps = useMemo(() => {
    return scannedApps.filter((a) => {
      if (scannedFilter === "games" && a.category !== "game") return false;
      if (scannedFilter === "apps" && a.category !== "app") return false;
      if (scannedFilter === "running" && !a.is_running) return false;
      if (scannedFilter === "stopped" && a.is_running) return false;
      if (scannedSearch.trim()) {
        const q = scannedSearch.toLowerCase();
        return a.name.toLowerCase().includes(q) || a.exe_name.toLowerCase().includes(q);
      }
      return true;
    });
  }, [scannedApps, scannedFilter, scannedSearch]);

  const runningScannedCount = useMemo(() => scannedApps.filter((a) => a.is_running).length, [scannedApps]);
  const stoppedScannedCount = useMemo(() => scannedApps.filter((a) => !a.is_running).length, [scannedApps]);

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
        exe_name: undefined as string | undefined,
        exe_path: undefined as string | undefined,
      })),
      ...(cfg.custom_platforms || []).map((cp) => ({
        id: cp.id,
        name: cp.name,
        category: cp.category,
        tag: cp.exe_name ? cp.exe_name : t.customPlatformTag(cp.domains.length),
        icon: cp.icon || (cp.category === "game" ? "🎮" : "🌐"),
        domains: cp.domains,
        probe: cp.domains[0] ? `https://${cp.domains[0]}` : "https://google.com",
        isCustom: true,
        exe_name: cp.exe_name,
        exe_path: cp.exe_path,
      })),
    ];
  }, [cat, cfg]);

  const filteredPlatforms = useMemo(() => {
    const enabledSet = new Set(cfg?.enabled_apps || []);
    const detectedSet = new Set(cfg?.process_detection_mode ? detectedProcesses : []);
    return allPlatforms
      .filter((p) => {
        if (platformFilter === "apps" && p.category !== "app") return false;
        if (platformFilter === "games" && p.category !== "game") return false;
        if (searchQuery.trim()) {
          const q = searchQuery.toLowerCase();
          return p.name.toLowerCase().includes(q) || p.tag.toLowerCase().includes(q);
        }
        return true;
      })
      .sort((a, b) => {
        // Running process priority in 3. Yöntem
        const isADetected = detectedSet.has(a.id);
        const isBDetected = detectedSet.has(b.id);
        if (isADetected && !isBDetected) return -1;
        if (!isADetected && isBDetected) return 1;

        const isAEnabled = enabledSet.has(a.id);
        const isBEnabled = enabledSet.has(b.id);
        if (isAEnabled && !isBEnabled) return -1;
        if (!isAEnabled && isBEnabled) return 1;
        return 0;
      });
  }, [allPlatforms, platformFilter, searchQuery, cfg?.enabled_apps, cfg?.process_detection_mode, detectedProcesses]);

  if (!isWindowVisible) {
    return <div id="hibernation-sleep-node" style={{ display: "none" }} />;
  }

  if (!cat || !cfg) {
    return (
      <div className="window-frame" dir={isRtl ? "rtl" : "ltr"}>
        <div className="titlebar" data-tauri-drag-region>
          <div className="brand" data-tauri-drag-region>
            <img src={logoSvg} alt="Slipstream" className="brand-logo" />
            <div className="status-indicator busy" />
            <span className="brand-name" data-tauri-drag-region>{t.appName}</span>
            <span className="version-pill" title={t.versionPrefix(updateInfo?.current_version || "0.2.1")}>v{getDisplayVersion(updateInfo?.current_version)}</span>
          </div>
          <div className="window-controls">
            <button className="win-btn minimize" title={t.minimizeTitle} onClick={() => invoke("hide_window")}>
              <svg viewBox="0 0 10 10">
                <path d="M1 5h8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
              </svg>
            </button>
          </div>
        </div>
        <div className="boot-container">
          <div className="spinner" />
          <p>{t.loadingText}</p>
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

  const handleTabChange = (tab: "services" | "settings" | "rules" | "preferences") => {
    if (tab !== activeTab && soundEnabled) {
      playTone("click");
    }
    setActiveTab(tab);
  };

  const toggleApp = (id: string) => {
    if (soundEnabled) playTone("toggle");
    const list = cfg.enabled_apps.includes(id)
      ? cfg.enabled_apps.filter((x) => x !== id)
      : [...cfg.enabled_apps, id];
    patch({ enabled_apps: list });
  };

  const toggleSelectAll = (select: boolean) => {
    if (soundEnabled) playTone("click");
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
    if (soundEnabled) playTone("click");
    patch({ enabled_apps: ["discord", "roblox"] });
    setSuccessMsg(t.suc_popularSelected);
  };

  const applyPreset = (preset: "gamer" | "social" | "media" | "all") => {
    if (soundEnabled) playTone("click");
    if (!cfg || !cat) return;
    let selected: string[] = [];
    if (preset === "gamer") {
      selected = ["roblox", "steam", "epicgames", "riotgames", "minecraft", "rockstar_blizzard_ea", "discord"];
      patch({ enabled_apps: selected, gaming_mode: true });
      setSuccessMsg(t.suc_gamerPack);
    } else if (preset === "social") {
      selected = ["discord", "x", "meta", "telegram", "reddit", "tiktok"];
      patch({ enabled_apps: selected });
      setSuccessMsg(t.suc_socialPack);
    } else if (preset === "media") {
      selected = ["youtube", "twitch", "kick", "spotify"];
      patch({ enabled_apps: selected });
      setSuccessMsg(t.suc_mediaPack);
    } else if (preset === "all") {
      const allIds = [
        ...cat.apps.map((a) => a.id),
        ...cfg.custom_platforms.map((cp) => cp.id),
      ];
      patch({ enabled_apps: Array.from(new Set(allIds)) });
      setSuccessMsg(t.suc_allSelected);
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
          ? t.suc_startupAdded
          : t.suc_startupRemoved
      );
    } catch (e) {
      setErr(t.err_startupFailed(String(e)));
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
        setSuccessMsg(t.suc_dnsSelected(fastest.name, fastest.latency!));
      }
    } catch (e) {
      setErr(t.err_dnsFailed(String(e)));
    }
    setBenchmarkingDoh(false);
  }

  async function handleRepairDiscord() {
    setRepairingDiscord(true);
    setErr(null);
    try {
      await invoke("repair_discord");
      setSuccessMsg(t.suc_discordRepaired);
      const diag = await invoke<DiscordDiagnosis>("diagnose_discord");
      setDiscordDiag(diag);
    } catch (e) {
      setErr(t.err_discordFailed(String(e)));
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
      setErr(t.err_healthFailed(String(e)));
    }
    setCheckingHealth(false);
  }

  async function handlePower() {
    setBusy(true);
    busyRef.current = true;
    setErr(null);
    setSuccessMsg(null);
    const nextState = !on;
    if (soundEnabled) {
      playTone(nextState ? "on" : "off");
    }
    // Instant optimistic update in 0ms
    setOn(nextState);
    try {
      if (on) {
        await invoke("stop");
      } else {
        await invoke("start", { cfg });
      }
    } catch (e) {
      setErr(String(e));
      setOn(!nextState);
    }
    setBusy(false);
    setTimeout(() => {
      busyRef.current = false;
    }, 600);
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
        setSuccessMsg(t.suc_bestStrategy(best));
      } else {
        setErr(t.suc_noStrategy);
      }
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  }

  async function handleAddScannedApp(app: ScannedApp) {
    if (!cfg) return;

    const id = "custom_" + app.exe_name.toLowerCase().replace(/[^a-z0-9]/g, "_");

    if (cfg.custom_platforms.some((p) => p.id === id || p.exe_name?.toLowerCase() === app.exe_name.toLowerCase())) {
      setErr(t.err_alreadyAdded(app.name));
      return;
    }

    // Intelligently predict and expand domains in background using app name & exe name
    let generatedDomains: string[] = [];
    try {
      const fromExe = await invoke<string[]>("expand_url", { raw: app.exe_name });
      const fromName = await invoke<string[]>("expand_url", { raw: app.name });
      generatedDomains = Array.from(new Set([...(app.domains || []), ...fromExe, ...fromName])).sort();
    } catch {
      generatedDomains = app.domains || [];
    }

    if (generatedDomains.length === 0) {
      const fallback = app.name.toLowerCase().replace(/[^a-z0-9]/g, "") + ".com";
      generatedDomains = [fallback];
    }

    const newPlatform: CustomPlatform = {
      id,
      name: app.name,
      category: app.category as "app" | "game",
      icon: app.icon || (app.category === "game" ? "🎮" : "📱"),
      exe_name: app.exe_name,
      exe_path: app.exe_path || undefined,
      domains: generatedDomains,
    };

    const nextExes = [...(cfg.custom_exes || [])];
    if (!nextExes.some((x) => x.toLowerCase() === app.exe_name.toLowerCase())) {
      nextExes.push(app.exe_name);
    }

    patch({
      custom_platforms: [...(cfg.custom_platforms || []), newPlatform],
      custom_exes: nextExes,
      enabled_apps: [...(cfg.enabled_apps || []), id],
    });

    setShowAddModal(false);
    setSuccessMsg(t.suc_addedWithDomains(app.name, generatedDomains.length));
  }

  async function handlePickExeFile() {
    try {
      const pickedPath = await invoke<string | null>("pick_exe_file");
      if (!pickedPath) return;

      const normalized = pickedPath.replace(/\\/g, "/");
      const exeName = normalized.split("/").pop() || "app.exe";
      const baseName = exeName.replace(/\.exe$/i, "");

      const isGame = /game|play|steam|riot|valve|epic|craft|sim|strike|auto|combat|racing/i.test(pickedPath);
      const category: "app" | "game" = isGame ? "game" : "app";
      const icon = isGame ? "🎮" : "📱";
      const id = "custom_" + exeName.toLowerCase().replace(/[^a-z0-9]/g, "_");

      if (cfg?.custom_platforms.some((p) => p.id === id || p.exe_name?.toLowerCase() === exeName.toLowerCase())) {
        setErr(t.err_alreadyAdded(baseName));
        return;
      }

      // Intelligently predict and expand domains in background
      let generatedDomains: string[] = [];
      try {
        const fromExe = await invoke<string[]>("expand_url", { raw: exeName });
        const fromBase = await invoke<string[]>("expand_url", { raw: baseName });
        generatedDomains = Array.from(new Set([...fromExe, ...fromBase])).sort();
      } catch {
        generatedDomains = [];
      }
      if (generatedDomains.length === 0) {
        generatedDomains = [baseName.toLowerCase().replace(/[^a-z0-9]/g, "") + ".com"];
      }

      const newPlatform: CustomPlatform = {
        id,
        name: baseName,
        category,
        icon,
        exe_name: exeName,
        exe_path: pickedPath,
        domains: generatedDomains,
      };

      const nextExes = [...(cfg?.custom_exes || [])];
      if (!nextExes.some((x) => x.toLowerCase() === exeName.toLowerCase())) {
        nextExes.push(exeName);
      }

      patch({
        custom_platforms: [...(cfg?.custom_platforms || []), newPlatform],
        custom_exes: nextExes,
        enabled_apps: [...(cfg?.enabled_apps || []), id],
      });

      setShowAddModal(false);
      setSuccessMsg(t.suc_addedWithDomains(baseName, generatedDomains.length));
    } catch (e) {
      setErr(t.err_fileNotFound(String(e)));
    }
  }

  async function handleQuitApp() {
    try {
      await invoke("close_app");
    } catch {}
  }

  async function handleSaveCustomPlatform() {
    const name = addName.trim();
    const rawTarget = addUrl.trim() || name;
    if (!name && !rawTarget) {
      setErr(t.err_invalidInput);
      return;
    }

    const displayName = name || rawTarget.replace(/^https?:\/\//i, "").split(/[\/?#]/)[0];
    let finalDomains = expandedPreview;
    if (finalDomains.length === 0 && rawTarget) {
      try {
        finalDomains = await invoke<string[]>("expand_url", { raw: rawTarget });
      } catch {
        finalDomains = [];
      }
    }
    if (finalDomains.length === 0) {
      finalDomains = [displayName.toLowerCase().replace(/[^a-z0-9]/g, "") + ".com"];
    }

    const id = "custom_" + Date.now().toString(36);
    const icon = addCategory === "game" ? "🎮" : "🌐";
    const newPlatform: CustomPlatform = {
      id,
      name: displayName,
      category: addCategory,
      icon,
      domains: finalDomains,
    };

    patch({
      custom_platforms: [...(cfg?.custom_platforms || []), newPlatform],
      enabled_apps: [...(cfg?.enabled_apps || []), id],
    });

    setAddName("");
    setAddUrl("");
    setExpandedPreview([]);
    setShowAddModal(false);
    setSuccessMsg(t.suc_addedWithDomains(displayName, finalDomains.length));
  }

  async function handleRestartDiscord() {
    try {
      await invoke("restart_discord");
      setSuccessMsg(t.suc_discordRestarting);
    } catch (e) {
      setErr(t.err_discordRestartFailed(String(e)));
    }
  }



  async function handleFlushDns() {
    try {
      await invoke("flush_dns_cache");
      setSuccessMsg(t.suc_dnsFlush);
    } catch (e) {
      setErr(t.err_dnsFlushFailed(String(e)));
    }
  }

  async function handleResetAdapterDns() {
    try {
      await invoke("reset_dns");
      setSuccessMsg(t.suc_adapterReset);
    } catch (e) {
      setErr(t.err_adapterFailed(String(e)));
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
          setSuccessMsg(t.suc_updateFound(info.latest_version));
        }
      } else if (manual) {
        setSuccessMsg(t.suc_upToDate(info.current_version));
      }
    } catch {
      if (manual) setErr(t.err_updateServerUnreachable);
    }
    if (manual) setCheckingUpdates(false);
  }

  async function handleApplyUpdate(downloadUrl?: string) {
    if (updatingRef.current) return;
    const url = downloadUrl || updateInfo?.download_url;
    if (!url) return;
    lastUpdateAttemptRef.current = Date.now();
    updatingRef.current = true;
    setUpdating(true);
    setSuccessMsg(t.suc_updatingNotice);
    try {
      await invoke("apply_update", { downloadUrl: url });
    } catch (e) {
      setErr(t.err_updateFailed(String(e)));
      setUpdating(false);
      updatingRef.current = false;
    }
  }

  function removeCustomPlatform(id: string) {
    if (!cfg) return;
    patch({
      custom_platforms: cfg.custom_platforms.filter((p) => p.id !== id),
      enabled_apps: cfg.enabled_apps.filter((x) => x !== id),
    });
  }

  async function handleAddDomain() {
    const v = domainDraft.trim();
    if (!v || !cfg) return;
    setSuggestingDomain(true);
    try {
      const res = await invoke<DomainSuggestionResult>("suggest_related_domains", { domain: v });
      if (res && res.suggestions && res.suggestions.length > 1) {
        setDomainSuggestion(res);
        setSelectedSuggestions(res.suggestions);
        setShowSuggestionModal(true);
      } else {
        const toAdd = res?.base_domain || v;
        patch({
          custom_domains: Array.from(new Set([...cfg.custom_domains, toAdd])),
        });
        setDomainDraft("");
        setSuccessMsg(t.suc_domainAdded(toAdd));
      }
    } catch {
      patch({
        custom_domains: Array.from(new Set([...cfg.custom_domains, v])),
      });
      setDomainDraft("");
      setSuccessMsg(t.suc_domainAdded(v));
    }
    setSuggestingDomain(false);
  }

  function addRule() {
    handleAddDomain();
  }

  function handleConfirmSuggestions(selectedOnly: boolean = false) {
    if (!cfg || !domainSuggestion) return;
    const toAdd = selectedOnly ? [domainSuggestion.base_domain] : selectedSuggestions;
    patch({
      custom_domains: Array.from(new Set([...cfg.custom_domains, ...toAdd])),
    });
    setSuccessMsg(
      selectedOnly
        ? t.suc_domainAdded(domainSuggestion.base_domain)
        : t.suc_suggestionsAdded(domainSuggestion.display_name, toAdd.length)
    );
    setDomainDraft("");
    setShowSuggestionModal(false);
    setDomainSuggestion(null);
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
    t.defaultStrategyName;

  return (
    <div className="window-frame" dir={isRtl ? "rtl" : "ltr"}>
      {/* Title Bar for System Tray Flyout */}
      <div className="titlebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <img src={logoSvg} alt="Slipstream" className="brand-logo" />
          <div className={`status-indicator ${busy ? "busy" : on ? "active" : "inactive"}`} />
          <span className="brand-name" data-tauri-drag-region>{t.appName}</span>
          <span className="version-pill" title={t.versionPrefix(updateInfo?.current_version || "0.2.1")}>v{getDisplayVersion(updateInfo?.current_version)}</span>
          {isPortable && <span className="portable-pill" title={t.portableTitle}>{t.portableTag}</span>}
        </div>
        <div className="window-controls">
          <button
            className="win-btn minimize"
            title={t.minimizeTitle}
            onClick={() => invoke("hide_window")}
          >
            <svg viewBox="0 0 10 10">
              <path d="M1 5h8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
            </svg>
          </button>
        </div>
      </div>

      <div className="content-scroll">
        {/* Compact Modern Update Notification Banner */}
        {updateInfo?.has_update && (
          <div className="update-banner">
            <div className="update-content">
              <div className="update-header-row">
                <span className="update-badge">
                  {cfg?.auto_update ? t.autoBadge : t.newVersionBadge}
                </span>
                <span className="update-version-tag">v{updateInfo.latest_version}</span>
              </div>
              <div className="update-summary-row">
                <span className="update-compact-desc">
                  {cfg?.auto_update
                    ? (updating ? t.autoUpdatingText : t.autoBgUpdatingText)
                    : t.manualUpdateText}
                </span>
                {updateInfo.release_notes && !cfg?.auto_update && (
                  <button
                    type="button"
                    className="update-details-link-btn"
                    onClick={() => setShowReleaseNotesModal(true)}
                  >
                    {t.detailsLink}
                  </button>
                )}
              </div>
            </div>
            {!cfg?.auto_update ? (
              <button
                className="update-action-btn"
                onClick={() => handleApplyUpdate()}
                disabled={updating}
              >
                {updating ? t.downloadingText : t.updateNowBtn}
              </button>
            ) : (
              <span className="update-auto-tag">
                {updating ? t.downloadingAutoTag : t.autoTag}
              </span>
            )}
          </div>
        )}

        {/* Navigation Tabs */}
        <div className="tab-bar">
          <button
            className={`tab-item ${activeTab === "services" ? "active" : ""}`}
            onClick={() => handleTabChange("services")}
          >
            {t.tabPlatforms(cfg.enabled_apps.length)}
          </button>
          <button
            className={`tab-item ${activeTab === "settings" ? "active" : ""}`}
            onClick={() => handleTabChange("settings")}
          >
            {t.tabSettings}
          </button>
          <button
            className={`tab-item ${activeTab === "rules" ? "active" : ""}`}
            onClick={() => handleTabChange("rules")}
          >
            {t.tabCustomRules(cfg.custom_domains.length)}
          </button>
          <button
            className={`tab-item ${activeTab === "preferences" ? "active" : ""}`}
            onClick={() => handleTabChange("preferences")}
          >
            {t.tabPreferences}
          </button>
        </div>

        {/* TAB 1: SERVICES (Filtered + Search + Bulk Actions + Quick Ping) */}
        {activeTab === "services" && (
          <section className="tab-pane">
            {/* Hero Power Section - Yalnızca Platformlar sekmesinde görünür */}
            <section className="hero-card">
              <div className="power-wrapper">
                <button
                  className={`power-button ${busy ? "busy" : on ? "on" : "off"}`}
                  onClick={handlePower}
                  disabled={busy}
                  aria-label={on ? t.powerOnLabel : t.powerOffLabel}
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
                  {busy ? t.powerBusy : on ? t.bypassActive : t.protectionOff}
                </div>
                <p className="hero-subtitle">
                  {on
                    ? t.heroPitchOn
                    : t.heroPitchOff}
                </p>
              </div>

              {/* Real-time Ping Radar Badge */}
              <div className="latency-radar-bar">
                <div className="latency-badge" title={t.discordPingTitle}>
                  <span className="latency-dot discord" />
                  <span className="latency-label">{t.discordPing}</span>
                  <span className="latency-val">
                    {latency.discord !== null ? `${latency.discord}ms` : "—"}
                  </span>
                </div>
                <div className="latency-badge" title={t.dohDnsTitle}>
                  <span className="latency-dot cloudflare" />
                  <span className="latency-label">{t.dohDns}</span>
                  <span className="latency-val">
                    {latency.cloudflare !== null ? `${latency.cloudflare}ms` : "—"}
                  </span>
                </div>
                <div className="latency-badge zero-overhead" title={t.noExtraLatencyTitle}>
                  <span className="latency-dot zero" />
                  <span className="latency-label">{t.noExtraLatency}</span>
                  <span className="latency-val zero">{t.zeroLatencyVal}</span>
                </div>
              </div>
            </section>
            {/* Search + Add Platform Row */}
            <div className="search-toolbar">
              <div className="search-bar">
                <span className="search-icon">🔍</span>
                <input
                  className="search-input"
                  placeholder={t.searchPlaceholder}
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
                title={t.addPlatformTitle}
              >
                {t.addPlatformBtn}
              </button>
            </div>

            {/* Category Filter Pills */}
            <div className="category-pills">
              <button
                className={`pill-btn ${platformFilter === "all" ? "active" : ""}`}
                onClick={() => setPlatformFilter("all")}
              >
                {t.allFilter(allPlatforms.length)}
              </button>
              <button
                className={`pill-btn ${platformFilter === "apps" ? "active" : ""}`}
                onClick={() => setPlatformFilter("apps")}
              >
                {t.appsFilter(appCount)}
              </button>
              <button
                className={`pill-btn ${platformFilter === "games" ? "active" : ""}`}
                onClick={() => setPlatformFilter("games")}
              >
                {t.gamesFilter(gameCount)}
              </button>
            </div>

            {/* Quick Actions & Ready Preset Chips */}
            <div className="quick-actions-bar">
              <button
                className="quick-chip"
                onClick={selectPopular}
                title={t.presetPopularTitle}
              >
                {t.presetPopular}
              </button>
              <button
                className="quick-chip"
                onClick={() => applyPreset("gamer")}
                title={t.presetGamerTitle}
              >
                {t.presetGamer}
              </button>
              <button
                className="quick-chip"
                onClick={() => applyPreset("social")}
                title={t.presetSocialTitle}
              >
                {t.presetSocial}
              </button>
              <button
                className="quick-chip"
                onClick={() => applyPreset("media")}
                title={t.presetMediaTitle}
              >
                {t.presetMedia}
              </button>
              <button
                className="quick-chip secondary"
                onClick={() => toggleSelectAll(true)}
                title={t.selectAllTitle}
              >
                {t.selectAllBtn}
              </button>
              <button
                className="quick-chip secondary"
                onClick={() => toggleSelectAll(false)}
                title={t.clearAllTitle}
              >
                {t.clearAllBtn}
              </button>
            </div>

            {/* Platform List */}
            <div className={`app-list ${cardDensity === "compact" ? "density-compact" : ""}`}>
              {filteredPlatforms.length === 0 ? (
                <div className="empty-state">
                  <p>{t.noPlatformsFound}</p>
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
                            {a.isCustom && <span className="custom-badge">{t.customBadge}</span>}
                            {cfg.process_detection_mode && detectedProcesses.includes(a.id) && (
                              <span className="live-process-badge" title={t.liveProcessTitle}>
                                {t.liveProcessBadge}
                              </span>
                            )}
                          </div>
                          <span className="app-desc">{a.tag}</span>
                        </div>
                      </div>

                      <div className="app-card-right">

                        {a.id === "discord" && isChecked && (
                          <button
                            className="discord-restart-pill"
                            title={t.restartDiscordTitle}
                            onClick={(e) => {
                              e.stopPropagation();
                              handleRestartDiscord();
                            }}
                          >
                            {t.restartDiscordBtn}
                          </button>
                        )}
                        {a.isCustom && (
                          <button
                            className="delete-custom-btn"
                            title={t.deletePlatformBtn}
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
              <label className="field-label">{t.ispLabel}</label>
              <select
                className="select-input"
                value={cfg.isp}
                onChange={(e) => {
                  const newIsp = e.target.value;
                  const firstStrat = cat.isps.find((i) => i.id === newIsp)?.order?.[0] || "fake-split-md5sig";
                  patch({ isp: newIsp, strategy: firstStrat });
                  const ispName = cat.isps.find((i) => i.id === newIsp)?.name || newIsp;
                  setSuccessMsg(t.suc_ispChanged(ispName, firstStrat));
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
              <label className="field-label">{t.strategyLabel}</label>
              <select
                className="select-input"
                value={cfg.strategy || "fake-split-md5sig"}
                onChange={(e) => {
                  const newStrat = e.target.value;
                  patch({ strategy: newStrat });
                  const stratName = cat.strategies.find((s) => s.id === newStrat)?.name || newStrat;
                  setSuccessMsg(t.suc_strategyChanged(stratName));
                }}
              >
                {cat.strategies.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
              <span className="field-hint">
                {t.activeStrategy} <b>{currentStrategy}</b>
              </span>
            </div>

            {/* Auto-Tune Box */}
            <div className="autotune-box">
              <button
                className="primary-btn autotune-btn"
                onClick={handleAutoTune}
                disabled={busy}
              >
                {busy ? t.autoTuneBusy : t.autoTuneBtn}
              </button>
              <button
                className="text-btn"
                onClick={() => setShowLog(!showLog)}
              >
                {showLog ? t.hideLogBtn : t.showLogBtn}
              </button>

              {showLog && (
                <div className="log-container">
                  {log.length === 0 ? (
                    <p className="empty-log">{t.noTestYet}</p>
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

            {/* Güvenli Ağ & DoH Benchmark Araçları */}
            <div className="pref-section-title" style={{ marginTop: 10 }}>{t.dnsTitle}</div>
            <div className="pref-card">
              <div className="pref-row" style={{ marginBottom: 10 }}>
                <div className="pref-meta">
                  <span className="pref-name">{t.autoSafeDns}</span>
                  <span className="pref-desc">
                    {t.autoSafeDnsDesc}
                  </span>
                </div>
                <div
                  className={`switch ${cfg.manage_dns ? "checked" : ""}`}
                  onClick={() => patch({ manage_dns: !cfg.manage_dns })}
                >
                  <div className="switch-thumb" />
                </div>
              </div>

              <div className="pref-divider" />

              <div className="field-group" style={{ marginBottom: 6 }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 4 }}>
                  <label className="field-label" style={{ marginBottom: 0 }}>{t.dohProviderLabel}</label>
                  <button
                    className="tool-btn"
                    style={{ flex: "none", padding: "3px 8px", fontSize: 10 }}
                    onClick={handleBenchDoh}
                    disabled={benchmarkingDoh}
                  >
                    {benchmarkingDoh ? t.benchDnsBusy : t.benchDnsBtn}
                  </button>
                </div>
                <select
                  className="select-input"
                  value={cfg.doh_provider}
                  onChange={(e) => patch({ doh_provider: e.target.value })}
                >
                  <option value="cloudflare">Cloudflare (1.1.1.1)</option>
                  <option value="google">Google DNS (dns.google)</option>
                  <option value="quad9">Quad9 (9.9.9.9)</option>
                  <option value="adguard">AdGuard (dns.adguard-dns.com)</option>
                  <option value="yandex">Yandex DNS (77.88.8.8)</option>
                </select>
              </div>

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
                            {b.latency !== null ? `${b.latency}ms` : t.benchError}
                          </span>
                        </div>
                        {isSelected && <span className="doh-best-tag">{t.dohActiveTag}</span>}
                      </div>
                    );
                  })}
                </div>
              )}

              <div className="tool-buttons-row">
                <button className="tool-btn" onClick={handleFlushDns} title={t.flushDnsTitle}>
                  {t.flushDnsBtn}
                </button>
                <button className="tool-btn" onClick={handleResetAdapterDns} title={t.resetAdaptersTitle}>
                  {t.resetAdaptersBtn}
                </button>
                <button className="tool-btn" onClick={handleRunHealthCheck} title={t.networkDiagTitle}>
                  {t.networkDiagBtn}
                </button>
              </div>
            </div>
          </section>
        )}

        {/* TAB 3: CUSTOM RULES */}
        {activeTab === "rules" && (
          <section className="tab-pane">
            <div className="field-group">
              <label className="field-label">{t.customDomainLabel}</label>
              <div className="add-row">
                <input
                  className="text-input"
                  value={domainDraft}
                  placeholder={t.customDomainPlaceholder}
                  onChange={(e) => setDomainDraft(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleAddDomain()}
                  disabled={suggestingDomain}
                />
                <button
                  className="primary-btn add-btn"
                  onClick={handleAddDomain}
                  disabled={suggestingDomain || !domainDraft.trim()}
                >
                  {suggestingDomain ? t.addingDomain : t.addBtn}
                </button>
              </div>
              <span className="field-hint">
                {t.customDomainHint}
              </span>
            </div>

            <div className="rules-list">
              {cfg.custom_domains.length === 0 ? (
                <p className="empty-state">{t.noCustomDomains}</p>
              ) : (
                cfg.custom_domains.map((dom) => (
                  <div key={dom} className="rule-chip">
                    <span className="chip-text">{dom}</span>
                    <button
                      className="chip-remove"
                      onClick={() => removeRule(dom)}
                      title={t.removeRuleTitle}
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
            {/* Sub-tab navigation */}
            <div className="subtabs-bar">
              <button
                type="button"
                className={`subtab-btn ${prefSubTab === "genel" ? "active" : ""}`}
                onClick={() => setPrefSubTab("genel")}
              >
                {t.generalSubTab}
              </button>
              <button
                type="button"
                className={`subtab-btn ${prefSubTab === "gelismis" ? "active" : ""}`}
                onClick={() => setPrefSubTab("gelismis")}
              >
                {t.advancedSubTab}
              </button>
              <button
                type="button"
                className={`subtab-btn ${prefSubTab === "kisisellestir" ? "active" : ""}`}
                onClick={() => setPrefSubTab("kisisellestir")}
              >
                {t.personalizeSubTab}
              </button>
            </div>

            {/* SUBTAB 1: GENEL */}
            {prefSubTab === "genel" && (
              <>
                {/* Sistem ve Başlangıç */}
                <div className="pref-section-title">{t.systemSectionTitle}</div>
                <div className="pref-card">
                  <div className="pref-row">
                    <div className="pref-meta">
                      <span className="pref-name">{t.autoStartPcLabel}</span>
                      <span className="pref-desc">{t.autoStartPcDesc}</span>
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
                      <span className="pref-name">{t.autoConnectLabel}</span>
                      <span className="pref-desc">{t.autoConnectDesc}</span>
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
                      <span className="pref-name">{t.startMinimizedLabel}</span>
                      <span className="pref-desc">{t.startMinimizedDesc}</span>
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
                      <span className="pref-name">{t.hideOnBlurLabel}</span>
                      <span className="pref-desc">{t.hideOnBlurDesc}</span>
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

                {/* Canlı Oyun & Uygulama Algılayıcı */}
                <div className="pref-section-title">{t.liveDetectionSectionTitle}</div>
                <div className="pref-card">
                  <div className="pref-row">
                    <div className="pref-meta">
                      <div className="pref-badge-row">
                        <span className="pref-name">{t.processDetectionLabel}</span>
                        <span className="pill-badge pill-cyan">{t.dynamicBadge}</span>
                      </div>
                      <span className="pref-desc">
                        {t.processDetectionDesc}
                      </span>
                    </div>
                    <div
                      className={`switch ${cfg.process_detection_mode ? "checked" : ""}`}
                      onClick={() => patch({ process_detection_mode: !cfg.process_detection_mode })}
                    >
                      <div className="switch-thumb" />
                    </div>
                  </div>

                  <div className="process-status-box" style={{ marginTop: 10 }}>
                    <div className="process-status-header">
                      <span className={`process-status-dot ${cfg.process_detection_mode ? "active" : ""}`} />
                      <span className="process-status-title">{t.detectedProcessesTitle}</span>
                    </div>
                    <div className="process-status-tags">
                      {detectedProcesses.length > 0 ? (
                        detectedProcesses.map((pId) => {
                          const found = cat?.apps.find((a) => a.id === pId) || cfg.custom_platforms.find((cp) => cp.id === pId);
                          return (
                            <span key={pId} className="proc-tag active">
                              {t.processRunningTag(found ? found.name : pId)}
                            </span>
                          );
                        })
                      ) : (
                        <span className="proc-tag empty">{t.noProcessesRunningTag}</span>
                      )}
                    </div>
                  </div>
                </div>

                {/* Güncelleme Yönetimi */}
                <div className="pref-section-title">{t.updateMgmtSectionTitle}</div>
                <div className="pref-card">
                  <div className="pref-row">
                    <div className="pref-meta">
                      <span className="pref-name">{t.autoUpdateLabel}</span>
                      <span className="pref-desc">{t.autoUpdateDesc}</span>
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
                      <span className="pref-name">{t.versionCheckLabel}</span>
                      <span className="pref-desc">
                        {t.currentVersionPrefix} v{updateInfo?.current_version || "0.2.1"}
                        {updateInfo?.has_update ? (
                          <span style={{ color: "var(--cyan)", fontWeight: 600, marginLeft: 6 }}>
                            {t.newVersionLiveTag(updateInfo.latest_version)}
                          </span>
                        ) : (
                          <span style={{ color: "var(--text-muted)", marginLeft: 6 }}>
                            {t.latestVersionTag}
                          </span>
                        )}
                      </span>
                    </div>
                    <div style={{ display: "flex", gap: 6 }}>
                      <button
                        className="secondary-btn"
                        style={{ padding: "6px 10px", fontSize: 11 }}
                        onClick={() => invoke("open_url", { url: "https://github.com/BaranK35/Slipstream2/releases" })}
                        title={t.githubBtnTitle}
                      >
                        GitHub
                      </button>
                      <button
                        className="secondary-btn"
                        style={{ padding: "6px 12px", fontSize: 11 }}
                        onClick={() => handleCheckUpdates(true)}
                        disabled={checkingUpdates}
                      >
                        {checkingUpdates ? t.checkingUpdatesText : t.checkUpdatesBtn}
                      </button>
                    </div>
                  </div>
                </div>

                {/* Yasal Şartlar & Gizlilik */}
                <div className="pref-section-title">{t.eulaSectionTitle}</div>
                <div className="pref-card">
                  <div className="pref-row">
                    <div className="pref-meta">
                      <span className="pref-name">{t.eulaLabel}</span>
                      <span className="pref-desc">{t.eulaDesc}</span>
                    </div>
                    <button
                      className="secondary-btn"
                      style={{ padding: "6px 12px", fontSize: 11 }}
                      onClick={() => setShowEulaModal(true)}
                    >
                      {t.readEulaBtn}
                    </button>
                  </div>
                </div>
              </>
            )}

            {/* SUBTAB 2: GELİŞMİŞ & PERFORMANS */}
            {prefSubTab === "gelismis" && (
              <>
                {/* Performans & Oyun Modu */}
                <div className="pref-section-title">{t.gamingModeSectionTitle}</div>
                <div className="pref-card">
                  <div className="pref-row">
                    <div className="pref-meta">
                      <div className="pref-badge-row">
                        <span className="pref-name">{t.gamingModeLabel}</span>
                        <span className="pill-badge pill-emerald">{t.networkPingBadge}</span>
                      </div>
                      <span className="pref-desc">
                        {t.gamingModeDesc}
                      </span>
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
                      <div className="pref-badge-row">
                        <span className="pref-name">{t.gamePriorityBoostLabel}</span>
                        <span className="pill-badge pill-cyan">{t.extraSettingBadge}</span>
                      </div>
                      <span className="pref-desc">
                        {t.gamePriorityBoostDescPart1} {t.gamePriorityBoostDescPart2}
                        <strong>{t.gamePriorityBoostDescStrong1}</strong>
                        {t.gamePriorityBoostDescPart3} {t.gamePriorityBoostDescPart4}
                        <strong>{t.gamePriorityBoostDescStrong2}</strong>
                        {t.gamePriorityBoostDescPart5}
                      </span>
                    </div>
                    <div
                      className={`switch ${cfg.game_priority_boost ? "checked" : ""}`}
                      onClick={() => patch({ game_priority_boost: !cfg.game_priority_boost })}
                    >
                      <div className="switch-thumb" />
                    </div>
                  </div>

                  <div className="pref-divider" />

                  <div className="pref-row">
                    <div className="pref-meta">
                      <span className="pref-name">{t.autoHealingLabel}</span>
                      <span className="pref-desc">{t.autoHealingDesc}</span>
                    </div>
                    <div
                      className={`switch ${cfg.auto_healing ? "checked" : ""}`}
                      onClick={() => patch({ auto_healing: !cfg.auto_healing })}
                    >
                      <div className="switch-thumb" />
                    </div>
                  </div>
                </div>

                {/* Discord Onarım Merkezi */}
                <div className="pref-section-title">{t.discordRepairSectionTitle}</div>
                <div className="pref-card">
                  <div className="discord-diag-card">
                    <div className="diag-row">
                      <span className="diag-label">{t.discordRelay18809}</span>
                      <span className={`diag-val ${discordDiag?.relay_ok ? "ok" : "warn"}`}>
                        {discordDiag?.relay_ok ? t.discordRelayReady : t.discordRelayWaiting}
                      </span>
                    </div>
                    <div className="diag-row">
                      <span className="diag-label">{t.discordSettingsJson}</span>
                      <span className={`diag-val ${discordDiag?.patched ? "ok" : "warn"}`}>
                        {discordDiag?.patched ? t.discordSettingsPatched : t.discordSettingsStandard}
                      </span>
                    </div>
                    <div className="diag-row">
                      <span className="diag-label">{t.discordProcessStatus}</span>
                      <span className={`diag-val ${discordDiag?.running ? "ok" : "warn"}`}>
                        {discordDiag?.running ? t.discordProcessRunning : t.discordProcessStopped}
                      </span>
                    </div>
                  </div>

                  <div className="tool-buttons-row" style={{ marginTop: 6 }}>
                    <button
                      className="tool-btn"
                      onClick={handleRepairDiscord}
                      disabled={repairingDiscord}
                      title={t.discordRepairTitle}
                    >
                      {repairingDiscord ? t.discordRepairBusy : t.discordRepairBtn}
                    </button>
                  </div>
                </div>
              </>
            )}

            {/* SUBTAB 3: KİŞİSELLEŞTİRME */}
            {prefSubTab === "kisisellestir" && (
              <>
                {/* Dil Seçimi / Language Selector */}
                <div className="pref-section-title">{t.languageSectionTitle}</div>
                <div className="pref-card">
                  <div className="field-group" style={{ marginBottom: 0 }}>
                    <label className="field-label" style={{ marginBottom: 6 }}>{t.languageLabel}</label>
                    <select
                      className="select-input"
                      value={lang}
                      onChange={(e) => setLang(e.target.value as LangCode)}
                    >
                      {LANGUAGES.map((l) => (
                        <option key={l.code} value={l.code}>
                          {l.flag} {l.nativeName} ({l.name})
                        </option>
                      ))}
                    </select>
                    <span className="field-hint" style={{ marginTop: 6 }}>
                      {t.languageDesc}
                    </span>
                  </div>
                </div>

                {/* Görsel Temalar */}
                <div className="pref-section-title">{t.visualThemesSectionTitle}</div>
                <div className="pref-card">
                  <div className="field-group">
                    <label className="field-label" style={{ marginBottom: 6 }}>{t.themeLabel}</label>
                    <div className="theme-grid">
                      {THEMES.map((itemTheme) => {
                        const isActive = theme === itemTheme.id;
                        return (
                          <button
                            key={itemTheme.id}
                            type="button"
                            className={`theme-badge-btn ${isActive ? "active" : ""}`}
                            style={{
                              background: itemTheme.bg,
                              borderColor: isActive ? itemTheme.accent : `${itemTheme.accent}44`,
                              color: isActive ? "#ffffff" : "#c5cbd8",
                            }}
                            onClick={() => setTheme(itemTheme.id)}
                          >
                            <span
                              className="theme-color-dot"
                              style={{
                                background: itemTheme.dot,
                                boxShadow: isActive ? `0 0 8px ${itemTheme.dot}` : `0 0 4px ${itemTheme.dot}88`,
                              }}
                            />
                            <span className="theme-badge-name">{itemTheme.name}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                </div>

                {/* Görünüm & Deneyim Tercihleri */}
                <div className="pref-section-title">{t.appearanceSectionTitle}</div>
                <div className="pref-card">
                  <div className="pref-row">
                    <div className="pref-meta">
                      <span className="pref-name">{t.densityLabel}</span>
                      <span className="pref-desc">{t.densityDesc}</span>
                    </div>
                    <div style={{ display: "flex", gap: 6 }}>
                      <button
                        className={`secondary-btn ${cardDensity === "comfortable" ? "active" : ""}`}
                        style={{
                          padding: "5px 10px",
                          fontSize: 11,
                          borderColor: cardDensity === "comfortable" ? "var(--accent)" : "var(--border)",
                          background: cardDensity === "comfortable" ? "rgba(99, 102, 241, 0.18)" : "transparent",
                        }}
                        onClick={() => setCardDensity("comfortable")}
                      >
                        {t.densityComfortable}
                      </button>
                      <button
                        className={`secondary-btn ${cardDensity === "compact" ? "active" : ""}`}
                        style={{
                          padding: "5px 10px",
                          fontSize: 11,
                          borderColor: cardDensity === "compact" ? "var(--accent)" : "var(--border)",
                          background: cardDensity === "compact" ? "rgba(99, 102, 241, 0.18)" : "transparent",
                        }}
                        onClick={() => setCardDensity("compact")}
                      >
                        {t.densityCompact}
                      </button>
                    </div>
                  </div>

                  <div className="pref-divider" />

                  <div className="pref-row">
                    <div className="pref-meta">
                      <span className="pref-name">{t.ambientGlowLabel}</span>
                      <span className="pref-desc">{t.ambientGlowDesc}</span>
                    </div>
                    <div
                      className={`switch ${ambientGlow ? "checked" : ""}`}
                      onClick={() => setAmbientGlow(!ambientGlow)}
                    >
                      <div className="switch-thumb" />
                    </div>
                  </div>

                  <div className="pref-divider" />

                  <div className="pref-row">
                    <div className="pref-meta">
                      <span className="pref-name">{t.soundEffectsLabel}</span>
                      <span className="pref-desc">{t.soundEffectsDesc}</span>
                    </div>
                    <div
                      className={`switch ${soundEnabled ? "checked" : ""}`}
                      onClick={() => setSoundEnabled(!soundEnabled)}
                    >
                      <div className="switch-thumb" />
                    </div>
                  </div>
                </div>
              </>
            )}
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
          <div className="modal-dialog modal-wide" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3 className="modal-title">{t.addModalTitle}</h3>
              <button className="modal-close" onClick={() => setShowAddModal(false)}>
                ✕
              </button>
            </div>

            <div className="modal-body">
              {/* Mode Selector */}
              <div className="modal-segmented">
                <button
                  className={`segment-btn ${addModalTab === "scan" ? "active" : ""}`}
                  onClick={() => setAddModalTab("scan")}
                >
                  {t.scanTab}
                </button>
                <button
                  className={`segment-btn ${addModalTab === "manual" ? "active" : ""}`}
                  onClick={() => setAddModalTab("manual")}
                >
                  {t.manualTab}
                </button>
              </div>

              {addModalTab === "scan" ? (
                <div className="scanned-controls">
                  <div className="scanned-search-row">
                    <input
                      className="scanned-search-input"
                      placeholder={t.scannedSearchPlaceholder}
                      value={scannedSearch}
                      onChange={(e) => setScannedSearch(e.target.value)}
                    />
                    <button
                      className="pick-exe-btn"
                      onClick={handlePickExeFile}
                      title={t.pickExeTitle}
                    >
                      {t.pickExeBtn}
                    </button>
                    <button
                      className="refresh-scan-btn"
                      onClick={loadScannedApps}
                      title={t.refreshScanTitle}
                    >
                      {t.refreshScanBtn}
                    </button>
                  </div>

                  <div className="scanned-filter-pills">
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "all" ? "active" : ""}`}
                      onClick={() => setScannedFilter("all")}
                    >
                      {t.scannedAllFilter(scannedApps.length)}
                    </button>
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "games" ? "active" : ""}`}
                      onClick={() => setScannedFilter("games")}
                    >
                      {t.scannedGamesFilter(scannedApps.filter((a) => a.category === "game").length)}
                    </button>
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "apps" ? "active" : ""}`}
                      onClick={() => setScannedFilter("apps")}
                    >
                      {t.scannedAppsFilter(scannedApps.filter((a) => a.category === "app").length)}
                    </button>
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "running" ? "active" : ""}`}
                      onClick={() => setScannedFilter("running")}
                    >
                      {t.scannedRunningFilter(runningScannedCount)}
                    </button>
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "stopped" ? "active" : ""}`}
                      onClick={() => setScannedFilter("stopped")}
                    >
                      {t.scannedStoppedFilter(stoppedScannedCount)}
                    </button>
                  </div>

                  <div className="scanned-list">
                    {loadingScannedApps ? (
                      <div className="scanned-loading">
                        <span>{t.scanningApps}</span>
                      </div>
                    ) : scannedApps.length === 0 ? (
                      <div className="scanned-initial-prompt" style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", padding: "48px 16px", textAlign: "center", gap: "14px" }}>
                        <div style={{ fontSize: "2.2rem" }}>🔍</div>
                        <div style={{ fontSize: "13px", color: "var(--text-muted)", maxWidth: "320px", lineHeight: "1.5" }}>
                          Sisteminizde yüklü olan oyun ve uygulamaları listelemek için aşağıdaki butona basın.
                        </div>
                        <button
                          className="refresh-scan-btn"
                          style={{ width: "auto", padding: "8px 24px", height: "36px", fontSize: "13px", display: "inline-flex", gap: "8px", alignItems: "center" }}
                          onClick={loadScannedApps}
                        >
                          ⚡ Yüklü Uygulamaları Tara
                        </button>
                      </div>
                    ) : filteredScannedApps.length === 0 ? (
                      <div className="scanned-empty">
                        <span>{t.noAppsFound}</span>
                      </div>
                    ) : (
                      filteredScannedApps.map((item) => {
                        const isAlreadyAdded =
                          cfg.custom_platforms.some(
                            (cp) =>
                              cp.exe_name?.toLowerCase() === item.exe_name.toLowerCase() ||
                              cp.id === item.id
                          ) ||
                          cat.apps.some((ca) =>
                            ca.domains?.some((d) => item.domains.includes(d))
                          );

                        return (
                          <div key={item.id} className="scanned-app-item">
                            <div className="scanned-app-left">
                              <span className="scanned-app-icon">{item.icon}</span>
                              <div className="scanned-app-info">
                                <span className="scanned-app-name" title={item.name}>
                                    {item.name}
                                </span>
                                <span className="scanned-app-exe">{item.exe_name}</span>
                              </div>
                            </div>

                            <div className="scanned-app-right">
                              {item.is_running ? (
                                <span className="scanned-status running">{t.runningStatus}</span>
                              ) : (
                                <span className="scanned-status stopped">{t.stoppedStatus}</span>
                              )}
                              {isAlreadyAdded ? (
                                <span className="scanned-added-badge">{t.alreadyAddedBadge}</span>
                              ) : (
                                <button
                                  className="scanned-add-btn"
                                  onClick={() => handleAddScannedApp(item)}
                                  title={t.addToListTitle(item.name)}
                                >
                                  {t.addToListBtn}
                                </button>
                              )}
                            </div>
                          </div>
                        );
                      })
                    )}
                  </div>
                </div>
              ) : (
                <div className="manual-web-content">
                  <div className="field-group">
                    <label className="field-label">{t.categoryLabel}</label>
                    <div className="modal-segmented">
                      <button
                        className={`segment-btn ${addCategory === "app" ? "active" : ""}`}
                        onClick={() => setAddCategory("app")}
                      >
                        {t.appCategoryBtn}
                      </button>
                      <button
                        className={`segment-btn ${addCategory === "game" ? "active" : ""}`}
                        onClick={() => setAddCategory("game")}
                      >
                        {t.gameCategoryBtn}
                      </button>
                    </div>
                  </div>

                  <div className="field-group">
                    <label className="field-label">{t.platformNameLabel}</label>
                    <input
                      className="text-input"
                      placeholder={t.platformNamePlaceholder}
                      value={addName}
                      onChange={(e) => setAddName(e.target.value)}
                    />
                  </div>

                  <div className="field-group">
                    <label className="field-label">{t.websiteLabel}</label>
                    <input
                      className="text-input"
                      placeholder={t.websitePlaceholder}
                      value={addUrl}
                      onChange={(e) => setAddUrl(e.target.value)}
                    />
                    <span className="field-hint">
                      {t.websiteHint}
                    </span>
                  </div>

                  {expandedPreview.length > 0 && (
                    <div className="expanded-preview-box">
                      <span className="preview-label">
                        {t.detectedDomainsLabel(expandedPreview.length)}
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
              )}
            </div>

            <div className="modal-footer">
              <button className="secondary-btn" onClick={() => setShowAddModal(false)}>
                {t.closeBtn}
              </button>
              {addModalTab === "manual" && (
                <button
                  className="primary-btn"
                  onClick={handleSaveCustomPlatform}
                  disabled={!addName.trim() && !addUrl.trim() && expandedPreview.length === 0}
                >
                  {t.saveActivateBtn}
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Health Check Modal */}
      {showHealthModal && (
        <div className="modal-backdrop" onClick={() => setShowHealthModal(false)}>
          <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3 className="modal-title">{t.healthTitle}</h3>
              <button className="modal-close" onClick={() => setShowHealthModal(false)}>
                ✕
              </button>
            </div>

            <div className="modal-body">
              {checkingHealth ? (
                <div className="boot-container" style={{ height: 160 }}>
                  <div className="spinner" />
                  <p>{t.healthTesting}</p>
                </div>
              ) : networkHealth ? (
                <div className="health-list">
                  <div className="health-item">
                    <div className="health-item-left">
                      <div>
                        <div className="health-item-title">{t.windivertLabel}</div>
                        <div className="health-item-desc">{t.windivertDesc}</div>
                      </div>
                    </div>
                    <span className={`health-status-badge ${networkHealth.windivert_driver_ok ? "ok" : "fail"}`}>
                      {networkHealth.windivert_driver_ok ? t.healthActive : t.healthError}
                    </span>
                  </div>

                  <div className="health-item">
                    <div className="health-item-left">
                      <div>
                        <div className="health-item-title">{t.proxyLabel}</div>
                        <div className="health-item-desc">{t.proxyDesc}</div>
                      </div>
                    </div>
                    <span className={`health-status-badge ${networkHealth.proxy_clean ? "ok" : "fail"}`}>
                      {networkHealth.proxy_clean ? t.healthClean : t.healthProxyDetected}
                    </span>
                  </div>

                  <div className="health-item">
                    <div className="health-item-left">
                      <div>
                        <div className="health-item-title">{t.dohLabel}</div>
                        <div className="health-item-desc">{t.dohDesc(networkHealth.active_doh_provider)}</div>
                      </div>
                    </div>
                    <span className={`health-status-badge ${networkHealth.doh_ok ? "ok" : "fail"}`}>
                      {networkHealth.doh_ok ? t.healthOk : t.healthFail}
                    </span>
                  </div>

                  <div className="health-item">
                    <div className="health-item-left">
                      <div>
                        <div className="health-item-title">{t.discordRelayLabel}</div>
                        <div className="health-item-desc">{t.discordRelayDesc}</div>
                      </div>
                    </div>
                    <span className={`health-status-badge ${networkHealth.discord_relay_ok ? "ok" : "fail"}`}>
                      {networkHealth.discord_relay_ok ? t.healthListening : t.healthClosed}
                    </span>
                  </div>
                </div>
              ) : (
                <p className="empty-state">{t.noHealthData}</p>
              )}
            </div>

            <div className="modal-footer">
              <button className="primary-btn" onClick={() => setShowHealthModal(false)}>
                {t.okBtn}
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
              <span className="modal-title">{t.eulaTitle}</span>
              <button className="modal-close-btn" onClick={() => setShowEulaModal(false)}>✕</button>
            </div>
            <div className="modal-body">
              <div className="eula-modal-body">
                <h4>{t.eulaSection1}</h4>
                <p>{t.eulaSection1Text}</p>

                <h4>{t.eulaSection2}</h4>
                <p>{t.eulaSection2Text}</p>

                <h4>{t.eulaSection3}</h4>
                <p>{t.eulaSection3Text}</p>

                <h4>{t.eulaSection4}</h4>
                <p>{t.eulaSection4Text}</p>
              </div>
            </div>
            <div className="modal-footer">
              <button className="primary-btn" onClick={() => setShowEulaModal(false)}>
                {t.agreeBtn}
              </button>
            </div>
          </div>
        </div>
      )}



      {/* Domain Suggestion & CDN Discovery Modal */}
      {showSuggestionModal && domainSuggestion && (
        <div className="modal-backdrop" onClick={() => setShowSuggestionModal(false)}>
          <div className="modal-dialog suggestion-modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <div className="modal-title-with-badge">
                <span className="suggestion-sparkle">✨</span>
                <h3 className="modal-title">{t.suggestionTitle}</h3>
              </div>
              <button className="modal-close" onClick={() => setShowSuggestionModal(false)}>
                ✕
              </button>
            </div>

            <div className="modal-body">
              <div className="suggestion-intro-card">
                <div className="suggestion-target">
                  <span className="target-label">{t.targetDomainLabel}</span>
                  <span className="target-domain">{domainSuggestion.base_domain}</span>
                </div>
                <p className="suggestion-explanation">
                  {domainSuggestion.explanation}
                </p>
              </div>

              <div className="suggestion-list-header">
                <span className="suggestion-list-title">
                  {t.detectedServers(selectedSuggestions.length, domainSuggestion.suggestions.length)}
                </span>
                <div className="suggestion-select-actions">
                  <button
                    className="chip-action-btn"
                    onClick={() => setSelectedSuggestions(domainSuggestion.suggestions)}
                  >
                    {t.selectAllSuggestions}
                  </button>
                  <button
                    className="chip-action-btn"
                    onClick={() => setSelectedSuggestions([])}
                  >
                    {t.clearSuggestions}
                  </button>
                </div>
              </div>

              <div className="suggestion-scroll-list">
                {domainSuggestion.suggestions.map((dom) => {
                  const isChecked = selectedSuggestions.includes(dom);
                  return (
                    <label key={dom} className={`suggestion-row ${isChecked ? "checked" : ""}`}>
                      <input
                        type="checkbox"
                        checked={isChecked}
                        onChange={() => {
                          if (isChecked) {
                            setSelectedSuggestions(selectedSuggestions.filter((s) => s !== dom));
                          } else {
                            setSelectedSuggestions([...selectedSuggestions, dom]);
                          }
                        }}
                      />
                      <span className="suggestion-domain-name">{dom}</span>
                      {dom === domainSuggestion.base_domain && (
                        <span className="domain-primary-tag">{t.primaryDomainTag}</span>
                      )}
                    </label>
                  );
                })}
              </div>
            </div>

            <div className="modal-footer suggestion-footer">
              <button
                className="secondary-btn"
                onClick={() => handleConfirmSuggestions(true)}
                title={t.addOnlyMain(domainSuggestion.base_domain)}
              >
                {t.addOnlyMain(domainSuggestion.base_domain)}
              </button>
              <button
                className="primary-btn"
                onClick={() => handleConfirmSuggestions(false)}
                disabled={selectedSuggestions.length === 0}
              >
                {selectedSuggestions.length > 1
                  ? t.addSelectedBtn(selectedSuggestions.length)
                  : t.addSelectedSingle}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Release Notes Modal */}
      {showReleaseNotesModal && updateInfo && (
        <div className="modal-backdrop" onClick={() => setShowReleaseNotesModal(false)}>
          <div className="modal-dialog" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 440 }}>
            <div className="modal-header">
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span style={{ fontSize: 16 }}>🚀</span>
                <h3 className="modal-title">{t.releaseNotesTitle(updateInfo.latest_version)}</h3>
              </div>
              <button className="modal-close" onClick={() => setShowReleaseNotesModal(false)}>✕</button>
            </div>
            <div className="modal-body" style={{ maxHeight: "320px", overflowY: "auto", padding: "14px 16px" }}>
              <div className="release-notes-list">
                {updateInfo.release_notes.split("\n").map((line, idx) => {
                  const trimmed = line.trim();
                  if (!trimmed) return <div key={idx} style={{ height: 6 }} />;
                  if (trimmed.startsWith("-") || trimmed.startsWith("•") || trimmed.startsWith("*")) {
                    return (
                      <div key={idx} className="release-note-item" style={{ display: "flex", gap: 8, marginBottom: 8, fontSize: 12.5, lineHeight: 1.45, color: "var(--text-main)" }}>
                        <span style={{ color: "#10b981", fontWeight: "bold" }}>•</span>
                        <span>{trimmed.replace(/^[-•*]\s*/, "")}</span>
                      </div>
                    );
                  }
                  if (trimmed.startsWith("###") || trimmed.startsWith("##") || trimmed.endsWith(":")) {
                    return (
                      <div key={idx} style={{ fontWeight: 700, fontSize: 13, color: "var(--primary-color, #10b981)", marginTop: idx > 0 ? 10 : 0, marginBottom: 6 }}>
                        {trimmed.replace(/^#+\s*/, "")}
                      </div>
                    );
                  }
                  return (
                    <p key={idx} style={{ fontSize: 12.5, color: "var(--text-muted)", marginBottom: 8, lineHeight: 1.4 }}>
                      {trimmed}
                    </p>
                  );
                })}
              </div>
            </div>
            <div className="modal-footer" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", paddingTop: 10 }}>
              <button className="secondary-btn" onClick={() => setShowReleaseNotesModal(false)}>
                {t.closeBtn2}
              </button>
              <button
                className="primary-btn"
                onClick={() => {
                  setShowReleaseNotesModal(false);
                  handleApplyUpdate();
                }}
                disabled={updating}
                style={{ padding: "8px 16px" }}
              >
                {updating ? t.downloadingText : t.updateNowBtn2}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Flyout Footer */}
      <footer className="footer">
        <span className="footer-status">
          {on ? t.protectionActive : t.serviceOff}
        </span>
        <button
          className="quit-btn"
          title={t.quitAppTitle}
          onClick={handleQuitApp}
        >
          {t.quitAppBtn}
        </button>
      </footer>
    </div>
  );
}
