#!/usr/bin/env python3
"""Bounded native Mac encoder comparison with a reusable generated coding fixture.

No screen/window/input capture. Decoded crops and bitstreams remain private.
Metrics do not admit a production codec or certify complete endpoint performance.
"""
import argparse
import ast
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write(path, value):
    temporary = path.with_suffix(path.suffix+".tmp")
    with temporary.open("w") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write("\n")
    temporary.replace(path)


def group_exists(group):
    try:
        os.killpg(group, 0)
        return True
    except ProcessLookupError:
        return False


def stop(process):
    actions = []
    if group_exists(process.pid):
        os.killpg(process.pid, signal.SIGTERM)
        actions.append("SIGTERM")
        try:
            process.wait(timeout=4)
        except subprocess.TimeoutExpired:
            pass
        if group_exists(process.pid):
            os.killpg(process.pid, signal.SIGKILL)
            actions.append("SIGKILL")
        process.wait(timeout=4)
    until = time.monotonic()+4
    while group_exists(process.pid) and time.monotonic() < until:
        time.sleep(.05)
    require(not group_exists(process.pid), "owned process group did not end")
    return actions


def parse_ssim(path, frames, fps=30):
    require(type(fps) is int and fps in [30,60], "unsupported SSIM source cadence")
    rows = []
    for line in path.read_text().splitlines():
        values = dict(re.findall(r"([A-Za-z]+):([-+0-9.eE]+|inf)", line))
        require(int(values.pop("n")) == len(rows)+1, "SSIM row sequence changed")
        scores = {key:float(value) for key,value in values.items() if key in ["Y","U","V","All"]}
        require(set(scores) == {"Y","U","V","All"} and all(math.isfinite(value) and -1 <= value <= 1 for value in scores.values()), "invalid SSIM channels/value")
        rows.append(scores)
    require(len(rows) == frames, "independent SSIM frame count mismatch")
    phases = ["still","typing","scrolling","window-movement"]
    result = {"frames":frames,"channels":{}}
    for channel in ["Y","U","V","All"]:
        samples = [row[channel] for row in rows]
        phase_values = {name:[row[channel] for index,row in enumerate(rows) if index%(fps*8)//(fps*2) == phase] for phase,name in enumerate(phases)}
        result["channels"][channel] = {"mean":sum(samples)/frames,"minimum":min(samples),"phaseMeans":{name:sum(values)/len(values) if values else None for name,values in phase_values.items()},"phaseFrames":{name:len(values) for name,values in phase_values.items()}}
    return result


def access_units(path, codec, total, warmup, fps):
    sizes, keys = [], []
    with path.open("rb") as stream:
        require(stream.read(6) == b"VXFR\x01"+bytes([0 if codec == "h264" else 1]), "framed stream header mismatch")
        for index in range(total):
            header = stream.read(13)
            require(len(header) == 13, "truncated frame header")
            pts = int.from_bytes(header[:8], "big", signed=True)
            flag, size = header[8], int.from_bytes(header[9:], "big")
            require(flag in [0,1] and 0 < size <= 8*1024*1024 and pts == index*1_000_000//fps, "frame clock/length mismatch")
            data = stream.read(size)
            require(len(data) == size, "truncated frame payload")
            types, offset = [], 0
            while True:
                offset = data.find(b"\0\0\0\1", offset)
                if offset < 0:
                    break
                require(offset+4 < len(data), "empty NAL")
                types.append(data[offset+4]&31 if codec == "h264" else (data[offset+4]>>1)&63)
                offset += 4
            key = 5 in types if codec == "h264" else any(16 <= value <= 21 for value in types)
            require(bool(flag) == key and (index%(fps*2) != 0 or key), "keyframe mismatch")
            if key:
                require(set([7,8] if codec == "h264" else [32,33,34]) <= set(types), "keyframe lacks parameter sets")
                keys.append(index)
            sizes.append(size)
        require(not stream.read(1), "extra encoded frames")
    observed = sizes[warmup:]
    windows = [sum(observed[index:index+fps])*8/1_000_000 for index in range(0,len(observed),fps) if len(observed[index:index+fps]) == fps]
    require(windows, "need a complete one-second observation window")
    return {"totalFrames":total,"observedFrames":len(observed),"keyframeIndices":keys,"totalAnnexBBytes":sum(sizes),"observedAnnexBBytes":sum(observed),"meanAnnexBMbps":sum(observed)*8*fps/(len(observed)*1_000_000),"p95OneSecondAnnexBMbps":sorted(windows)[math.ceil(len(windows)*.95)-1],"peakOneSecondAnnexBMbps":max(windows),"firstFrameAnnexBBytes":sizes[0],"observedFrameBytes":observed}


def profile(specification):
    parts = specification.split(":")
    require(3 <= len(parts) <= 5, "invalid native profile shape")
    codec,tuning,bitrate = parts[:3]
    require(codec in ["h264","hevc"] and tuning in ["compatible","constrained-high","low-latency-compatible","low-latency-constrained-high"] and bitrate.isdigit() and 250 <= int(bitrate) <= 20000, "invalid native profile")
    require(codec == "h264" or tuning in ["compatible","low-latency-compatible"], "HEVC uses compatible Main profile")
    options=parts[3:]
    priority=options.pop(0) if options and options[0] in ["speed","quality"] else "default"
    require(not options or len(options)==1 and re.fullmatch(r"qp([1-9]|[1-4][0-9]|5[01])",options[0]), "explicit options must be speed/quality followed by optional qp1..qp51; omit defaults")
    maximum_qp=int(options[0][2:]) if options else None
    return codec,tuning,int(bitrate),priority,maximum_qp


def pipeline_evidence(text, expected_depth, total, observed):
    require(type(expected_depth) is int and expected_depth in [1,2] and 1 <= observed <= total <= 19800, "invalid pipeline observation request")
    match = re.search(r"^pipeline_depth=(\d+) maximum_pending=(\d+) submitted_frames=(\d+) completed_frames=(\d+) output_order=verified$", text, re.M)
    require(match is not None and int(match[1]) == expected_depth and 1 <= int(match[2]) <= expected_depth and list(map(int, match.group(3,4))) == [total,total], "pipeline bound/order/count mismatch")
    arrival_match = re.search(r"^output_arrivals_ms=(\[.*\])$", text, re.M)
    late_match = re.search(r"^submission_lateness_ms=(\[.*\])$", text, re.M)
    require(arrival_match is not None and late_match is not None, "missing pipeline timing samples")
    arrivals, late = ast.literal_eval(arrival_match[1]), ast.literal_eval(late_match[1])
    require(len(arrivals) == len(late) == observed and all(type(v) in (int,float) and math.isfinite(v) and v >= 0 for v in arrivals+late), "invalid pipeline timing samples")
    require(all(after > before for before, after in zip(arrivals, arrivals[1:])), "pipeline output receipt clock did not advance")
    interval = (arrivals[-1]-arrivals[0])/1000 if observed > 1 else None
    return {"requestedDepth":expected_depth,"maximumPending":int(match[2]),"submittedFrames":total,"completedFrames":total,
        "observedOutputArrivalsMs":arrivals,"submissionLatenessMs":late,"outputFirstToLastSeconds":interval,
        "observedCompletionFps":(observed-1)/interval if interval is not None else None,
        "scope":"Bounded ordered output receipts in the probe process. Includes producer drain scheduling; not native callback timestamps, application playback cadence or end-to-end latency."}


class Runner:
    def __init__(self, args):
        self.args, self.steps, self.cases = args, [], []
        self.fps = getattr(args,"fps",30)
        self.width, self.height = getattr(args,"width",1920), getattr(args,"height",1080)
        self.terminal, self.exit_code = False, None
        self.environment = os.environ | {"CARGO_TARGET_DIR":str(args.workspace/"media/target"),"CARGO_BUILD_JOBS":"2","CARGO_BUILD_WARNINGS":"deny","CARGO_INCREMENTAL":"0","RUSTC_WRAPPER":"","CI":"true","GIT_OPTIONAL_LOCKS":"0","PYTHONDONTWRITEBYTECODE":"1","LC_ALL":"C","OMP_NUM_THREADS":"1","OPENBLAS_NUM_THREADS":"1"}
        if args.developer_dir:
            self.environment["DEVELOPER_DIR"] = str(args.developer_dir)

    def clean(self):
        head = subprocess.check_output(["git","rev-parse","HEAD"],cwd=self.args.workspace,text=True,timeout=10).strip()
        dirty = subprocess.check_output(["git","status","--porcelain"],cwd=self.args.workspace,text=True,timeout=10).strip()
        require(head == self.args.revision and not dirty, "source must stay clean at the exact revision")

    def save(self):
        expected = [f"run-{run+1}-{case.replace(':','-')}" for run in range(self.args.runs) for case in self.args.case]
        value = {"source":self.args.revision,"helperSha256":digest(__file__),"terminal":self.terminal,"exitCode":self.exit_code,"steps":self.steps,"cases":self.cases,"unexecutedCases":[name for name in expected if name not in {case["name"] for case in self.cases if case.get("oraclesComplete")}],"plan":{"fps":self.fps,"warmupSeconds":self.args.warmup_seconds,"observationSeconds":self.args.observation_seconds,"independentRuns":self.args.runs,"cases":self.args.case},"productionAdmission":False,"unsupportedCurrentNativeProfiles":["av1"],"limits":"Generated native encoder/file-input comparison only. No capture/control/transport, actual wire/relay traffic, OS helper/GPU/power/baseline-subtracted app CPU or physical endpoints. CPU/RSS cover the probe process including I/O, initialization and warmup. Metric comparisons are not complete text/codec admission."}
        write(self.args.output/"results.json",value)

    def run(self, name, command, seconds, locked=False):
        self.clean()
        require(shutil.disk_usage(self.args.output).free >= 12*1024**3, "need 12 GiB free for bounded private intermediates")
        if locked:
            command = ["/usr/bin/lockf","-k","-t","45","/tmp/fincode-build.lock","/usr/bin/nice","-n","19",*command]
            seconds += 50
        else:
            command = ["/usr/bin/nice","-n","19",*command]
        log = self.args.output/"private"/(name+".log")
        started, pending, timed = time.monotonic(), None, False
        with log.open("x") as stream:
            process = subprocess.Popen(command,cwd=self.args.workspace,env=self.environment,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
            write(self.args.output/"active-group.json",{"pgid":process.pid,"name":name,"command":command})
            try:
                code = process.wait(timeout=seconds)
            except subprocess.TimeoutExpired:
                code, timed = 124, True
            except BaseException as error:
                code, pending = 130, error
            finally:
                cleanup = stop(process)
        self.clean()
        self.steps.append({"name":name,"command":command,"deadlineSeconds":seconds,"exitCode":code,"childExitCode":process.returncode,"seconds":round(time.monotonic()-started,3),"pgid":process.pid,"groupGone":not group_exists(process.pid),"timedOut":timed,"cleanupSignals":cleanup,"privateLog":str(log.relative_to(self.args.output)),"logSha256":digest(log),"sourceCleanBefore":True,"sourceCleanAfter":True})
        (self.args.output/"active-group.json").unlink()
        self.save()
        print(name+" exit="+str(code),flush=True)
        if pending:
            raise pending
        require(code == 0 and not cleanup, "command failed, timed out or needed unexpected descendant cleanup: "+name)
        return log

    def build(self):
        for name,command in [("load",["uptime"]),("os",["sw_vers"]),("cpu",["sysctl","-n","machdep.cpu.brand_string"]),("model",["sysctl","-n","hw.model"]),("toolchain",["cargo","--version"]),("ffmpeg-version",[str(self.args.ffmpeg),"-version"]),("ffprobe-version",[str(self.args.ffprobe),"-version"])]:
            self.run(name,command,15)
        common = ["--locked","--manifest-path","media/Cargo.toml","--release","--example","codec_probe","-j2","-q"]
        build = self.run("build",["cargo","build",*common,"--message-format=json"],600,True)
        self.run("clippy",["cargo","clippy","--locked","--manifest-path","media/Cargo.toml","--lib","--example","codec_probe","-j2","-q","--message-format=short","--","-D","warnings"],300,True)
        artifacts = [json.loads(line) for line in build.read_text().splitlines() if line.startswith("{")]
        artifacts = [row for row in artifacts if row.get("reason") == "compiler-artifact" and row.get("target",{}).get("name") == "codec_probe" and row.get("executable")]
        require(len(artifacts) == 1, "expected one exact codec executable")
        executable = Path(artifacts[0]["executable"]).resolve()
        require(executable.parent == self.args.workspace/"media/target/release/examples", "unexpected target location")
        write(self.args.output/"compiler-artifact.json",{"source":self.args.revision,"artifact":artifacts[0],"executableSha256":digest(executable),"privateBuildLogSha256":digest(build)})
        return executable

    def fixture(self):
        fixture = self.args.fixture
        if fixture is None:
            generator = self.args.output/"private/desktop-workload"
            self.run("generator-build",["xcrun","--sdk","macosx","swiftc","-O","-swift-version","6","-strict-concurrency=complete","-warnings-as-errors","media/tools/desktop-workload.swift","-o",str(generator)],120,True)
            fixture = self.args.output/"private/fixture"
            self.run("generate",[str(generator),str(fixture),str(self.width),str(self.height),str(self.fps)],180)
        require(fixture.is_dir() and not fixture.is_symlink(), "expected an owned generated fixture directory")
        import importlib.util
        path = self.args.workspace/"media/tools/analyze-text-workload.py"
        spec = importlib.util.spec_from_file_location("text_reference",path)
        metrics = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(metrics)
        manifest, _, _, validation = metrics.reference(fixture)
        require(manifest["width"] == self.width and manifest["height"] == self.height and manifest["fps"] == self.fps, "fixture geometry/cadence must match the requested profile")
        raw = fixture/"reference.nv12"
        font = Path(manifest["fontURL"])
        require(font == Path("/System/Library/Fonts/Menlo.ttc") and manifest["fontPostScriptName"] == "Menlo-Regular", "unexpected installed font")
        write(self.args.output/"fixture.json",{"manifest":manifest,"manifestSha256":digest(fixture/"manifest.json"),"validation":validation,"currentGeneratorSourceSha256":digest(self.args.workspace/"media/tools/desktop-workload.swift"),"currentlyInstalledFontSha256":digest(font),"reuse":self.args.fixture is not None,"scope":"Reused assets are identified by their byte hashes; current source/font hashes do not establish their historical generation environment."})
        observation = self.args.output/"private/observation-reference.nv12"
        prefix = self.width*self.height*3//2*manifest["warmupFrames"]
        with raw.open("rb") as source, observation.open("xb") as destination:
            source.seek(prefix)
            shutil.copyfileobj(source,destination,1024*1024)
        require(observation.stat().st_size == self.width*self.height*3//2*manifest["observedFrames"], "observed reference length mismatch")
        return fixture, manifest, observation

    def source_args(self):
        return ["-f","rawvideo","-pixel_format","nv12","-video_size",f"{self.width}x{self.height}","-framerate",str(self.fps),"-color_range","pc","-colorspace","bt709","-color_primaries","bt709","-color_trc","iec61966-2-1"]

    def rgb(self, name, path, manifest, output, raw=False):
        frames, warmup = self.args.observation_seconds*self.fps, self.args.warmup_seconds*self.fps
        region = manifest["regions"]["thinColoredText"]
        command = [str(self.args.ffmpeg),"-nostdin","-hide_banner","-loglevel","error","-threads","2","-filter_threads","1"]
        if raw:
            command += ["-stream_loop","-1",*self.source_args()]
        command += ["-i",str(path)]
        trim = f"trim=end_frame={frames}" if raw else f"trim=start_frame={warmup}:end_frame={warmup+frames}"
        crop = "crop="+":".join(str(region[key]) for key in ["width","height","x","y"])
        graph = trim+f",settb=1/{self.fps},setpts=N,"+crop+",colorspace=all=bt709:trc=srgb:range=pc:format=yuv444p:dither=none,format=rgb24"
        command += ["-vf",graph,"-frames:v",str(frames),"-fps_mode","passthrough","-f","rawvideo",str(output)]
        self.run(name,command,180)
        require(output.stat().st_size == region["width"]*region["height"]*3*frames, "normalized RGB frame count mismatch")

    def case(self, executable, specification, run, fixture, manifest, observation, *, pipeline_depth=1, name=None, native_deadline_seconds=None):
        codec, tuning, bitrate, priority, maximum_qp = profile(specification)
        name = name or f"run-{run}-{specification.replace(':','-')}"
        folder = self.args.output/"private"/name
        folder.mkdir(mode=0o700)
        video, framed, rgb = folder/("encoded."+codec), folder/"frames.vxfr", folder/"thin.rgb"
        frames, warmup = self.args.observation_seconds*self.fps, self.args.warmup_seconds*self.fps
        before = digest(fixture/"reference.nv12")
        command = ["/usr/bin/time","-lp",str(executable),"hardware",str(video),codec,str(framed),f"--warmup-frames={warmup}",f"--frames={frames}",f"--width={self.width}",f"--height={self.height}",f"--fps={self.fps}","--pixel-range=full","--color-tags=srgb","--input-nv12="+str(fixture/"reference.nv12"),"--input-cycle-prefix="+str(manifest["warmupFrames"]),"--tuning="+tuning,"--priority="+priority,"--bitrate-kbps="+str(bitrate),f"--force-keyframe-period={self.fps*2}"]
        command.append("--pipeline-depth="+str(pipeline_depth))
        if maximum_qp is not None:command.append("--max-frame-qp="+str(maximum_qp))
        log = self.run(name,command,native_deadline_seconds if native_deadline_seconds is not None else self.args.warmup_seconds+self.args.observation_seconds+20,True)
        text = log.read_text()
        samples = ast.literal_eval(re.search(r"^encode_samples_ms=(\[.*\])$",text,re.M).group(1))
        require(len(samples) == frames and all(isinstance(v,(int,float)) and math.isfinite(v) and 0 <= v <= 2500 for v in samples), "encode sample count/value mismatch")
        summary = re.search(r"^codec=(H264|Hevc) hardware=(true|false) width=(\d+) height=(\d+) fps=(\d+) target_bitrate_kbps=(\d+) frames=(\d+) warmup_frames=(\d+) .+ wall_seconds=([0-9.]+)",text,re.M)
        require(summary is not None and summary.group(1) == ("H264" if codec == "h264" else "Hevc") and summary.group(2) == "true" and list(map(int,summary.group(3,4,5,6,7,8))) == [self.width,self.height,self.fps,bitrate,frames,warmup], "native requested/observed profile mismatch")
        pipeline_observation = pipeline_evidence(text,pipeline_depth,warmup+frames,frames)
        acceleration = re.search(r"^hardware_selection=(\w+)$",text,re.M).group(1)
        require(acceleration in ["QueriedHardware","RequiredHardware"], "hardware selection not established")
        actual_priority = re.search(r"^encoding_priority=(Default|Speed|Quality) priority_property_verified=(true|false)$",text,re.M)
        require(actual_priority is not None and actual_priority.group(1).lower() == priority and (actual_priority.group(2) == "true") == (priority != "default"), "encoding priority readback does not match the requested mode")
        actual_qp=re.search(r"^max_frame_qp=(\d+) qp_property_verified=true$",text,re.M)
        require((actual_qp is None if maximum_qp is None else actual_qp is not None and int(actual_qp.group(1))==maximum_qp), "maximum frame QP readback does not match the requested limit")
        units = access_units(framed,codec,warmup+frames,warmup,self.fps)
        require(units["totalAnnexBBytes"] == video.stat().st_size and digest(fixture/"reference.nv12") == before, "stream or source changed")
        cpu = {key:float(re.search(r"^"+key+r"\s+([0-9.]+)$",text,re.M).group(1)) for key in ["real","user","sys"]}
        rss = int(re.search(r"^\s*(\d+)\s+maximum resident set size",text,re.M).group(1))
        require(cpu["real"] > 0 and rss > 0, "missing process CPU/RSS")
        decode = self.run(name+"-decode",[str(self.args.ffprobe),"-v","error","-threads","2","-count_frames","-select_streams","v:0","-show_entries","stream=codec_name,profile,pix_fmt,width,height,nb_read_frames,color_range,color_space,color_transfer,color_primaries","-of","json",str(video)],90)
        streams = json.loads(decode.read_text())["streams"]
        require(len(streams) == 1, "unexpected decoded stream count")
        metadata = streams[0]
        require(metadata["codec_name"] == codec and metadata["width"] == self.width and metadata["height"] == self.height and int(metadata["nb_read_frames"]) == warmup+frames and [metadata[k] for k in ["color_range","color_space","color_transfer","color_primaries"]] == ["pc","bt709","iec61966-2-1","bt709"], "independent decode/format mismatch")
        graph = f"[0:v]trim=start_frame={warmup}:end_frame={warmup+frames},settb=1/{self.fps},setpts=N,scale=out_range=pc:flags=accurate_rnd+bitexact,format=yuv420p,split=3[a0][a1][a2];[1:v]trim=end_frame={frames},settb=1/{self.fps},setpts=N,scale=out_range=pc:flags=accurate_rnd+bitexact,format=yuv420p,split=3[b0][b1][b2];[a0][b0]ssim=shortest=1:repeatlast=0:stats_file="+str(folder/"full.ssim")+"[full];"
        for index,label,key in [(1,"text","codeText"),(2,"thin","thinColoredText")]:
            region = manifest["regions"][key]
            crop = "crop="+":".join(str(region[k]) for k in ["width","height","x","y"])
            graph += f"[a{index}]{crop}[ar{index}];[b{index}]{crop}[br{index}];[ar{index}][br{index}]ssim=shortest=1:repeatlast=0:stats_file="+str(folder/(label+".ssim"))+f"[{label}];"
        self.run(name+"-quality",[str(self.args.ffmpeg),"-nostdin","-hide_banner","-loglevel","error","-threads","2","-filter_complex_threads","1","-i",str(video),"-stream_loop","-1",*self.source_args(),"-i",str(observation),"-filter_complex",graph.rstrip(";"),"-map","[full]","-map","[text]","-map","[thin]","-f","null","-"],240)
        quality = {label:parse_ssim(folder/(label+".ssim"),frames,self.fps) for label in ["full","text","thin"]}
        self.rgb(name+"-rgb",video,manifest,rgb)
        wall, nominal = float(summary.group(9)), (warmup+frames-1)/self.fps
        record = {"name":name,"specification":specification,"independentRun":run,"oraclesComplete":False,"encodingAndDecodeOraclesComplete":True,"inputSha256":before,"encodedSha256":digest(video),"framedSha256":digest(framed),"normalizedRgbSha256":digest(rgb),"actualDecodedMetadata":metadata,"hardwareSelection":acceleration,"accessUnits":units,"encodeLatencyMs":{"samples":samples,"p50":sorted(samples)[math.ceil(len(samples)*.5)-1],"p95":sorted(samples)[math.ceil(len(samples)*.95)-1],"maximum":max(samples),"scope":"encode call to output callback received, excludes source read/fill"},"probeWallSeconds":wall,"pacing":{"nominalFirstToLastFrameSeconds":nominal,"wallMinusNominalSeconds":wall-nominal,"scope":"Probe wall includes final drain. Nominal PTS do not prove application presentation cadence or end-to-end latency."},"processResources":{"wallSeconds":cpu["real"],"userSeconds":cpu["user"],"systemSeconds":cpu["sys"],"averageCpuPercentOneCore":100*(cpu["user"]+cpu["sys"])/cpu["real"],"maximumResidentBytes":rss,"scope":"macOS time-lp probe process only; includes file I/O/init/warmup, excludes OS helper/GPU/power and app baseline"},"quality":quality,"rgbPath":str(rgb)}
        record["pipeline"] = pipeline_observation
        record["encodingPriority"] = {"requested":priority,"nativePropertyVerified":priority != "default","scope":"Default leaves the property untouched; explicit values require matching CFBoolean readback. Setter acceptance alone is insufficient; quality/speed gains require separate measurements."}
        record["maximumFrameQp"] = {"requested":maximum_qp,"nativePropertyVerified":maximum_qp is not None,"scope":"Absent leaves the property untouched. Explicit limits require matching integer readback, not an actual per-frame QP observation. Native drops remain terminal and prevent full frame-count/quality qualification."}
        self.cases.append(record)
        write(self.args.output/(name+".json"),record)
        self.save()
        return record

    def execute(self):
        executable = self.build()
        fixture, manifest, observation = self.fixture()
        floor = self.args.output/"private/floor.rgb"
        self.rgb("source-floor",observation,manifest,floor,True)
        for run in range(1,self.args.runs+1):
            records = []
            ordering = self.args.case[(run-1)%len(self.args.case):]+self.args.case[:(run-1)%len(self.args.case)]
            for specification in ordering:
                records.append(self.case(executable,specification,run,fixture,manifest,observation))
            baseline = next(record for record in records if record["specification"] == "h264:compatible:3000")
            output = self.args.output/f"foreground-run-{run}.json"
            command = [sys.executable,"-B","media/tools/analyze-text-workload.py","--fixture",str(fixture),"--baseline",baseline["name"],"--floor","uncompressed420","--decoded","uncompressed420="+str(floor),"--observed-frames",str(self.args.observation_seconds*self.fps),"--capture-warmup-frames",str(self.args.warmup_seconds*self.fps),"--engine",self.args.analysis_engine,"--output",str(output)]
            for record in records:
                command += ["--decoded",record["name"]+"="+record["rgbPath"]]
            self.run(f"foreground-run-{run}",command,300)
            foreground = json.loads(output.read_text())
            require(foreground["frameMapping"] == {"captureWarmupFrames":self.args.warmup_seconds*self.fps,"sourcePrefixFrames":manifest["warmupFrames"],"sourceObservedCycleFrames":manifest["observedFrames"],"observedFrames":self.args.observation_seconds*self.fps} and foreground["baseline"] == baseline["name"] and foreground["manifestSha256"] == digest(fixture/"manifest.json"), "foreground result attribution mismatch")
            for record in records:
                metrics = foreground["variants"][record["name"]]
                require(metrics["sha256"] == record["normalizedRgbSha256"] and metrics["frames"] == self.args.observation_seconds*self.fps, "foreground variant count/hash mismatch")
                record["foregroundAnalysis"] = {"file":output.name,"sha256":digest(output),"meanAbsoluteForegroundRgbCodeError":metrics["meanAbsoluteForegroundRgbCodeError"],"worseAggregateFrames":len(metrics["foregroundErrorWorseThanBaselineFrames"]),"worseAggregateChannelFrames":{key:len(value) for key,value in metrics["foregroundChannelErrorWorseThanBaselineFrames"].items()},"scope":"Exact frame/channel aggregate sums on reference-derived ink+halo; not pixelwise/glyph/visual nonregression or full codec admission."}
                record["oraclesComplete"] = True
            # Crops can be regenerated from the hashed encoded/reference files.
            # Retain all raw numeric rows, SSIM rows and bitstreams; remove only
            # these newly created multi-gigabyte intermediates after analysis.
            for record in records:
                path = Path(record["rgbPath"])
                require(path.is_relative_to(self.args.output/"private") and path.name == "thin.rgb", "unexpected intermediate path")
                path.unlink()
                record["normalizedRgbIntermediateRemovedAfterAnalysis"] = True
                write(self.args.output/(record["name"]+".json"),record)
            self.save()
        floor_hash = digest(floor)
        floor.unlink()
        write(self.args.output/"floor.json",{"sha256":floor_hash,"frames":self.args.observation_seconds*self.fps,"regenerableIntermediateRemoved":True,"scope":"Uncompressed 4:2:0 reconstruction control, not additive codec loss or hardware decode proof"})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace",type=Path,default=Path(__file__).resolve().parents[2])
    parser.add_argument("--revision",required=True)
    parser.add_argument("--output",required=True,type=Path)
    parser.add_argument("--fixture",type=Path,help="reuse an immutable generated 1080p fixture at the selected cadence; hashes are validated")
    parser.add_argument("--fps",type=int,choices=[30,60],default=30)
    parser.add_argument("--case",action="append",help="codec:tuning:kbps[:speed|quality][:qp1..qp51]; h264:compatible:3000 is mandatory baseline")
    parser.add_argument("--warmup-seconds",type=int,default=30)
    parser.add_argument("--observation-seconds",type=int,default=300)
    parser.add_argument("--runs",type=int,default=5)
    parser.add_argument("--analysis-engine",choices=["python","numpy"],default="python")
    parser.add_argument("--developer-dir",type=Path)
    parser.add_argument("--ffmpeg",type=Path,default=shutil.which("ffmpeg"))
    parser.add_argument("--ffprobe",type=Path,default=shutil.which("ffprobe"))
    args = parser.parse_args()
    require(sys.platform == "darwin", "native codec matrix requires macOS")
    require(re.fullmatch(r"[0-9a-f]{40}",args.revision), "full source revision required")
    require(0 <= args.warmup_seconds <= 60 and 1 <= args.observation_seconds <= 600 and 1 <= args.runs <= 5, "observation plan exceeds bounds")
    require(args.observation_seconds*args.fps <= 18000 and args.warmup_seconds*args.fps <= 1800, "probe frame budget exceeded")
    args.workspace, args.output = args.workspace.resolve(), args.output.resolve()
    require(not args.output.is_relative_to(args.workspace) and not args.output.exists(), "new output must be outside the source checkout")
    require((args.workspace/"media/target").resolve().is_relative_to(args.workspace), "target must belong to this checkout")
    require(args.ffmpeg and args.ffprobe, "independent FFmpeg/ffprobe required")
    args.ffmpeg, args.ffprobe = Path(args.ffmpeg).resolve(), Path(args.ffprobe).resolve()
    if args.fixture:
        require(args.fixture.is_dir() and not args.fixture.is_symlink(), "expected a non-symlink generated fixture directory")
        args.fixture = args.fixture.resolve()
    args.case = args.case or ["h264:compatible:3000","hevc:compatible:1500"]
    require(2 <= len(args.case) <= 4 and len(set(args.case)) == len(args.case) and "h264:compatible:3000" in args.case, "two to four unique cases including the baseline required")
    for specification in args.case:
        profile(specification)
    os.umask(0o077)
    args.output.mkdir(mode=0o700,parents=True)
    (args.output/"private").mkdir(mode=0o700)
    # Requested bitrate is not an output cap. Reserve three times it for both
    # retained stream copies, plus all crops in one run and a free-space margin.
    retained = sum(int(value.split(":")[2])*1000/8 for value in args.case)*2*3*(args.warmup_seconds+args.observation_seconds)*args.runs
    crops = 1720*56*3*args.observation_seconds*args.fps*(len(args.case)+1)
    require(shutil.disk_usage(args.output).free >= retained+crops+2*1024**3+12*1024**3, "insufficient free disk for planned retained outputs and private intermediates")
    def interrupted(signum,frame):
        raise KeyboardInterrupt()
    signal.signal(signal.SIGTERM,interrupted)
    signal.signal(signal.SIGINT,interrupted)
    runner = Runner(args)
    try:
        runner.clean()
        runner.execute()
        runner.exit_code = 0
    except BaseException as error:
        (args.output/"private/failure.txt").write_text(f"{type(error).__name__}: {error}\n")
        print("matched native comparison failed: "+type(error).__name__,flush=True)
        runner.exit_code = 130 if isinstance(error,KeyboardInterrupt) else 1
    finally:
        runner.terminal = True
        runner.save()
    return runner.exit_code


if __name__ == "__main__":
    raise SystemExit(main())
