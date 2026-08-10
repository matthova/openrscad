#!/usr/bin/env python3
"""Validate the compatibility manifest without third-party dependencies."""

from __future__ import annotations

import json
import re
import sys
from collections import Counter
from pathlib import Path


HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
MANIFEST = HERE / "openscad-2021.01.json"
SCHEMA = HERE / "schema.json"

STATUSES = {
    "verified",
    "implemented",
    "missing",
    "warned_divergence",
    "permanent_divergence",
    "unknown",
}
CATEGORIES = {
    "syntax",
    "value",
    "operator",
    "function",
    "special_variable",
    "module",
    "module_parameter",
    "file_semantics",
    "import",
    "export",
}
TIERS = {
    "core_2021_01",
    "deprecated_core_2021_01",
    "current_stable",
    "openrscad_extension",
}
ID_RE = re.compile(r"^[a-z0-9][a-z0-9._:-]*$")


def load(path: Path) -> dict:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def evidence_paths(test_id: str) -> tuple[Path, ...] | None:
    if test_id.startswith("echo:"):
        name = test_id.removeprefix("echo:")
        return ROOT / "corpus" / "echo" / f"{name}.scad", ROOT / "corpus" / "golden" / "echo" / f"{name}.txt"
    if test_id.startswith("geom:"):
        name = test_id.removeprefix("geom:")
        return ROOT / "corpus" / "geom" / f"{name}.scad", ROOT / "corpus" / "golden" / "geom" / f"{name}.txt"
    if test_id.startswith("rust:"):
        path = test_id.removeprefix("rust:").split("#", 1)[0]
        return (ROOT / path,)
    return None


def main() -> int:
    errors: list[str] = []
    try:
        load(SCHEMA)
        doc = load(MANIFEST)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"compatibility manifest: {exc}", file=sys.stderr)
        return 1

    if doc.get("manifest_version") != 1:
        errors.append("manifest_version must be 1")
    target = doc.get("target", {})
    if target.get("product") != "OpenSCAD" or target.get("core_baseline") != "2021.01":
        errors.append("target must be OpenSCAD 2021.01")

    hosts = doc.get("hosts", {})
    if not isinstance(hosts, dict) or not hosts:
        errors.append("hosts must be a non-empty object")

    source_ids: set[str] = set()
    for source in doc.get("sources", []):
        source_id = source.get("id")
        if not isinstance(source_id, str) or not ID_RE.fullmatch(source_id):
            errors.append(f"invalid source id: {source_id!r}")
        elif source_id in source_ids:
            errors.append(f"duplicate source id: {source_id}")
        else:
            source_ids.add(source_id)
        if "path" in source and not (ROOT / source["path"]).is_file():
            errors.append(f"source {source_id}: missing local path {source['path']}")

    feature_ids: set[str] = set()
    features = doc.get("features", [])
    if not isinstance(features, list) or not features:
        errors.append("features must be a non-empty array")
        features = []

    required = {"id", "category", "surface", "tier", "status", "hosts", "summary"}
    for index, feature in enumerate(features):
        label = feature.get("id", f"features[{index}]")
        missing = required - feature.keys()
        if missing:
            errors.append(f"{label}: missing keys {sorted(missing)}")
            continue
        feature_id = feature["id"]
        if not isinstance(feature_id, str) or not ID_RE.fullmatch(feature_id):
            errors.append(f"invalid feature id: {feature_id!r}")
        elif feature_id in feature_ids:
            errors.append(f"duplicate feature id: {feature_id}")
        else:
            feature_ids.add(feature_id)
        if feature["status"] not in STATUSES:
            errors.append(f"{label}: invalid status {feature['status']!r}")
        if feature["category"] not in CATEGORIES:
            errors.append(f"{label}: invalid category {feature['category']!r}")
        if feature["tier"] not in TIERS:
            errors.append(f"{label}: invalid tier {feature['tier']!r}")
        if not isinstance(feature["hosts"], list) or not feature["hosts"]:
            errors.append(f"{label}: hosts must be a non-empty array")
        else:
            for host in feature["hosts"]:
                if host not in hosts:
                    errors.append(f"{label}: unknown host {host!r}")

        tests = feature.get("tests", [])
        if feature["status"] == "verified":
            if not tests:
                errors.append(f"{label}: verified entries require tests")
            if not any(test.startswith(("echo:", "geom:")) for test in tests):
                errors.append(f"{label}: verified entries require an echo: or geom: oracle")
        if feature["status"] in {"warned_divergence", "permanent_divergence"} and not feature.get("repros"):
            errors.append(f"{label}: divergence entries require repros")
        if feature["status"] == "permanent_divergence" and not feature.get("rationale"):
            errors.append(f"{label}: permanent divergences require a rationale")

        for test_id in tests:
            paths = evidence_paths(test_id)
            if paths is None:
                continue
            for path in paths:
                if not path.is_file():
                    errors.append(f"{label}: {test_id} is missing {path.relative_to(ROOT)}")

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1

    counts = Counter(feature["status"] for feature in features)
    rendered = ", ".join(f"{status}={counts[status]}" for status in sorted(STATUSES) if counts[status])
    core = sum(
        feature["tier"] in {"core_2021_01", "deprecated_core_2021_01"}
        for feature in features
    )
    print(f"compatibility manifest valid: {len(features)} features ({core} core/deprecated); {rendered}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
