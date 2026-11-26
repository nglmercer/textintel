#!/usr/bin/env python3
"""Generate src/speech/data/unit-symbols.json from pinned CLDR 48.0.0.

The extraction is deliberately narrow: suffix unit patterns from CLDR short and
narrow widths only. Patterns that place text before {0}, omit {0}, or contain a
second replacement field are skipped. Directional formatting controls are not
semantic letters and are removed; joining characters such as ZWNJ are retained.
"""
from __future__ import annotations

import json
import unicodedata
import urllib.request
from pathlib import Path

VERSION = "48.0.0"
BASE = (
    "https://raw.githubusercontent.com/unicode-org/cldr-json/"
    f"{VERSION}/cldr-json/cldr-units-full/main"
)
LOCALES = [
    "ar", "bn", "de", "en", "es", "fr", "hi", "id", "ja", "ko",
    "mr", "pa", "pt", "ru", "ta", "te", "tr", "ur", "vi", "zh",
]
# The same 55 canonical keys bundled in lexicons.json.
UNIT_KEYS = [
    "area-acre", "area-hectare", "area-square-kilometer", "area-square-meter",
    "concentr-percent", "concentr-permille", "digital-bit", "digital-byte",
    "digital-gigabit", "digital-gigabyte", "digital-kilobit", "digital-kilobyte",
    "digital-megabit", "digital-megabyte", "digital-terabyte", "duration-hour",
    "duration-millisecond", "duration-minute", "duration-second", "electric-ampere",
    "electric-volt", "energy-kilowatt-hour", "frequency-gigahertz", "frequency-hertz",
    "frequency-kilohertz", "frequency-megahertz", "length-centimeter", "length-foot",
    "length-inch", "length-kilometer", "length-meter", "length-mile",
    "length-millimeter", "length-yard", "mass-gram", "mass-kilogram",
    "mass-microgram", "mass-milligram", "mass-tonne", "power-kilowatt",
    "power-megawatt", "power-watt", "pressure-hectopascal", "pressure-millibar",
    "pressure-millimeter-ofhg", "speed-kilometer-per-hour", "speed-meter-per-second",
    "speed-mile-per-hour", "temperature-celsius", "temperature-fahrenheit",
    "temperature-kelvin", "volume-cubic-centimeter", "volume-cubic-meter",
    "volume-liter", "volume-milliliter",
]
# Directional controls may be needed for layout but do not change the typed
# unit characters. ZWNJ/ZWJ are retained because they can be lexical in an
# abbreviation (for example Telugu digitality abbreviations).
BIDI_CONTROLS = {
    "\u061c", "\u200e", "\u200f", "\u202a", "\u202b", "\u202c",
    "\u202d", "\u202e", "\u2066", "\u2067", "\u2068", "\u2069",
}


def nfkc(text: str) -> str:
    return unicodedata.normalize("NFKC", text)


def extract_width(entry: dict, width: str) -> list[dict]:
    found: dict[tuple[str, str], set[str]] = {}
    for pattern_key, pattern in entry.items():
        if not isinstance(pattern, str) or not pattern_key.startswith("unitPattern-count-"):
            continue
        if pattern.count("{0}") != 1 or "{" in pattern.replace("{0}", ""):
            continue
        prefix, suffix = pattern.split("{0}", 1)
        # This parser supports quantity + suffix-unit forms. Prefix or split
        # patterns are grammar constructions, not simple written aliases.
        if prefix:
            continue
        suffix = "".join(ch for ch in suffix if ch not in BIDI_CONTROLS)
        suffix = suffix.rstrip()
        if not suffix:
            continue
        separated = suffix[:1].isspace()
        form = nfkc(suffix.lstrip())
        if not form or form.startswith("/"):
            # A leading slash is a reciprocal/per construction, not a simple
            # unit token. One pinned Bengali narrow milligram pattern has this
            # shape and is therefore not evidence for attached unit parsing.
            continue
        key = (form, "separated" if separated else "attached")
        found.setdefault(key, set()).add(width)
    return [
        {"form": form, "attachment": attachment, "widths": sorted(widths)}
        for (form, attachment), widths in sorted(
            found.items(), key=lambda item: (-len(item[0][0]), item[0][0], item[0][1])
        )
    ]


def main() -> None:
    locales: dict[str, dict[str, list[dict]]] = {}
    for locale in LOCALES:
        url = f"{BASE}/{locale}/units.json"
        with urllib.request.urlopen(url, timeout=30) as response:
            data = json.load(response)["main"][locale]["units"]
        rows: dict[str, list[dict]] = {}
        for key in UNIT_KEYS:
            merged: dict[tuple[str, str], set[str]] = {}
            for width in ("short", "narrow"):
                entry = data.get(width, {}).get(key)
                if not isinstance(entry, dict):
                    continue
                for form in extract_width(entry, width):
                    merged.setdefault((form["form"], form["attachment"]), set()).update(
                        form["widths"]
                    )
            forms = [
                {"form": form, "attachment": attachment, "widths": sorted(widths)}
                for (form, attachment), widths in sorted(
                    merged.items(), key=lambda item: (-len(item[0][0]), item[0][0], item[0][1])
                )
            ]
            if forms:
                rows[key] = forms
        locales[locale] = rows
    out = {
        "source": {
            "name": "Unicode CLDR JSON short and narrow unit patterns",
            "version": VERSION,
            "base_url": BASE,
            "extraction": "suffix unitPattern-count-* forms; directional controls removed; whitespace reduced to attached/separated; leading-slash reciprocal forms skipped",
        },
        "locales": locales,
    }
    path = Path("src/speech/data/unit-symbols.json")
    path.write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    forms = sum(len(rows) for locale in locales.values() for rows in locale.values())
    print(f"wrote {path}: {len(locales)} locales, {forms} distinct surface forms")


if __name__ == "__main__":
    main()
