//! Desync strategies and ISP presets with dynamic remote strategy update capability.
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

#[derive(Serialize, Clone, Debug)]
pub struct Strategy {
    pub id: &'static str,
    pub name: &'static str,
    pub tls: &'static [&'static str],
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DynamicStrategy {
    pub id: String,
    pub name: String,
    pub tls: Vec<String>,
}

pub const STRATEGIES: &[Strategy] = &[
    Strategy {
        id: "fake-split-md5sig",
        name: "Fake + Split2 (md5sig - En Yüksek Uyum)",
        tls: &["--dpi-desync=fake,split2", "--dpi-desync-split-pos=2", "--dpi-desync-fooling=md5sig"],
    },
    Strategy {
        id: "fake-split-seqovl",
        name: "Fake + Split2 + SeqOvl (Sıra Çakışması - Modern Sansür Aşıcı)",
        tls: &["--dpi-desync=fake,split2", "--dpi-desync-split-pos=2", "--dpi-desync-split-seqovl=1", "--dpi-desync-fooling=md5sig"],
    },
    Strategy {
        id: "fake-split-badsum",
        name: "Fake + Split2 (badsum - Hatalı Checksum Evasion)",
        tls: &["--dpi-desync=fake,split2", "--dpi-desync-split-pos=2", "--dpi-desync-fooling=badsum"],
    },
    Strategy {
        id: "fake-disorder-badsum",
        name: "Fake + Disorder2 (badsum - Paket Sıralama)",
        tls: &["--dpi-desync=fake,disorder2", "--dpi-desync-split-pos=2", "--dpi-desync-fooling=badsum"],
    },
    Strategy {
        id: "fake-split-pos1",
        name: "Fake + Split2 (Split pos 1 + md5sig)",
        tls: &["--dpi-desync=fake,split2", "--dpi-desync-split-pos=1", "--dpi-desync-fooling=md5sig"],
    },
    Strategy {
        id: "fake-disorder-md5sig",
        name: "Fake + Disorder2 (md5sig - Sıra Değiştirme)",
        tls: &["--dpi-desync=fake,disorder2", "--dpi-desync-split-pos=2", "--dpi-desync-fooling=md5sig"],
    },
    Strategy {
        id: "fake-seqovl-badsum",
        name: "Fake + Split + SeqOvl (badsum - İleri Düzey)",
        tls: &["--dpi-desync=fake,split2", "--dpi-desync-split-pos=2", "--dpi-desync-split-seqovl=2", "--dpi-desync-fooling=badsum"],
    },
    Strategy {
        id: "fake-autottl",
        name: "Fake packet (Auto TTL + md5sig)",
        tls: &["--dpi-desync=fake", "--dpi-desync-autottl=2", "--dpi-desync-fooling=md5sig"],
    },
    Strategy {
        id: "fake-ttl4",
        name: "Fake packet (TTL 4 + md5sig)",
        tls: &["--dpi-desync=fake", "--dpi-desync-ttl=4", "--dpi-desync-fooling=md5sig"],
    },
    Strategy {
        id: "fake-ttl3",
        name: "Fake packet (TTL 3 + md5sig)",
        tls: &["--dpi-desync=fake", "--dpi-desync-ttl=3", "--dpi-desync-fooling=md5sig"],
    },
    Strategy {
        id: "multisplit-pos2",
        name: "Multi-Split (Split pos 2)",
        tls: &["--dpi-desync=multisplit", "--dpi-desync-split-pos=2"],
    },
    Strategy {
        id: "multidisorder-seqovl",
        name: "Multi-Disorder (Sıra bozma + çakışma)",
        tls: &["--dpi-desync=multidisorder", "--dpi-desync-split-pos=2", "--dpi-desync-split-seqovl=1"],
    },
    Strategy {
        id: "syndata-fake",
        name: "SYN Data + Fake (md5sig - El Sıkışma Atlatma)",
        tls: &["--dpi-desync=syndata,fake", "--dpi-desync-fooling=md5sig"],
    },
    Strategy {
        id: "ipfrag2-split",
        name: "IP Frag2 + Split (IP Katmanı Parçalama)",
        tls: &["--dpi-desync=ipfrag2,split2", "--dpi-desync-split-pos=24"],
    },
    Strategy {
        id: "fake-multisplit",
        name: "Fake + MultiSplit (md5sig - Çoklu Bölme)",
        tls: &["--dpi-desync=fake,multisplit", "--dpi-desync-split-pos=2", "--dpi-desync-fooling=md5sig"],
    },
    Strategy {
        id: "fake-ttl4-badseq",
        name: "Fake packet (TTL 4 + badseq - Alternatif)",
        tls: &["--dpi-desync=fake", "--dpi-desync-ttl=4", "--dpi-desync-fooling=badseq"],
    },
    Strategy {
        id: "fake-ttl3-badseq",
        name: "Fake packet (TTL 3 + badseq - Alternatif)",
        tls: &["--dpi-desync=fake", "--dpi-desync-ttl=3", "--dpi-desync-fooling=badseq"],
    },
];

#[derive(Serialize, Clone, Debug)]
pub struct Isp {
    pub id: &'static str,
    pub name: &'static str,
    pub order: &'static [&'static str],
}

pub const ISPS: &[Isp] = &[
    Isp {
        id: "turk-telekom",
        name: "Türk Telekom",
        order: &[
            "fake-split-md5sig",
            "fake-split-seqovl",
            "fake-split-badsum",
            "fake-split-pos1",
            "fake-disorder-md5sig",
            "fake-autottl",
            "fake-ttl4",
            "multisplit-pos2",
        ],
    },
    Isp {
        id: "turkcell",
        name: "Turkcell Superonline",
        order: &[
            "fake-split-badsum",
            "fake-split-seqovl",
            "fake-split-md5sig",
            "fake-split-pos1",
            "fake-disorder-badsum",
            "fake-ttl3",
            "fake-autottl",
            "multisplit-pos2",
        ],
    },
    Isp {
        id: "vodafone-tr",
        name: "Vodafone Türkiye",
        order: &[
            "fake-split-md5sig",
            "fake-split-seqovl",
            "fake-split-badsum",
            "fake-split-pos1",
            "fake-ttl4",
            "fake-autottl",
            "multisplit-pos2",
        ],
    },
    Isp {
        id: "turknet",
        name: "TurkNet",
        order: &[
            "fake-split-md5sig",
            "fake-split-seqovl",
            "fake-split-badsum",
            "fake-split-pos1",
            "fake-ttl4",
            "multisplit-pos2",
            "fake-autottl",
        ],
    },
    Isp {
        id: "other",
        name: "Diğer / Otomatik",
        order: &[
            "fake-split-md5sig",
            "fake-split-seqovl",
            "fake-split-badsum",
            "fake-split-pos1",
            "fake-disorder-md5sig",
            "fake-autottl",
            "fake-ttl4",
        ],
    },
];

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RemoteIspUpdates {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub dynamic_strategies: Vec<DynamicStrategy>,
    #[serde(default)]
    pub custom_isp_orders: HashMap<String, Vec<String>>,
}

/// Returns ordered strategy IDs for the given ISP.
#[allow(dead_code)]
pub fn order(isp: &str) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = ISPS
        .iter()
        .find(|i| i.id == isp)
        .unwrap_or(&ISPS[ISPS.len() - 1])
        .order
        .to_vec();

    for s in STRATEGIES {
        if !v.contains(&s.id) {
            v.push(s.id);
        }
    }
    v
}

/// Finds a strategy definition by its identifier. Falls back to default first strategy.
#[allow(dead_code)]
pub fn strategy(id: &str) -> &'static Strategy {
    STRATEGIES.iter().find(|s| s.id == id).unwrap_or(&STRATEGIES[0])
}

/// Loads cached remote ISP rules from local config dir if available.
pub fn load_cached_updates(dir: &Path) -> Option<RemoteIspUpdates> {
    let path = dir.join("isp_updates.json");
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            return serde_json::from_str(&content).ok();
        }
    }
    None
}

/// Saves remote ISP updates to local cache.
pub fn save_cached_updates(dir: &Path, updates: &RemoteIspUpdates) -> Result<(), String> {
    let path = dir.join("isp_updates.json");
    let json = serde_json::to_string_pretty(updates).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StrategyDef {
    pub id: String,
    pub name: String,
    pub tls: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IspDef {
    pub id: String,
    pub name: String,
    pub order: Vec<String>,
}

/// Returns all strategies combining static built-in strategies and any dynamic ones from cache.
pub fn get_all_strategies(dir: Option<&Path>) -> Vec<StrategyDef> {
    let mut list: Vec<StrategyDef> = STRATEGIES
        .iter()
        .map(|s| StrategyDef {
            id: s.id.to_string(),
            name: s.name.to_string(),
            tls: s.tls.iter().map(|&x| x.to_string()).collect(),
        })
        .collect();

    if let Some(d) = dir {
        if let Some(updates) = load_cached_updates(d) {
            for dyn_s in updates.dynamic_strategies {
                if let Some(existing) = list.iter_mut().find(|s| s.id == dyn_s.id) {
                    existing.name = dyn_s.name;
                    existing.tls = dyn_s.tls;
                } else {
                    list.push(StrategyDef {
                        id: dyn_s.id,
                        name: dyn_s.name,
                        tls: dyn_s.tls,
                    });
                }
            }
        }
    }

    list
}

/// Returns all ISP presets, updated with dynamic custom orders if available.
pub fn get_all_isps(dir: Option<&Path>) -> Vec<IspDef> {
    ISPS.iter()
        .map(|isp| IspDef {
            id: isp.id.to_string(),
            name: isp.name.to_string(),
            order: order_with_updates(isp.id, dir),
        })
        .collect()
}

/// Resolves TLS desync arguments for the given strategy ID, checking dynamic updates first.
pub fn resolve_strategy_tls(id: &str, dir: Option<&Path>) -> Vec<String> {
    if let Some(d) = dir {
        if let Some(updates) = load_cached_updates(d) {
            if let Some(dyn_s) = updates.dynamic_strategies.into_iter().find(|s| s.id == id) {
                return dyn_s.tls;
            }
        }
    }

    if let Some(s) = STRATEGIES.iter().find(|s| s.id == id) {
        return s.tls.iter().map(|&x| x.to_string()).collect();
    }

    // Default fallback to first strategy
    STRATEGIES[0].tls.iter().map(|&x| x.to_string()).collect()
}

/// Returns ordered strategy IDs for the given ISP, incorporating remote updates and dynamic strategies.
pub fn order_with_updates(isp: &str, dir: Option<&Path>) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();

    if let Some(d) = dir {
        if let Some(updates) = load_cached_updates(d) {
            if let Some(custom_order) = updates.custom_isp_orders.get(isp) {
                v = custom_order.clone();
            }
        }
    }

    if v.is_empty() {
        let base = ISPS
            .iter()
            .find(|i| i.id == isp)
            .unwrap_or(&ISPS[ISPS.len() - 1])
            .order;
        v = base.iter().map(|&s| s.to_string()).collect();
    }

    // Append any strategies not in the list yet
    let all = get_all_strategies(dir);
    for s in all {
        if !v.contains(&s.id) {
            v.push(s.id);
        }
    }

    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_strategies_have_valid_tls_args() {
        for s in STRATEGIES {
            assert!(!s.id.is_empty(), "Strategy ID must not be empty");
            assert!(!s.name.is_empty(), "Strategy name must not be empty");
            assert!(!s.tls.is_empty(), "Strategy tls args must not be empty for {}", s.id);
            for arg in s.tls {
                assert!(arg.starts_with("--dpi-desync"), "Argument {} in {} must start with --dpi-desync", arg, s.id);
            }
        }
    }

    #[test]
    fn test_isp_order_contains_unique_elements() {
        for isp in ISPS {
            let ord = order(isp.id);
            let mut seen = std::collections::HashSet::new();
            for id in ord {
                assert!(seen.insert(id), "Duplicate strategy {} in ISP {}", id, isp.id);
            }
        }
    }

    #[test]
    fn test_modern_strategies_exist() {
        let seqovl = strategy("fake-split-seqovl");
        assert_eq!(seqovl.id, "fake-split-seqovl");
        assert!(seqovl.tls.iter().any(|a| a.contains("split-seqovl")));

        let badsum = strategy("fake-split-badsum");
        assert_eq!(badsum.id, "fake-split-badsum");
        assert!(badsum.tls.iter().any(|a| a.contains("badsum")));

        let syndata = strategy("syndata-fake");
        assert_eq!(syndata.id, "syndata-fake");
        assert!(syndata.tls.iter().any(|a| a.contains("syndata")));
    }

    #[test]
    fn test_dynamic_strategy_resolution_and_fallback() {
        let temp_dir = std::env::temp_dir().join("slipstream_isp_test");
        let _ = fs::create_dir_all(&temp_dir);

        let mut updates = RemoteIspUpdates::default();
        updates.version = 2;
        updates.dynamic_strategies.push(DynamicStrategy {
            id: "custom-ultra-bypass".into(),
            name: "Custom Ultra Bypass".into(),
            tls: vec!["--dpi-desync=fake,multisplit".into(), "--dpi-desync-split-pos=3".into()],
        });

        let mut custom_orders = HashMap::new();
        custom_orders.insert("turk-telekom".into(), vec!["custom-ultra-bypass".into(), "fake-split-seqovl".into()]);
        updates.custom_isp_orders = custom_orders;

        assert!(save_cached_updates(&temp_dir, &updates).is_ok());

        // Test loading cached updates
        let loaded = load_cached_updates(&temp_dir);
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().dynamic_strategies[0].id, "custom-ultra-bypass");

        // Test get_all_strategies includes the dynamic strategy
        let all = get_all_strategies(Some(&temp_dir));
        assert!(all.iter().any(|s| s.id == "custom-ultra-bypass"));

        // Test resolve_strategy_tls returns dynamic flags
        let tls = resolve_strategy_tls("custom-ultra-bypass", Some(&temp_dir));
        assert_eq!(tls, vec!["--dpi-desync=fake,multisplit", "--dpi-desync-split-pos=3"]);

        // Test order_with_updates prioritizes dynamic custom order
        let order = order_with_updates("turk-telekom", Some(&temp_dir));
        assert_eq!(order[0], "custom-ultra-bypass");
        assert_eq!(order[1], "fake-split-seqovl");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
