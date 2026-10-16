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

let sharedAudioCtx: AudioContext | null = null;
const preloadedBuffers: Record<string, AudioBuffer> = {};

function initAudioSystem(): AudioContext | null {
  try {
    const AudioContextClass =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    if (!AudioContextClass) return null;

    if (!sharedAudioCtx) {
      sharedAudioCtx = new AudioContextClass({ latencyHint: "interactive" });

      const sr = sharedAudioCtx.sampleRate;

      // 1. Click buffer (subtle 16ms high-frequency tactile pop)
      const clickSamples = Math.floor(sr * 0.018);
      const clickBuf = sharedAudioCtx.createBuffer(1, clickSamples, sr);
      const clickData = clickBuf.getChannelData(0);
      for (let i = 0; i < clickSamples; i++) {
        const t = i / sr;
        clickData[i] = Math.sin(2 * Math.PI * 1600 * t) * Math.exp(-t * 260) * 0.08;
      }
      preloadedBuffers["click"] = clickBuf;

      // 2. Toggle buffer (28ms mechanical switch click)
      const toggleSamples = Math.floor(sr * 0.028);
      const toggleBuf = sharedAudioCtx.createBuffer(1, toggleSamples, sr);
      const toggleData = toggleBuf.getChannelData(0);
      for (let i = 0; i < toggleSamples; i++) {
        const t = i / sr;
        toggleData[i] = Math.sin(2 * Math.PI * 1050 * t) * Math.exp(-t * 140) * 0.1;
      }
      preloadedBuffers["toggle"] = toggleBuf;

      // 3. Power ON (smooth, warm upward dual chime)
      const onSamples = Math.floor(sr * 0.18);
      const onBuf = sharedAudioCtx.createBuffer(1, onSamples, sr);
      const onData = onBuf.getChannelData(0);
      for (let i = 0; i < onSamples; i++) {
        const t = i / sr;
        const freq = t < 0.08 ? 523.25 : 880.0;
        const env = t < 0.14 ? 0.08 : 0.08 * (1 - (t - 0.14) / 0.04);
        onData[i] = Math.sin(2 * Math.PI * freq * t) * env;
      }
      preloadedBuffers["on"] = onBuf;

      // 4. Power OFF (smooth downward dual chime)
      const offSamples = Math.floor(sr * 0.16);
      const offBuf = sharedAudioCtx.createBuffer(1, offSamples, sr);
      const offData = offBuf.getChannelData(0);
      for (let i = 0; i < offSamples; i++) {
        const t = i / sr;
        const freq = t < 0.07 ? 880.0 : 440.0;
        const env = (1 - t / 0.16) * 0.07;
        offData[i] = Math.sin(2 * Math.PI * freq * t) * env;
      }
      preloadedBuffers["off"] = offBuf;

      // Keep-alive silent node: keeps Windows WASAPI audio device permanently warm
      try {
        const silentOsc = sharedAudioCtx.createOscillator();
        const silentGain = sharedAudioCtx.createGain();
        silentGain.gain.value = 0.0;
        silentOsc.connect(silentGain);
        silentGain.connect(sharedAudioCtx.destination);
        silentOsc.start();
      } catch {}
    }

    if (sharedAudioCtx.state === "suspended") {
      sharedAudioCtx.resume().catch(() => {});
    }

    return sharedAudioCtx;
  } catch {
    return null;
  }
}

function playTone(type: "on" | "off" | "click" | "toggle") {
  try {
    const ctx = initAudioSystem();
    if (!ctx) return;

    if (ctx.state === "suspended") {
      ctx.resume().catch(() => {});
    }

    const buf = preloadedBuffers[type];
    if (buf) {
      const src = ctx.createBufferSource();
      src.buffer = buf;
      src.connect(ctx.destination);
      src.start(0);
      return;
    }
  } catch {}
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

  // Appearance, Themes & Personalization
  type ThemeId = "cyber" | "erkek" | "amoled" | "emerald" | "crimson" | "violet" | "frost";

  const THEMES: { id: ThemeId; name: string; accent: string; bg: string; dot: string }[] = [
    { id: "cyber", name: "Siber Gece", accent: "#6366f1", bg: "#0f1424", dot: "#6366f1" },
    { id: "erkek", name: "🌸 Erkek Rengi (Pembe)", accent: "#f43f85", bg: "#170713", dot: "#f43f85" },
    { id: "amoled", name: "OLED Siyah", accent: "#3b82f6", bg: "#050505", dot: "#3b82f6" },
    { id: "emerald", name: "Zümrüt Oyuncu", accent: "#10b981", bg: "#06150e", dot: "#10b981" },
    { id: "crimson", name: "Kan Kırmızı", accent: "#f43f5e", bg: "#16070a", dot: "#f43f5e" },
    { id: "violet", name: "Ametist Gece", accent: "#a855f7", bg: "#12091f", dot: "#a855f7" },
    { id: "frost", name: "Buzul Arktik", accent: "#06b6d4", bg: "#07131d", dot: "#06b6d4" },
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
    const v = (rawVer || "0.1.6").replace(/^v/, "");
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

  // Pre-warm Web Audio immediately on first user interaction so latency is 0ms
  useEffect(() => {
    const warmAudio = () => {
      initAudioSystem();
    };
    window.addEventListener("pointerdown", warmAudio, { passive: true });
    window.addEventListener("keydown", warmAudio, { passive: true });
    return () => {
      window.removeEventListener("pointerdown", warmAudio);
      window.removeEventListener("keydown", warmAudio);
    };
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
      setSuccessMsg("DNS önbelleği başarıyla temizlendi.");
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
                const now = Date.now();
                if (now - lastUpdateAttemptRef.current > 15 * 60 * 1000) {
                  handleApplyUpdate(info.download_url);
                }
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
      unlistenDetected.then((f) => f());
      unlistenAutoBypass.then((f) => f());
      unlistenStrategy.then((f) => f());
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

  useEffect(() => {
    if (showAddModal) {
      loadScannedApps();
    }
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
        tag: cp.exe_name ? cp.exe_name : `${cp.domains.length} alan adı & web sitesi`,
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

  if (!cat || !cfg) {
    return (
      <div className="window-frame">
        <div className="titlebar" data-tauri-drag-region>
          <div className="brand" data-tauri-drag-region>
            <img src={logoSvg} alt="Slipstream" className="brand-logo" />
            <div className="status-indicator busy" />
            <span className="brand-name" data-tauri-drag-region>Slipstream</span>
            <span className="version-pill" title={`Sürüm: v${updateInfo?.current_version || "0.1.6"}`}>v{getDisplayVersion(updateInfo?.current_version)}</span>
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
    patch({ enabled_apps: ["discord", "youtube", "roblox"] });
    setSuccessMsg("Popüler platformlar (Discord, YouTube, Roblox) seçildi.");
  };

  const applyPreset = (preset: "gamer" | "social" | "media" | "all") => {
    if (soundEnabled) playTone("click");
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
        setSuccessMsg(`En iyi strateji bulundu ve aktif edildi: ${best}`);
      } else {
        setErr("Hiçbir strateji hedefe ulaşamadı. Log detaylarını inceleyebilirsiniz.");
      }
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  }

  function handleAddScannedApp(app: ScannedApp) {
    if (!cfg) return;

    const id = "custom_" + app.exe_name.toLowerCase().replace(/[^a-z0-9]/g, "_");

    if (cfg.custom_platforms.some((p) => p.id === id || p.exe_name?.toLowerCase() === app.exe_name.toLowerCase())) {
      setErr(`"${app.name}" zaten platform listenizde ekli.`);
      return;
    }

    const newPlatform: CustomPlatform = {
      id,
      name: app.name,
      category: app.category as "app" | "game",
      icon: app.icon || (app.category === "game" ? "🎮" : "📱"),
      exe_name: app.exe_name,
      exe_path: app.exe_path || undefined,
      domains: app.domains && app.domains.length > 0 ? app.domains : [app.name.toLowerCase().replace(/[^a-z0-9]/g, "") + ".com"],
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
    setSuccessMsg(`"${app.name}" (${app.exe_name}) başarıyla eklendi ve tüm bağlantıları korumaya alındı!`);
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
        setErr(`"${baseName}" zaten platform listenizde ekli.`);
        return;
      }

      const newPlatform: CustomPlatform = {
        id,
        name: baseName,
        category,
        icon,
        exe_name: exeName,
        exe_path: pickedPath,
        domains: [baseName.toLowerCase().replace(/[^a-z0-9]/g, "") + ".com"],
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
      setSuccessMsg(`"${baseName}" (${exeName}) başarıyla eklendi ve korumaya alındı!`);
    } catch (e) {
      setErr("Dosya seçilemedi: " + String(e));
    }
  }

  async function handleQuitApp() {
    try {
      await invoke("close_app");
    } catch {}
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
    lastUpdateAttemptRef.current = Date.now();
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
        setSuccessMsg(`"${toAdd}" özel alan adı eklendi.`);
      }
    } catch {
      patch({
        custom_domains: Array.from(new Set([...cfg.custom_domains, v])),
      });
      setDomainDraft("");
      setSuccessMsg(`"${v}" özel alan adı eklendi.`);
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
        ? `"${domainSuggestion.base_domain}" eklendi.`
        : `"${domainSuggestion.display_name}" için ${toAdd.length} adet ilişkili alan adı ve CDN başarıyla eklendi!`
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
    "Otomatik (Fake + Split2 md5sig)";

  return (
    <div className="window-frame">
      {/* Title Bar for System Tray Flyout */}
      <div className="titlebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <img src={logoSvg} alt="Slipstream" className="brand-logo" />
          <div className={`status-indicator ${busy ? "busy" : on ? "active" : "inactive"}`} />
          <span className="brand-name" data-tauri-drag-region>Slipstream</span>
          <span className="version-pill" title={`Sürüm: v${updateInfo?.current_version || "0.1.6"}`}>v{getDisplayVersion(updateInfo?.current_version)}</span>
          {isPortable && <span className="portable-pill" title="Taşınabilir Tek Dosya Sürümü">Portable</span>}
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
        {/* Compact Modern Update Notification Banner */}
        {updateInfo?.has_update && (
          <div className="update-banner">
            <div className="update-content">
              <div className="update-header-row">
                <span className="update-badge">
                  {cfg?.auto_update ? "⚡ OTOMATİK" : "YENİ SÜRÜM"}
                </span>
                <span className="update-version-tag">v{updateInfo.latest_version}</span>
              </div>
              <div className="update-summary-row">
                <span className="update-compact-desc">
                  {cfg?.auto_update
                    ? (updating ? "İndiriliyor ve kuruluyor..." : "Arka planda otomatik güncelleniyor.")
                    : "Yeni performans ve altyapı yaması hazır."}
                </span>
                {updateInfo.release_notes && !cfg?.auto_update && (
                  <button
                    type="button"
                    className="update-details-link-btn"
                    onClick={() => setShowReleaseNotesModal(true)}
                  >
                    Detaylar
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
                {updating ? "İndiriliyor..." : "Şimdi Güncelle"}
              </button>
            ) : (
              <span className="update-auto-tag">
                {updating ? "⏳ İndiriliyor..." : "⚡ Otomatik"}
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
            Platformlar ({cfg.enabled_apps.length})
          </button>
          <button
            className={`tab-item ${activeTab === "settings" ? "active" : ""}`}
            onClick={() => handleTabChange("settings")}
          >
            İSS & Strateji
          </button>
          <button
            className={`tab-item ${activeTab === "rules" ? "active" : ""}`}
            onClick={() => handleTabChange("rules")}
          >
            Özel ({cfg.custom_domains.length})
          </button>
          <button
            className={`tab-item ${activeTab === "preferences" ? "active" : ""}`}
            onClick={() => handleTabChange("preferences")}
          >
            ⚙️ Ayarlar
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
            {/* Search + Add Platform Row */}
            <div className="search-toolbar">
              <div className="search-bar">
                <span className="search-icon">🔍</span>
                <input
                  className="search-input"
                  placeholder="Platform veya oyun ara..."
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
            <div className={`app-list ${cardDensity === "compact" ? "density-compact" : ""}`}>
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
                            {cfg.process_detection_mode && detectedProcesses.includes(a.id) && (
                              <span className="live-process-badge" title="Çalışan süreç arka planda algılandı">
                                ⚡ Canlı Süreç
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

            {/* Güvenli Ağ & DoH Benchmark Araçları */}
            <div className="pref-section-title" style={{ marginTop: 10 }}>🌐 Güvenli Ağ & DNS (DoH) Araçları</div>
            <div className="pref-card">
              <div className="pref-row" style={{ marginBottom: 10 }}>
                <div className="pref-meta">
                  <span className="pref-name">Otomatik Güvenli DNS Yönetimi</span>
                  <span className="pref-desc">
                    Discord ses kanalları ve engelli dinamik sunucular için ağ kartını otomatik olarak temiz DNS'e yönlendirir (Kapatıldığında eski haline döner).
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
                  <option value="yandex">Yandex DNS (77.88.8.8 - Hızlı ve Kararlı)</option>
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
                  placeholder="örn: pastebin.com veya roblox.com"
                  onChange={(e) => setDomainDraft(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleAddDomain()}
                  disabled={suggestingDomain}
                />
                <button
                  className="primary-btn add-btn"
                  onClick={handleAddDomain}
                  disabled={suggestingDomain || !domainDraft.trim()}
                >
                  {suggestingDomain ? "Taranıyor..." : "Ekle"}
                </button>
              </div>
              <span className="field-hint">
                ✨ Girdiğiniz alan adının CDN, API ve bağlı sunucuları otomatik algılanır ve toplu ekleme için önerilir.
              </span>
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
            {/* Sub-tab navigation */}
            <div className="subtabs-bar">
              <button
                type="button"
                className={`subtab-btn ${prefSubTab === "genel" ? "active" : ""}`}
                onClick={() => setPrefSubTab("genel")}
              >
                💻 Genel
              </button>
              <button
                type="button"
                className={`subtab-btn ${prefSubTab === "gelismis" ? "active" : ""}`}
                onClick={() => setPrefSubTab("gelismis")}
              >
                ⚡ Gelişmiş & Performans
              </button>
              <button
                type="button"
                className={`subtab-btn ${prefSubTab === "kisisellestir" ? "active" : ""}`}
                onClick={() => setPrefSubTab("kisisellestir")}
              >
                🎨 Kişiselleştirme
              </button>
            </div>

            {/* SUBTAB 1: GENEL */}
            {prefSubTab === "genel" && (
              <>
                {/* Sistem ve Başlangıç */}
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

                {/* Canlı Oyun & Uygulama Algılayıcı */}
                <div className="pref-section-title">⚡ Canlı Oyun & Süreç Tanıma</div>
                <div className="pref-card">
                  <div className="pref-row">
                    <div className="pref-meta">
                      <div className="pref-badge-row">
                        <span className="pref-name">Otomatik Süreç & Oyun Algılama</span>
                        <span className="pill-badge pill-cyan">Dinamik</span>
                      </div>
                      <span className="pref-desc">
                        Bilgisayarınızda Discord, Roblox, Steam, Spotify, Minecraft, Riot Games veya listenize eklediğiniz özel oyunlar açıldığı an arka planda sıfır gecikmeyle tespit eder, listenin en başına taşır ve koruma kurallarını otomatik devreye sokar.
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
                      <span className="process-status-title">Canlı Algılanan Aktif Süreçler:</span>
                    </div>
                    <div className="process-status-tags">
                      {detectedProcesses.length > 0 ? (
                        detectedProcesses.map((pId) => {
                          const found = cat?.apps.find((a) => a.id === pId) || cfg.custom_platforms.find((cp) => cp.id === pId);
                          return (
                            <span key={pId} className="proc-tag active">
                              🟢 {found ? found.name : pId} (Çalışıyor)
                            </span>
                          );
                        })
                      ) : (
                        <span className="proc-tag empty">⚪ Şu an çalışan desteklenen oyun/uygulama süreci bulunmuyor</span>
                      )}
                    </div>
                  </div>
                </div>

                {/* Güncelleme Yönetimi */}
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
                        Mevcut: v{updateInfo?.current_version || "0.1.6"}
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
                        onClick={() => invoke("open_url", { url: "https://github.com/BaranK35/Slipstream2/releases" })}
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

                {/* Yasal Şartlar & Gizlilik */}
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
              </>
            )}

            {/* SUBTAB 2: GELİŞMİŞ & PERFORMANS */}
            {prefSubTab === "gelismis" && (
              <>
                {/* Performans & Oyun Modu */}
                <div className="pref-section-title">🎮 Performans & Oyun Modu</div>
                <div className="pref-card">
                  <div className="pref-row">
                    <div className="pref-meta">
                      <div className="pref-badge-row">
                        <span className="pref-name">Düşük Gecikmeli Oyun & Ses Modu (TCP NoDelay & Ağ Kısıtlama İptali)</span>
                        <span className="pill-badge pill-emerald">Ağ & Ping</span>
                      </div>
                      <span className="pref-desc">
                        Nagle algoritmasını devre dışı bırakır (TCPNoDelay / TcpAckFrequency), Windows multimedya ağ kısıtlamasını (NetworkThrottlingIndex) tamamen kaldırır ve sistem işlem önceliğini (SystemResponsiveness = 0) tam güçle oyun ve ses paketlerine yönlendirir. Oyun içi gecikme (ping) ve paket dalgalanmalarını (jitter) en aza indirir. Kapatıldığında Windows varsayılan değerlerine temizce geri döner.
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
                        <span className="pref-name">Oyun Süreçlerine Yüksek İşlemci Önceliği</span>
                        <span className="pill-badge pill-cyan">Ekstra Ayar</span>
                      </div>
                      <span className="pref-desc">
                        Sistemde çalışan süreçlerin önceliğinin otomatik değiştirilmesini istemeyen kullanıcılar için bağımsız ekstra ayardır.
                        Açıldığında <strong>SADECE aktif oyun süreçlerini</strong> (Roblox, Valorant, CS2, Minecraft vb.) High Priority (Yüksek Öncelik) moduna alır.
                        Discord, Spotify veya tarayıcı gibi arka plan uygulamalarının önceliğine <strong>asla dokunulmaz</strong>; böylece oyununuzun işlemci gücü korunur ve mikro takılmalar (FPS drop) engellenir.
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
                      <span className="pref-name">Kendi Kendini Onaran Bekçi (Watchdog) & Otonom İyileştirme</span>
                      <span className="pref-desc">DPI motoru bir sebeple kapanırsa veya İSS engel kurallarını değiştirirse saniyeler içinde arka planda sessizce tespit edip yeniden ayağa kaldırır ve çalışan stratejiye geçer.</span>
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
              </>
            )}

            {/* SUBTAB 3: KİŞİSELLEŞTİRME */}
            {prefSubTab === "kisisellestir" && (
              <>
                {/* Görsel Temalar */}
                <div className="pref-section-title">🎨 Görsel Temalar</div>
                <div className="pref-card">
                  <div className="field-group">
                    <label className="field-label" style={{ marginBottom: 6 }}>Arayüz Teması</label>
                    <div className="theme-grid">
                      {THEMES.map((t) => {
                        const isActive = theme === t.id;
                        return (
                          <button
                            key={t.id}
                            type="button"
                            className={`theme-badge-btn ${isActive ? "active" : ""}`}
                            style={{
                              background: t.bg,
                              borderColor: isActive ? t.accent : `${t.accent}44`,
                              color: isActive ? "#ffffff" : "#c5cbd8",
                            }}
                            onClick={() => setTheme(t.id)}
                          >
                            <span
                              className="theme-color-dot"
                              style={{
                                background: t.dot,
                                boxShadow: isActive ? `0 0 8px ${t.dot}` : `0 0 4px ${t.dot}88`,
                              }}
                            />
                            <span className="theme-badge-name">{t.name}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                </div>

                {/* Görünüm & Deneyim Tercihleri */}
                <div className="pref-section-title">✨ Görünüm & Deneyim Tercihleri</div>
                <div className="pref-card">
                  <div className="pref-row">
                    <div className="pref-meta">
                      <span className="pref-name">Platform Kart Yoğunluğu</span>
                      <span className="pref-desc">Platform ve oyun kartlarının ekrandaki boyutunu ve sıkılığını ayarlar.</span>
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
                        Büyük
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
                        Kompakt
                      </button>
                    </div>
                  </div>

                  <div className="pref-divider" />

                  <div className="pref-row">
                    <div className="pref-meta">
                      <span className="pref-name">Neon Parıltı & Cam Efekti (Ambient Glow)</span>
                      <span className="pref-desc">Arayüz kartları ve başlıklarındaki modern cam ve neon parıltı efektlerini kontrol eder. Kapatıldığında daha sade mat bir görünüm sunar.</span>
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
                      <span className="pref-name">Akustik Ses Efektleri</span>
                      <span className="pref-desc">Bypass koruması açılıp kapandığında hafif futuristik akustik geri bildirim sinyali verir.</span>
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
              <h3 className="modal-title">Yeni Platform / Oyun Ekle</h3>
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
                  ⚡ Bilgisayardan Seç
                </button>
                <button
                  className={`segment-btn ${addModalTab === "manual" ? "active" : ""}`}
                  onClick={() => setAddModalTab("manual")}
                >
                  🌐 Manuel Web / Link
                </button>
              </div>

              {addModalTab === "scan" ? (
                <div className="scanned-controls">
                  <div className="scanned-search-row">
                    <input
                      className="scanned-search-input"
                      placeholder="🔍 Oyun veya uygulama adı ara..."
                      value={scannedSearch}
                      onChange={(e) => setScannedSearch(e.target.value)}
                    />
                    <button
                      className="pick-exe-btn"
                      onClick={handlePickExeFile}
                      title="Bilgisayardaki herhangi bir .exe dosyasını seçin"
                    >
                      📁 .exe Seç
                    </button>
                    <button
                      className="refresh-scan-btn"
                      onClick={loadScannedApps}
                      title="Listeyi yeniden tara"
                    >
                      🔄
                    </button>
                  </div>

                  <div className="scanned-filter-pills">
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "all" ? "active" : ""}`}
                      onClick={() => setScannedFilter("all")}
                    >
                      Tümü ({scannedApps.length})
                    </button>
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "games" ? "active" : ""}`}
                      onClick={() => setScannedFilter("games")}
                    >
                      🎮 Oyunlar ({scannedApps.filter((a) => a.category === "game").length})
                    </button>
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "apps" ? "active" : ""}`}
                      onClick={() => setScannedFilter("apps")}
                    >
                      📱 Uygulamalar ({scannedApps.filter((a) => a.category === "app").length})
                    </button>
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "running" ? "active" : ""}`}
                      onClick={() => setScannedFilter("running")}
                    >
                      🟢 Çalışıyor ({runningScannedCount})
                    </button>
                    <button
                      className={`scanned-filter-pill ${scannedFilter === "stopped" ? "active" : ""}`}
                      onClick={() => setScannedFilter("stopped")}
                    >
                      ⚪ Kapalı ({stoppedScannedCount})
                    </button>
                  </div>

                  <div className="scanned-list">
                    {loadingScannedApps ? (
                      <div className="scanned-loading">
                        <span>⏳ Oyun ve uygulamalar taranıyor...</span>
                      </div>
                    ) : filteredScannedApps.length === 0 ? (
                      <div className="scanned-empty">
                        <span>Eşleşen oyun veya uygulama bulunamadı.</span>
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
                                <span className="scanned-status running">🟢 Çalışıyor</span>
                              ) : (
                                <span className="scanned-status stopped">⚪ Kapalı</span>
                              )}
                              {isAlreadyAdded ? (
                                <span className="scanned-added-badge">✓ Eklendi</span>
                              ) : (
                                <button
                                  className="scanned-add-btn"
                                  onClick={() => handleAddScannedApp(item)}
                                  title={`${item.name} uygulamasını ekle ve korumaya al`}
                                >
                                  + Ekle
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
                <>
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
                      placeholder="örn: Kick, Wattpad, Roblox..."
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
                      Girdiğiniz adrese göre web sitesi, API ve CDN sunucuları otomatik yapılandırılır.
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
                </>
              )}
            </div>

            <div className="modal-footer">
              <button className="secondary-btn" onClick={() => setShowAddModal(false)}>
                Kapat
              </button>
              {addModalTab === "manual" && (
                <button
                  className="primary-btn"
                  onClick={handleSaveCustomPlatform}
                  disabled={!addName.trim() || expandedPreview.length === 0}
                >
                  Kaydet ve Aktif Et
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



      {/* Domain Suggestion & CDN Discovery Modal */}
      {showSuggestionModal && domainSuggestion && (
        <div className="modal-backdrop" onClick={() => setShowSuggestionModal(false)}>
          <div className="modal-dialog suggestion-modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <div className="modal-title-with-badge">
                <span className="suggestion-sparkle">✨</span>
                <h3 className="modal-title">Akıllı Alan Adı & CDN Önerisi</h3>
              </div>
              <button className="modal-close" onClick={() => setShowSuggestionModal(false)}>
                ✕
              </button>
            </div>

            <div className="modal-body">
              <div className="suggestion-intro-card">
                <div className="suggestion-target">
                  <span className="target-label">Hedef Alan Adı:</span>
                  <span className="target-domain">{domainSuggestion.base_domain}</span>
                </div>
                <p className="suggestion-explanation">
                  {domainSuggestion.explanation}
                </p>
              </div>

              <div className="suggestion-list-header">
                <span className="suggestion-list-title">
                  Tespit Edilen Bağlı Sunucular ({selectedSuggestions.length}/{domainSuggestion.suggestions.length}):
                </span>
                <div className="suggestion-select-actions">
                  <button
                    className="chip-action-btn"
                    onClick={() => setSelectedSuggestions(domainSuggestion.suggestions)}
                  >
                    Tümünü Seç
                  </button>
                  <button
                    className="chip-action-btn"
                    onClick={() => setSelectedSuggestions([])}
                  >
                    Temizle
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
                        <span className="domain-primary-tag">Ana Alan Adı</span>
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
                title="Yalnızca ana adresi ekler"
              >
                Yalnızca "{domainSuggestion.base_domain}" Ekle
              </button>
              <button
                className="primary-btn"
                onClick={() => handleConfirmSuggestions(false)}
                disabled={selectedSuggestions.length === 0}
              >
                {selectedSuggestions.length > 1
                  ? `Önerilenleri ve Alan Adını Ekle (${selectedSuggestions.length})`
                  : "Seçileni Ekle"}
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
                <h3 className="modal-title">v{updateInfo.latest_version} Sürüm Yenilikleri</h3>
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
                Kapat
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
                {updating ? "İndiriliyor..." : "Şimdi Güncelle"}
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
          onClick={handleQuitApp}
        >
          Tamamen Kapat
        </button>
      </footer>
    </div>
  );
}
