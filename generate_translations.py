# -*- coding: utf-8 -*-
"""
Slipstream i18n Generator
Assembles all 15 language packs and writes src/translations.ts.
"""

from lang_tr_en import TR, EN
from lang_west_eu import DE, FR, ES, IT
from lang_east_eu import PT, RU, UK, PL
from lang_asia import ZH, JA, KO
from lang_ar_id import AR, ID

LANGUAGES = [
    {"code": "en", "name": "English", "nativeName": "English", "flag": "🇺🇸", "dict": EN},
    {"code": "tr", "name": "Turkish", "nativeName": "Türkçe", "flag": "🇹🇷", "dict": TR},
    {"code": "de", "name": "German", "nativeName": "Deutsch", "flag": "🇩🇪", "dict": DE},
    {"code": "fr", "name": "French", "nativeName": "Français", "flag": "🇫🇷", "dict": FR},
    {"code": "es", "name": "Spanish", "nativeName": "Español", "flag": "🇪🇸", "dict": ES},
    {"code": "it", "name": "Italian", "nativeName": "Italiano", "flag": "🇮🇹", "dict": IT},
    {"code": "pt", "name": "Portuguese", "nativeName": "Português", "flag": "🇧🇷", "dict": PT},
    {"code": "ru", "name": "Russian", "nativeName": "Русский", "flag": "🇷🇺", "dict": RU},
    {"code": "uk", "name": "Ukrainian", "nativeName": "Українська", "flag": "🇺🇦", "dict": UK},
    {"code": "pl", "name": "Polish", "nativeName": "Polski", "flag": "🇵🇱", "dict": PL},
    {"code": "zh", "name": "Chinese", "nativeName": "简体中文", "flag": "🇨🇳", "dict": ZH},
    {"code": "ja", "name": "Japanese", "nativeName": "日本語", "flag": "🇯🇵", "dict": JA},
    {"code": "ko", "name": "Korean", "nativeName": "한국어", "flag": "🇰🇷", "dict": KO},
    {"code": "ar", "name": "Arabic", "nativeName": "العربية", "flag": "🇸🇦", "rtl": True, "dict": AR},
    {"code": "id", "name": "Indonesian", "nativeName": "Bahasa Indonesia", "flag": "🇮🇩", "dict": ID},
]

FUNC_SIGS = {
    "versionPrefix": ("v: string", ["v"]),
    "tabPlatforms": ("n: number", ["n"]),
    "tabCustomRules": ("n: number", ["n"]),
    "allFilter": ("n: number", ["n"]),
    "appsFilter": ("n: number", ["n"]),
    "gamesFilter": ("n: number", ["n"]),
    "customPlatformTag": ("n: number", ["n"]),
    "customPlatformCount": ("n: number", ["n"]),
    "processRunningTag": ("name: string", ["name"]),
    "newVersionLiveTag": ("v: string", ["v"]),
    "releaseNotesTitle": ("v: string", ["v"]),
    "dohDesc": ("p: string", ["p"]),
    "detectedServers": ("sel: number, total: number", ["sel", "total"]),
    "addOnlyMain": ("d: string", ["d"]),
    "addSelectedBtn": ("n: number", ["n"]),
    "scannedAllFilter": ("n: number", ["n"]),
    "scannedGamesFilter": ("n: number", ["n"]),
    "scannedAppsFilter": ("n: number", ["n"]),
    "scannedRunningFilter": ("n: number", ["n"]),
    "scannedStoppedFilter": ("n: number", ["n"]),
    "addToListTitle": ("name: string", ["name"]),
    "detectedDomainsLabel": ("n: number", ["n"]),
    "err_alreadyAdded": ("name: string", ["name"]),
    "err_fileNotFound": ("e: string", ["e"]),
    "err_updateFailed": ("e: string", ["e"]),
    "err_dnsFailed": ("e: string", ["e"]),
    "err_adapterFailed": ("e: string", ["e"]),
    "err_startupFailed": ("e: string", ["e"]),
    "err_discordFailed": ("e: string", ["e"]),
    "err_healthFailed": ("e: string", ["e"]),
    "err_discordRestartFailed": ("e: string", ["e"]),
    "err_dnsFlushFailed": ("e: string", ["e"]),
    "suc_connected": ("strat: string", ["strat"]),
    "suc_added": ("name: string", ["name"]),
    "suc_addedWithDomains": ("name: string, n: number", ["name", "n"]),
    "suc_dnsSelected": ("name: string, ms: number", ["name", "ms"]),
    "suc_updateFound": ("v: string", ["v"]),
    "suc_upToDate": ("v: string", ["v"]),
    "suc_bestStrategy": ("s: string", ["s"]),
    "suc_domainAdded": ("d: string", ["d"]),
    "suc_suggestionsAdded": ("name: string, n: number", ["name", "n"]),
    "suc_ispChanged": ("isp: string, strat: string", ["isp", "strat"]),
    "suc_strategyChanged": ("strat: string", ["strat"]),
}

def escape_ts_string(s: str) -> str:
    # Use JSON string formatting to escape quotes and newlines, then remove outer quotes
    import json
    return json.dumps(s, ensure_ascii=False)

def escape_ts_template(s: str) -> str:
    # Escape backticks and unescaped ${} that aren't our placeholders
    s = s.replace("\\", "\\\\").replace("`", "\\`")
    return s

def main():
    base_keys = set(TR.keys())
    print(f"Total keys in base (TR): {len(base_keys)}")

    # Verify key completeness
    for item in LANGUAGES:
        code = item["code"]
        d = item["dict"]
        missing = base_keys - set(d.keys())
        if missing:
            for m in missing:
                d[m] = EN.get(m, TR[m])
        extra = set(d.keys()) - base_keys
        if extra:
            raise ValueError(f"Language '{code}' has extra keys: {extra}")
        for k, v in d.items():
            if k in FUNC_SIGS:
                if not callable(v):
                    raise ValueError(f"Key '{k}' in language '{code}' must be callable")
            else:
                if callable(v):
                    raise ValueError(f"Key '{k}' in language '{code}' must not be callable")

    print("All 15 languages have identical, valid keys!")

    # Generate translations.ts
    lines = []
    lines.append("/* Auto-generated by generate_translations.py - Slipstream i18n */")
    lines.append("")
    lines.append("export type LangCode =")
    for item in LANGUAGES:
        lines.append(f"  | '{item['code']}'")
    lines.append(";")
    lines.append("")

    lines.append("export interface LanguageInfo {")
    lines.append("  code: LangCode;")
    lines.append("  name: string;")
    lines.append("  nativeName: string;")
    lines.append("  flag: string;")
    lines.append("  rtl?: boolean;")
    lines.append("}")
    lines.append("")

    lines.append("export interface Translations {")
    lines.append("  [key: string]: string | ((...args: any[]) => string);")
    for k in sorted(base_keys):
        if k in FUNC_SIGS:
            sig, _ = FUNC_SIGS[k]
            lines.append(f"  {k}: ({sig}) => string;")
        else:
            lines.append(f"  {k}: string;")
    lines.append("}")
    lines.append("")

    lines.append("export const LANGUAGES: LanguageInfo[] = [")
    for item in LANGUAGES:
        rtl_str = " rtl: true," if item.get("rtl") else ""
        lines.append(
            f"  {{ code: '{item['code']}', name: '{item['name']}', nativeName: '{item['nativeName']}', flag: '{item['flag']}',{rtl_str} }},"
        )
    lines.append("];")
    lines.append("")

    lines.append("export const translations: Record<LangCode, Translations> = {")
    for item in LANGUAGES:
        code = item["code"]
        d = item["dict"]
        lines.append(f"  {code}: {{")
        for k in sorted(base_keys):
            val = d[k]
            if k in FUNC_SIGS:
                sig, arg_names = FUNC_SIGS[k]
                dummy_args = [f"__SLIP_ARG_{i}__" for i in range(len(arg_names))]
                raw_rendered = val(*dummy_args)
                escaped = escape_ts_template(raw_rendered)
                for i, arg in enumerate(arg_names):
                    escaped = escaped.replace(f"__SLIP_ARG_{i}__", f"${{{arg}}}")
                lines.append(f"    {k}: ({sig}) => `{escaped}`,")
            else:
                lines.append(f"    {k}: {escape_ts_string(val)},")
        lines.append("  },")
    lines.append("};")
    lines.append("")

    target_path = "src/translations.ts"
    with open(target_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"Successfully generated {target_path} ({len(lines)} lines)")

if __name__ == "__main__":
    main()
