#!/usr/bin/env python3
"""Source-bound, memory-only native CPU/Metal experiment on the user's Mac."""
from contextlib import contextmanager
from dataclasses import asdict
from datetime import datetime, timezone
import argparse
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from process_guard import run


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")
    temporary.chmod(0o600)
    temporary.replace(path)


def clean(root, revision):
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True, timeout=10).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True, timeout=10).strip()
    require(head == revision and not dirty, "exact clean source required")


@contextmanager
def slot(limit):
    started = time.monotonic()
    while True:
        lock = open("/tmp/fincode-build.lock", "a")
        try:
            while True:
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    if time.monotonic() - started >= limit:
                        raise TimeoutError("shared native build slot unavailable")
                    time.sleep(.1)
            current, held = os.stat(lock.name), os.fstat(lock.fileno())
            if (current.st_dev, current.st_ino) == (held.st_dev, held.st_ino):
                break
        except BaseException:
            lock.close()
            raise
        fcntl.flock(lock, fcntl.LOCK_UN)
        lock.close()
    value = {"waitLimitSeconds": limit, "waitSeconds": round(time.monotonic()-started, 3),
             "device": held.st_dev, "inode": held.st_ino, "acquired": True, "released": False, "pid": os.getpid()}
    try:
        yield value, lock
    finally:
        fcntl.flock(lock, fcntl.LOCK_UN)
        lock.close()
        value["released"] = True


def validate(report, samples, runs, gpu):
    require(report.get("schemaVersion") == 1 and report.get("scope") == "generated_nv12_memory_only_comparison" and report.get("productionAdmission") is False and not report.get("failed"), "invalid experiment scope")
    require(report["cpuOraclesPassed"] is True and report["gpuAvailable"] is gpu and report["gpuOraclesPassed"] is gpu and report["gpuSnapshotRoundTripPassed"] is gpu, "missing comparison oracle")
    require(report["roundTripByteValuesPerComponent"] == [256, 256, 256] and report["finalSnapshotBytes"] == 0, "incomplete byte/reset oracle")
    require(report["samplesPerCase"] == samples and report["withinProcessRepeats"] == runs and report["warmupPerCase"] == 10 and report["snapshotLimitBytes"] == 64*1024**2, "changed experiment bounds")
    require(type(report["maximumProcessResidentBytes"]) is int and report["maximumProcessResidentBytes"] > 0, "missing process peak RSS")
    patterns = {"unchanged", "padding", "lumaFirst", "lumaLast", "chromaUFirst", "chromaVLast", "fullChange"}
    geometries = {(1112, 720), (1670, 1080), (3840, 2160)}
    expected = {(backend, trial, pattern, width, height) for backend in (["cpu", "metal"] if gpu else ["cpu"])
                for trial in range(1, runs+1) for pattern in patterns for width, height in geometries}
    seen = set()
    for case in report["cases"]:
        key = case["backend"], case["withinProcessRepeat"], case["pattern"], case["width"], case["height"]
        require(key in expected and key not in seen, "duplicate/unexpected comparison case")
        seen.add(key)
        active = case["width"] * case["height"] * 3 // 2
        require(case["oraclesPassed"] is True and case["changed"] is (case["pattern"] not in {"unchanged", "padding"}), "comparison result mismatch")
        require(case["activeBytes"] == case["snapshotLogicalBytes"] == active and type(case["ownedSnapshotAllocationBytes"]) is int and case["ownedSnapshotAllocationBytes"] >= active, "missing bounded snapshot allocation")
        for name in ("referenceInputStrideBytes", "candidateInputStrideBytes"):
            require(len(case[name]) == 2 and all(type(v) is int and v >= case["width"] for v in case[name]), "invalid input stride")
        wall = case["wallMicros"]
        require(wall["samples"] == samples and all(type(wall[k]) in (float, int) and math.isfinite(wall[k]) and wall[k] > 0 for k in ("mean", "p50", "p95", "maximum")), "invalid measured wall cost")
        require(wall["p50"] <= wall["p95"] <= wall["maximum"], "invalid measured percentiles")
        require(type(case["processCpuMicros"]) in (float, int) and math.isfinite(case["processCpuMicros"]) and case["processCpuMicros"] >= 0 and abs(case["processCpuMicrosPerCall"]*samples-case["processCpuMicros"]) < .001, "invalid measured process CPU")
        if case["backend"] == "cpu":
            require(case["gpuCommandMicros"] is None, "CPU case claimed GPU timing")
        else:
            require(type(case["gpuCommandMicros"]) is dict and case["gpuCommandMicros"]["samples"] == samples, "missing actual GPU command timing")
    require(seen == expected, "comparison matrix incomplete")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--developer-dir", type=Path)
    parser.add_argument("--samples", type=int, default=60)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    require(sys.platform == "darwin" and platform.machine() in ("arm64", "x86_64"), "native macOS required")
    require(20 <= args.samples <= 300 and args.samples % 2 == 0 and 1 <= args.runs <= 3, "invalid bounded matrix")
    root, out = args.workspace.resolve(), args.output.resolve()
    require(not out.is_relative_to(root) and not out.exists(), "new private output outside the checkout required")
    out.mkdir(parents=True, mode=0o700); (out/"private").mkdir(mode=0o700)
    env = os.environ | {"PYTHONDONTWRITEBYTECODE": "1"}
    if args.developer_dir:
        env["DEVELOPER_DIR"] = str(args.developer_dir)
    steps, queue = [], {}
    state = {"source": args.revision, "status": "queued", "runnerSha256": digest(Path(__file__)),
             "guardSha256": digest(root/"media/tools/process_guard.py"), "productionAdmission": False,
             "startedAtUtc": datetime.now(timezone.utc).isoformat()}
    write(out/"results.json", state)
    try:
        clean(root, args.revision)
        with slot(900) as (queue, lock):
            state = state | {"status": "running", "queue": queue, "steps": steps}
            write(out/"results.json", state)

            def step(name, command, seconds):
                clean(root, args.revision)
                held, current = os.fstat(lock.fileno()), os.stat(lock.name)
                require((held.st_dev, held.st_ino) == (current.st_dev, current.st_ino), "shared lock inode changed")
                print("Starting " + name, flush=True)
                path = out/"private"/(name+".log")
                with path.open("x") as output:
                    path.chmod(0o600)
                    result = run(["/usr/bin/nice", "-n", "19", *command], cwd=root, env=env, output=output, seconds=seconds)
                record = asdict(result); record["end"] = result.end.value
                record.update(name=name, command=command, deadlineSeconds=seconds, privateLog=str(path.relative_to(out)), logSha256=digest(path))
                steps.append(record)
                clean(root, args.revision)
                held, current = os.fstat(lock.fileno()), os.stat(lock.name)
                require((held.st_dev, held.st_ino) == (current.st_dev, current.st_ino), "shared lock inode changed")
                write(out/"results.json", state)
                require(result.code == result.child_code == 0 and not result.signals and result.group_gone and result.leader_held_until_quiet, "command failed or required cleanup: " + name)
                return path

            step("load", ["uptime"], 10)
            guard_log = step("guard-contracts", [sys.executable, "-B", "media/tools/test-process-guard.py"], 30)
            require(re.search(r"Ran 5 tests", guard_log.read_text()) is not None, "guard test selection did not execute five cases")
            step("toolchain", ["xcrun", "swiftc", "--version"], 15)
            binary = out/"private/compare-frames"
            step("build", ["xcrun", "--sdk", "macosx", "swiftc", "-swift-version", "5", "-parse-as-library", "-target", platform.machine()+"-apple-macos14.0", "-O", "-warnings-as-errors", "media/tools/compare-frames.swift", "-o", str(binary)], 120)
            binary_hash = digest(binary)
            control_path = step("cpu-control", [str(binary), "20", "1", "--cpu-only"], 120)
            require(digest(binary) == binary_hash, "experiment executable changed")
            matrix_path = step("metal-matrix", [str(binary), str(args.samples), str(args.runs)], 240)
            require(digest(binary) == binary_hash, "experiment executable changed")
            control, matrix = json.loads(control_path.read_text()), json.loads(matrix_path.read_text())
            validate(control, 20, 1, False); validate(matrix, args.samples, args.runs, True)
            write(out/"cpu-control.json", control); write(out/"comparison.json", matrix)
            state = state | {"executableSha256": binary_hash, "swiftSourceSha256": digest(root/"media/tools/compare-frames.swift"), "matrixValidated": True}
        state = state | {"status": "complete", "exitCode": 0, "queue": queue, "steps": steps}
        write(out/"results.json", state)
        print("Memory-only CPU/Metal matrix complete; no production admission.", flush=True)
        return 0
    except BaseException as error:
        code = 75 if isinstance(error, TimeoutError) else 130 if isinstance(error, KeyboardInterrupt) else 1
        state = state | {"status": "failed", "exitCode": code, "queue": queue, "steps": steps, "failure": type(error).__name__+": "+str(error)}
        write(out/"results.json", state)
        print("Comparison did not finish; inspect retained diagnostics.", flush=True)
        return code


if __name__ == "__main__":
    raise SystemExit(main())
