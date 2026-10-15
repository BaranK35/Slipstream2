//! Desync strategies and ISP presets.
use serde::Serialize;

#[derive(Serialize)]
pub struct Strategy {
    pub id: &'static str,
    pub name: &'static str,
    pub tls: &'static [&'static str],
}

pub const STRATEGIES: &[Strategy] = &[
    Strategy {
        id: "fake-split-md5sig",
        name: "Fake + Split2 (md5sig - En Yüksek Uyum)",
        tls: &["--dpi-desync=fake,split2", "--dpi-desync-split-pos=2", "--dpi-desync-fooling=md5sig"],
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

#[derive(Serialize)]
pub struct Isp {
    pub id: &'static str,
    pub name: &'static str,
    pub order: &'static [&'static str],
}

pub const ISPS: &[Isp] = &[
    Isp {
        id: "turk-telekom",
        name: "Türk Telekom",
        order: &["fake-split-md5sig", "fake-split-pos1", "fake-disorder-md5sig", "fake-autottl", "fake-ttl4", "multisplit-pos2"],
    },
    Isp {
        id: "turkcell",
        name: "Turkcell Superonline",
        order: &["fake-split-md5sig", "fake-split-pos1", "fake-disorder-md5sig", "fake-ttl3", "fake-autottl", "multisplit-pos2"],
    },
    Isp {
        id: "vodafone-tr",
        name: "Vodafone Türkiye",
        order: &["fake-split-md5sig", "fake-split-pos1", "fake-ttl4", "fake-autottl", "multisplit-pos2"],
    },
    Isp {
        id: "turknet",
        name: "TurkNet",
        order: &["fake-split-md5sig", "fake-split-pos1", "fake-ttl4", "multisplit-pos2", "fake-autottl"],
    },
    Isp {
        id: "other",
        name: "Diğer / Otomatik",
        order: &["fake-split-md5sig", "fake-split-pos1", "fake-disorder-md5sig", "fake-autottl", "fake-ttl4"],
    },
];

pub fn order(isp: &str) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = ISPS.iter().find(|i| i.id == isp).unwrap_or(&ISPS[ISPS.len() - 1]).order.to_vec();
    for s in STRATEGIES {
        if !v.contains(&s.id) {
            v.push(s.id);
        }
    }
    v
}

pub fn strategy(id: &str) -> &'static Strategy {
    STRATEGIES.iter().find(|s| s.id == id).unwrap_or(&STRATEGIES[0])
}
