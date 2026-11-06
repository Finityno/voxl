#!/usr/bin/env python3
"""Paired bounded native depth-one/depth-two encoder experiment; no app default."""
import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("matched", Path(__file__).with_name("run-matched-codecs.py"))
matched = importlib.util.module_from_spec(spec)
spec.loader.exec_module(matched)
require, digest, write = matched.require, matched.digest, matched.write


def ssim_regressions(baseline, candidate, frames):
    rows=[]
    for path in [baseline,candidate]:
        values=[]
        for line in path.read_text().splitlines():
            index=re.search(r"\bn:(\d+)",line);require(index is not None and int(index[1])==len(values)+1,"paired SSIM row order differs")
            row={key:float(value) for key,value in re.findall(r"(Y|U|V|All):([-+0-9.eE]+)",line)}
            require(set(row)=={"Y","U","V","All"} and all(math.isfinite(v) and -1<=v<=1 for v in row.values()),"invalid paired SSIM row")
            values.append(row)
        require(len(values)==frames,"paired SSIM frame count differs");rows.append(values)
    return {channel:sum(after[channel]<before[channel] for before,after in zip(*rows)) for channel in ["Y","U","V","All"]}


class PipelineRunner(matched.Runner):
    def __init__(self, args):
        super().__init__(args)
        # Retain the installed global compiler cache; targets remain per tree.
        if "RUSTC_WRAPPER" not in os.environ:
            self.environment.pop("RUSTC_WRAPPER", None)
        self.comparisons = []

    def save(self):
        expected = [f"pair-{pair}-{codec}-depth-{depth}" for pair in range(1,self.args.runs+1) for codec in self.args.codec for depth in [1,2]]
        write(self.args.output/"results.json", {"source":self.args.revision,"runnerSha256":digest(Path(__file__)),
            "matchedHelperSha256":digest(Path(matched.__file__)),"terminal":self.terminal,"exitCode":self.exit_code,
            "plan":{"width":self.width,"height":self.height,"fps":self.fps,"warmupSeconds":self.args.warmup_seconds,
                "observationSeconds":self.args.observation_seconds,"independentPairsPerCodec":self.args.runs,"codecs":self.args.codec,"targetBitrateKbps":self.args.bitrate_kbps},
            "steps":self.steps,"cases":self.cases,"comparisons":self.comparisons,
            "unexecutedCases":[name for name in expected if name not in {v["name"] for v in self.cases if v.get("oraclesComplete")}],
            "productionAdmission":False,"scope":"Generated matched native encoder/file input and independent decode/quality experiment only. No capture, control, transport or app changes. Receipt cadence is not display cadence. Process CPU/RSS includes I/O/init/warmup and excludes system helpers/GPU/power and app baseline."})

    def execute(self):
        executable = self.build()
        fixture, manifest, observation = self.fixture()
        floor = self.args.output/"private/floor.rgb"
        self.rgb("source-floor", observation, manifest, floor, True)
        for pair in range(1,self.args.runs+1):
            for codec_index, codec in enumerate(self.args.codec):
                records = []
                depths = [1,2] if (pair+codec_index)%2 else [2,1]
                for depth in depths:
                    name=f"pair-{pair}-{codec}-depth-{depth}"
                    records.append(self.case(executable,f"{codec}:compatible:{self.args.bitrate_kbps}",pair,fixture,manifest,observation,pipeline_depth=depth,name=name))
                baseline = next(v for v in records if v["pipeline"]["requestedDepth"]==1)
                candidate = next(v for v in records if v["pipeline"]["requestedDepth"]==2)
                require(baseline["inputSha256"]==candidate["inputSha256"],"paired source pixels changed")
                output = self.args.output/f"foreground-pair-{pair}-{codec}.json"
                command=[sys.executable,"-B","media/tools/analyze-text-workload.py","--fixture",str(fixture),"--baseline",baseline["name"],"--floor","uncompressed420","--decoded","uncompressed420="+str(floor),
                    "--observed-frames",str(self.args.observation_seconds*self.fps),"--capture-warmup-frames",str(self.args.warmup_seconds*self.fps),"--engine",self.args.analysis_engine,"--output",str(output)]
                for record in records:command += ["--decoded",record["name"]+"="+record["rgbPath"]]
                self.run(f"foreground-pair-{pair}-{codec}",command,300)
                foreground=json.loads(output.read_text())
                require(foreground["manifestSha256"]==digest(fixture/"manifest.json") and foreground["baseline"]==baseline["name"] and foreground["frameMapping"]=={
                    "captureWarmupFrames":self.args.warmup_seconds*self.fps,"sourcePrefixFrames":manifest["warmupFrames"],"sourceObservedCycleFrames":manifest["observedFrames"],"observedFrames":self.args.observation_seconds*self.fps},"pipeline foreground attribution mismatch")
                for record in records:
                    value=foreground["variants"][record["name"]]
                    require(value["sha256"]==record["normalizedRgbSha256"] and value["frames"]==self.args.observation_seconds*self.fps,"pipeline foreground count/hash mismatch")
                    record["foregroundAnalysis"]={"file":output.name,"sha256":digest(output),"meanAbsoluteForegroundRgbCodeError":value["meanAbsoluteForegroundRgbCodeError"],
                        "worseAggregateFrames":len(value["foregroundErrorWorseThanBaselineFrames"]),"worseAggregateChannelFrames":{k:len(v) for k,v in value["foregroundChannelErrorWorseThanBaselineFrames"].items()},
                        "scope":"Exact reference-derived ink/halo frame/channel aggregate sums. Not pixelwise/glyph/visual nonregression or complete quality admission."}
                    record["oraclesComplete"]=True
                    path=Path(record["rgbPath"]);require(path.is_relative_to(self.args.output/"private") and path.name=="thin.rgb","unexpected pipeline crop")
                    path.unlink();record["normalizedRgbIntermediateRemovedAfterAnalysis"]=True
                    write(self.args.output/(record["name"]+".json"),record)
                quality_ok = candidate["foregroundAnalysis"]["worseAggregateFrames"]==0 and not any(candidate["foregroundAnalysis"]["worseAggregateChannelFrames"].values())
                ssim_worse={region:ssim_regressions(Path(baseline["rgbPath"]).parent/(region+".ssim"),Path(candidate["rgbPath"]).parent/(region+".ssim"),self.args.observation_seconds*self.fps) for region in ["full","text","thin"]}
                self.comparisons.append({"pair":pair,"codec":codec,"order":depths,"baseline":baseline["name"],"candidate":candidate["name"],
                    "fixedInputSha256":baseline["inputSha256"],"sameInput":baseline["inputSha256"]==candidate["inputSha256"],
                    "sameEncodedBytes":baseline["encodedSha256"]==candidate["encodedSha256"],"sameNormalizedDecodedRgb":baseline["normalizedRgbSha256"]==candidate["normalizedRgbSha256"],
                    "foregroundNonregression":quality_ok,"worseSsimFrames":ssim_worse,"measuredQualityNonregression":quality_ok and not any(v for region in ssim_worse.values() for v in region.values()),
                    "baselineCompletionFps":baseline["pipeline"]["observedCompletionFps"],"candidateCompletionFps":candidate["pipeline"]["observedCompletionFps"],
                    "baselineReceiptP95Ms":baseline["encodeLatencyMs"]["p95"],"candidateReceiptP95Ms":candidate["encodeLatencyMs"]["p95"],
                    "baselinePeakRssBytes":baseline["processResources"]["maximumResidentBytes"],"candidatePeakRssBytes":candidate["processResources"]["maximumResidentBytes"],
                    "baselineCpuPercentOneCore":baseline["processResources"]["averageCpuPercentOneCore"],"candidateCpuPercentOneCore":candidate["processResources"]["averageCpuPercentOneCore"],
                    "scope":"Paired component observation, not production or performance-target admission. Quality differences are retained, never converted into passing gates."})
                self.save()
        floor_hash=digest(floor);floor.unlink()
        write(self.args.output/"floor.json",{"sha256":floor_hash,"regenerableIntermediateRemoved":True,"scope":"Uncompressed420 reconstruction control; no hardware decoder admission."})


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace",type=Path,default=Path(__file__).resolve().parents[2]);parser.add_argument("--revision",required=True)
    parser.add_argument("--output",type=Path,required=True);parser.add_argument("--fixture",type=Path)
    parser.add_argument("--resolution",choices=["1080p","2160p"],default="2160p");parser.add_argument("--fps",type=int,choices=[30,60],default=60)
    parser.add_argument("--codec",action="append",choices=["h264","hevc"]);parser.add_argument("--bitrate-kbps",type=int,default=20000)
    parser.add_argument("--runs",type=int,default=2);parser.add_argument("--warmup-seconds",type=int,default=1);parser.add_argument("--observation-seconds",type=int,default=8)
    parser.add_argument("--analysis-engine",choices=["python","numpy"],default="python");parser.add_argument("--developer-dir",type=Path)
    parser.add_argument("--ffmpeg",type=Path,default=shutil.which("ffmpeg"));parser.add_argument("--ffprobe",type=Path,default=shutil.which("ffprobe"))
    args=parser.parse_args();args.codec=args.codec or ["h264","hevc"];args.case=[]
    require(sys.platform=="darwin" and re.fullmatch(r"[0-9a-f]{40}",args.revision),"native Mac and full source revision required")
    require(1<=args.runs<=5 and 0<=args.warmup_seconds<=30 and 1<=args.observation_seconds<=300 and args.observation_seconds*args.fps<=18000,"pipeline observation plan exceeds bounds")
    require(250<=args.bitrate_kbps<=20000 and len(set(args.codec))==len(args.codec),"pipeline bitrate/codec request invalid")
    args.width,args.height=(1920,1080) if args.resolution=="1080p" else (3840,2160)
    args.workspace,args.output=args.workspace.resolve(),args.output.resolve()
    require(not args.output.exists() and not args.output.is_relative_to(args.workspace) and (args.workspace/"media/target").resolve().is_relative_to(args.workspace),"new private output and checkout-owned target required")
    require(args.ffmpeg and args.ffprobe,"independent ffmpeg/ffprobe required");args.ffmpeg,args.ffprobe=Path(args.ffmpeg).resolve(),Path(args.ffprobe).resolve()
    if args.fixture:
        require(args.fixture.is_dir() and not args.fixture.is_symlink(),"immutable generated fixture required");args.fixture=args.fixture.resolve()
    os.umask(0o077);args.output.mkdir(mode=0o700,parents=True);(args.output/"private").mkdir(mode=0o700)
    source=args.width*args.height*3//2*args.fps*17
    crops=(args.width-200)*56*3*args.observation_seconds*args.fps*3
    require(shutil.disk_usage(args.output).free>=source+crops+16*1024**3,"insufficient free disk for bounded 4K intermediates and build margin")
    def interrupted(*_):raise KeyboardInterrupt()
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted)
    runner=PipelineRunner(args)
    try:runner.clean();runner.execute();runner.exit_code=0
    except BaseException as error:
        (args.output/"private/failure.txt").write_text(f"{type(error).__name__}: {error}\n");runner.exit_code=130 if isinstance(error,KeyboardInterrupt) else 1
    finally:runner.terminal=True;runner.save()
    print(f"pipeline experiment exit={runner.exit_code}; results: {args.output}",flush=True);return runner.exit_code


if __name__=="__main__":raise SystemExit(main())
