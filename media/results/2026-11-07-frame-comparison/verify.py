"""Verify retained numeric evidence without running native code or using pixels."""
from pathlib import Path
import argparse
import gzip
import hashlib
import json
import math
import statistics

ROOT = Path(__file__).resolve().parent
SOURCE = "69c924898ae38678de4fef888938f8b0e62d1b2a"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read(name):
    raw = (ROOT / name).read_bytes()
    return gzip.decompress(raw) if name.endswith(".gz") else raw


def verify_matrix(report, samples, repeats, backends):
    assert report["scope"] == "generated_nv12_memory_only_comparison"
    assert report["schemaVersion"] == 1 and report["productionAdmission"] is False
    assert report["cpuOraclesPassed"] is True
    gpu = "metal" in backends
    assert all(report[key] is gpu for key in ("gpuAvailable", "gpuOraclesPassed", "gpuSnapshotRoundTripPassed"))
    assert report["roundTripByteValuesPerComponent"] == [256, 256, 256]
    assert report["snapshotLimitBytes"] == 64 * 1024**2 and report["finalSnapshotBytes"] == 0
    assert report["samplesPerCase"] == samples and report["withinProcessRepeats"] == repeats
    assert report["warmupPerCase"] == 10
    assert type(report["maximumProcessResidentBytes"]) is int and report["maximumProcessResidentBytes"] > 0
    patterns = {"unchanged", "padding", "lumaFirst", "lumaLast", "chromaUFirst", "chromaVLast", "fullChange"}
    shapes = {(1112, 720), (1670, 1080), (3840, 2160)}
    expected = {(backend, repeat, pattern, width, height) for backend in backends
                for repeat in range(1, repeats + 1) for pattern in patterns for width, height in shapes}
    seen = set()
    for row in report["cases"]:
        key = row["backend"], row["withinProcessRepeat"], row["pattern"], row["width"], row["height"]
        assert key in expected and key not in seen
        seen.add(key)
        active = row["width"] * row["height"] * 3 // 2
        assert row["oraclesPassed"] is True and row["changed"] is (row["pattern"] not in {"unchanged", "padding"})
        assert row["activeBytes"] == row["snapshotLogicalBytes"] == active
        assert type(row["ownedSnapshotAllocationBytes"]) is int and row["ownedSnapshotAllocationBytes"] >= active
        for key in ("referenceInputStrideBytes", "candidateInputStrideBytes"):
            assert len(row[key]) == 2 and all(type(value) is int and value >= row["width"] for value in row[key])
        if row["pattern"] == "padding":
            assert all(a != b for a, b in zip(row["referenceInputStrideBytes"], row["candidateInputStrideBytes"]))
        for key in ("wallMicros", "gpuCommandMicros"):
            values = row[key]
            if key == "gpuCommandMicros" and row["backend"] == "cpu":
                assert values is None
                continue
            assert values["samples"] == samples
            assert all(type(values[k]) in (int, float) and math.isfinite(values[k]) and values[k] > 0
                       for k in ("mean", "p50", "p95", "maximum"))
            assert values["p50"] <= values["p95"] <= values["maximum"]
            assert values["mean"] <= values["maximum"]
        cpu = row["processCpuMicros"]
        assert type(cpu) in (int, float) and math.isfinite(cpu) and cpu >= 0
        assert abs(row["processCpuMicrosPerCall"] * samples - cpu) < .001
    assert seen == expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path)
    args = parser.parse_args()
    files = json.loads(read("files.json"))
    for row in files:
        path = ROOT / row["path"]
        assert path.resolve().is_relative_to(ROOT) and path.is_file()
        raw = path.read_bytes()
        assert len(raw) == row["bytes"] and digest(raw) == row["sha256"], row["path"]
        assert b"/Users/" not in read(row["path"]), row["path"]
    originals = json.loads(read("artifacts.json"))
    assert len(originals) == 21 and len({row["path"] for row in originals}) == 21
    if args.archive:
        for row in originals:
            path = args.archive / row["path"]
            assert path.resolve().is_relative_to(args.archive.resolve())
            raw = path.read_bytes()
            assert len(raw) == row["bytes"] and digest(raw) == row["sha256"], row["path"]
    receipts = {row["cohort"]: row for row in json.loads(read("receipts.json"))}
    assert set(receipts) == {"initial-build", "rejected-requested-sha", "complete"}
    original_map = {row["path"]: row for row in originals}
    recorded_groups = set()
    for name, wrapper in receipts.items():
        assert original_map[name + "/results.json"]["sha256"] == wrapper["originalReceiptSha256"]
        receipt = wrapper["receipt"]
        for step in receipt.get("steps", []):
            recorded_groups.add(step["pgid"])
            assert step["end"] == "exit" and not step["signals"]
            assert step["leader_held_until_quiet"] and step["group_gone"]
            assert step["code"] == step["child_code"]
            assert original_map[name + "/" + step["privateLog"]]["sha256"] == step["logSha256"]
    failed = receipts["initial-build"]["receipt"]
    assert failed["source"] == "65de1a344c8deeb9cb8396f33ebb0fd7421637f9"
    assert failed["status"] == "failed" and failed["exitCode"] == 1
    assert [(step["name"], step["code"]) for step in failed["steps"]] == [("load", 0), ("guard-contracts", 0), ("toolchain", 0), ("build", 1)]
    assert b"withExtendedLifetime" in read("logs/initial-build-build.log.gz")
    rejected = receipts["rejected-requested-sha"]["receipt"]
    assert rejected["status"] == "failed" and rejected["exitCode"] == 1 and rejected["steps"] == []
    assert rejected["source"] != SOURCE and rejected["failure"] == "RuntimeError: exact clean source required"
    complete = receipts["complete"]["receipt"]
    assert complete["source"] == SOURCE and complete["status"] == "complete" and complete["exitCode"] == 0
    assert complete["productionAdmission"] is False and complete["matrixValidated"]
    assert complete["queue"]["acquired"] and complete["queue"]["released"]
    assert [(step["name"], step["code"]) for step in complete["steps"]] == [(name, 0) for name in ("load", "guard-contracts", "toolchain", "build", "cpu-control", "metal-matrix")]
    assert b"Ran 5 tests" in read("logs/complete-guard-contracts.log.gz")
    steps = {step["name"]: step for step in complete["steps"]}
    assert digest(read("comparison.json.gz")) == steps["metal-matrix"]["logSha256"]
    assert digest(read("cpu-control.json.gz")) == steps["cpu-control"]["logSha256"]
    for key, path in [("runnerSha256", "source/run-frame-comparison.py"),
                      ("guardSha256", "source/process_guard.py"),
                      ("swiftSourceSha256", "source/compare-frames.swift"),
                      ("executableSha256", "complete/private/compare-frames")]:
        assert complete[key] == original_map[path]["sha256"]
    control, matrix = json.loads(read("cpu-control.json.gz")), json.loads(read("comparison.json.gz"))
    verify_matrix(control, 20, 1, ["cpu"])
    verify_matrix(matrix, 60, 3, ["cpu", "metal"])
    assert control["gpuUnavailableReason"] == "explicit_cpu_control"
    assert matrix["device"] == "Apple M5 Pro" and matrix["unifiedMemory"] is True
    assert matrix["maximumProcessResidentBytes"] == 206143488
    assert control["maximumProcessResidentBytes"] == 51265536
    for width, height in [(1112, 720), (1670, 1080), (3840, 2160)]:
        values = []
        for backend in ["cpu", "metal"]:
            rows = [row for row in matrix["cases"] if row["backend"] == backend and row["pattern"] == "unchanged" and (row["width"], row["height"]) == (width, height)]
            values.append(round(statistics.median(row["wallMicros"]["p50"] for row in rows), 3))
        assert values == {(1112, 720): [32.208, 259.542], (1670, 1080): [71.958, 447.042], (3840, 2160): [309.208, 540.375]}[(width, height)]
    cleanup = json.loads(read("cleanup.json"))
    assert cleanup["source"] == SOURCE and cleanup["trackedSourceClean"]
    assert len(recorded_groups) == 10 and {row["pgid"] for row in cleanup["groups"]} == recorded_groups
    assert all(row["independentlyAbsent"] for row in cleanup["groups"])
    assert cleanup["initialQueueReleased"] and cleanup["completeQueueReleased"]
    print("Verified 147 generated-frame cases, five guard tests per compiled attempt, retained build/source failures, 21 original files and ten absent recorded command groups; no streaming or performance admission.")


if __name__ == "__main__":
    main()
