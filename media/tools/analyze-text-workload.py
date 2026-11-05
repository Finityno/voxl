#!/usr/bin/env python3
"""Compare normalized RGB regions from the generated workload, without viewing images.

Run with an external process-group deadline. Decode crops independently first;
this tool neither launches a codec nor changes encoded input color metadata.
"""
import argparse
from collections import Counter
from fractions import Fraction
import hashlib
import json
from pathlib import Path


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def regular(path):
    require(path.is_file() and not path.is_symlink(), "expected regular non-symlink input")


def reference(fixture):
    manifest_path = fixture / "manifest.json"
    regular(manifest_path)
    require(manifest_path.stat().st_size <= 32768, "manifest size bound")
    manifest = json.loads(manifest_path.read_text())
    require(manifest["schemaVersion"] == 1 and manifest["totalFrames"] == 270
        and manifest["warmupFrames"] == 30 and manifest["observedFrames"] == 240
        and manifest["fps"] == 30, "unexpected workload clocks/counts")
    width, height = manifest["width"], manifest["height"]
    require(isinstance(width, int) and isinstance(height, int) and 960 <= width <= 3840
        and 540 <= height <= 2160 and width % 2 == height % 2 == 0, "invalid geometry")
    region = manifest["regions"]["thinColoredText"]
    x, y, crop_width, crop_height = [region[k] for k in ["x", "y", "width", "height"]]
    require(all(isinstance(v, int) and v >= 0 and v % 2 == 0 for v in [x,y,crop_width,crop_height])
        and crop_width > 0 and crop_height > 0 and x+crop_width <= width
        and y+crop_height <= height, "invalid source region")
    require([manifest[k] for k in ["pixelFormat","pixelRange","colorPrimaries","transferFunction","ycbcrMatrix"]]
        == ["nv12","full","bt709","iec61966-2-1","bt709"], "unexpected source interpretation")
    frame_bytes = crop_width*crop_height*3
    for name, expected in [("reference.nv12", width*height*3//2*270), ("reference-thin.rgb",frame_bytes*270)]:
        path = fixture/name
        regular(path)
        record = manifest["files"][name]
        require(path.stat().st_size == record["bytes"] == expected and digest(path) == record["sha256"], "source bytes/hash mismatch")
    with (fixture/"reference-thin.rgb").open("rb") as stream:
        first = stream.read(frame_bytes)
        for _ in range(269):
            require(stream.read(frame_bytes) == first, "thin reference changes; constant-region analysis is invalid")
        require(not stream.read(1), "extra reference frame bytes")
    background = Counter(first[i:i+3] for i in range(0,len(first),3)).most_common(1)[0][0]
    foreground = {i for i in range(crop_width*crop_height) if max(abs(first[3*i+c]-background[c]) for c in range(3)) > 8}
    require(foreground and len(foreground) < crop_width*crop_height, "foreground mask must include ink and exclude background")
    # A one-pixel halo retains blur/chroma bleed beside source strokes. The mask
    # is derived only from the uncompressed RGB reference, never decoded output.
    mask = sorted({yy*crop_width+xx for i in foreground for yy in range(max(0,i//crop_width-1),min(crop_height,i//crop_width+2))
        for xx in range(max(0,i%crop_width-1),min(crop_width,i%crop_width+2))})
    # Independent rational spot checks of RGB->NV12, including saturated strokes.
    samples = sorted({mask[i] for i in range(0,len(mask),max(1,len(mask)//160))} | {0,crop_width*crop_height-1})
    largest = 0
    def quantize(value):
        return min(255,max(0,(value+Fraction(1,2)).numerator//(value+Fraction(1,2)).denominator))
    with (fixture/"reference.nv12").open("rb") as stream:
        for frame in [0,269]:
            offset = frame*width*height*3//2
            for pixel in samples:
                px, py = pixel%crop_width,pixel//crop_width
                red, green, blue = first[3*pixel:3*pixel+3]
                luma = Fraction(2126*red+7152*green+722*blue,10000)
                stream.seek(offset+(y+py)*width+x+px)
                largest = max(largest,abs(stream.read(1)[0]-quantize(luma)))
                left, top = px//2*2,py//2*2
                channels = [sum(first[((top+dy)*crop_width+left+dx)*3+c] for dy in range(2) for dx in range(2)) for c in range(3)]
                red,green,blue = [Fraction(v,4) for v in channels]
                luma = (2126*red+7152*green+722*blue)/10000
                expected = [quantize(128+(blue-luma)*Fraction(10000,18556)),quantize(128+(red-luma)*Fraction(10000,15748))]
                stream.seek(offset+width*height+(y+top)//2*width+x+left)
                actual = stream.read(2)
                largest = max(largest,*(abs(a-b) for a,b in zip(actual,expected)))
    require(largest <= 1, "source conversion differs from independent rational matrix check")
    return manifest, first, mask, {"backgroundRgb":list(background),"sourceInkPixels":len(foreground),"evaluatedPixelsIncludingHalo":len(mask),
        "totalRegionPixels":crop_width*crop_height,"contrastThresholdCodes":8,"haloPixels":1,"independentMatrixMaxCodeDifference":largest,
        "matrixSamplesPerCheckedFrame":len(samples),"checkedSourceFrames":[0,269],"staticReferenceFramesVerified":270}


def compare(path, original, mask, frames=240, capture_warmup=30, engine="python"):
    regular(path)
    size = len(original)
    require(path.stat().st_size == size*frames, "decoded region must contain exactly the requested observed frame count")
    offsets = [pixel*3 for pixel in mask]
    if engine == "numpy":
        import numpy as np
        selected = np.asarray(mask, dtype=np.intp)
        expected = np.frombuffer(original, dtype=np.uint8).reshape(-1,3)[selected].astype(np.int16)
    rows = []
    with path.open("rb") as stream:
        for index in range(frames):
            frame = stream.read(size)
            if engine == "numpy":
                # Widen before subtraction; uint8 would wrap and understate
                # errors. Integer sums preserve the original exact comparison.
                errors = np.abs(np.frombuffer(frame, dtype=np.uint8).reshape(-1,3)[selected].astype(np.int16)-expected)
                sums = [int(value) for value in errors.sum(axis=0,dtype=np.uint64)]
                maximum = int(errors.max())
            else:
                sums, maximum = [0,0,0],0
                for pixel in offsets:
                    for channel in range(3):
                        error = abs(frame[pixel+channel]-original[pixel+channel])
                        sums[channel] += error
                        maximum = max(maximum,error)
            rows.append({"observedFrame":index,"sourceFrame":30+index%240,"sumAbsoluteCodeErrorByRgbChannel":sums,
                "meanAbsoluteForegroundRgbCodeError":sum(sums)/(3*len(mask)),"maxAbsoluteForegroundCodeError":maximum})
            if frames != 240 or capture_warmup != 30:
                rows[-1]["captureFrame"] = capture_warmup+index
        require(not stream.read(1), "unexpected decoded bytes")
    errors = sorted(row["meanAbsoluteForegroundRgbCodeError"] for row in rows)
    return {"sha256":digest(path),"frames":frames,"meanAbsoluteForegroundRgbCodeError":sum(errors)/frames,
        "frameMeanErrorP95":errors[(frames*95+99)//100-1],"worstFrameMeanError":errors[-1],"rows":rows}


def nonregression(rows, baseline):
    require(len(rows) == len(baseline), "candidate and baseline frame counts differ")
    worse = [i for i,(candidate,base) in enumerate(zip(rows,baseline))
        if sum(candidate["sumAbsoluteCodeErrorByRgbChannel"]) > sum(base["sumAbsoluteCodeErrorByRgbChannel"])]
    # Trading one color channel's aggregate error against another is reported
    # separately. Neither result proves that every individual pixel is no worse.
    channels = {channel:[i for i,(candidate,base) in enumerate(zip(rows,baseline))
        if candidate["sumAbsoluteCodeErrorByRgbChannel"][c] > base["sumAbsoluteCodeErrorByRgbChannel"][c]] for c,channel in enumerate(["r","g","b"])}
    return {"foregroundErrorWorseThanBaselineFrames":worse,"sameOrLowerAggregateForegroundErrorEveryFrame":not worse,
        "foregroundChannelErrorWorseThanBaselineFrames":channels,"sameOrLowerAggregateChannelErrorEveryFrame":not any(channels.values())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", required=True, type=Path)
    parser.add_argument("--decoded", action="append", required=True, help="unique-label=normalized-observed-region.rgb")
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--floor", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--observed-frames", type=int, default=240, help="1..18000; long input repeats the source's 240 observed frames")
    parser.add_argument("--capture-warmup-frames", type=int, default=30, help="0..1800; source file still has its fixed 30-frame prefix")
    parser.add_argument("--engine", choices=["python","numpy"], default="python", help="optional exact-integer NumPy acceleration; no metric/tolerance change")
    args = parser.parse_args()
    require(1 <= args.observed_frames <= 18000 and 0 <= args.capture_warmup_frames <= 1800, "observation/capture clock exceeds bounds")
    require(2 <= len(args.decoded) <= 8, "expected two to eight decoded variants")
    variants = {}
    for value in args.decoded:
        label, separator, path = value.partition("=")
        require(separator and label and len(label) <= 80 and label not in variants, "invalid or duplicate variant")
        variants[label] = Path(path)
    require(args.baseline in variants and args.floor in variants and args.baseline != args.floor, "baseline/floor must identify distinct variants")
    manifest, original, mask, calibration = reference(args.fixture)
    results = {label:compare(path,original,mask,args.observed_frames,args.capture_warmup_frames,args.engine) for label,path in variants.items()}
    baseline = results[args.baseline]["rows"]
    for label,value in results.items():
        if label == args.floor:
            continue
        value.update(nonregression(value["rows"],baseline))
    result = {"schemaVersion":1,"manifestSha256":digest(args.fixture/"manifest.json"),"geometry":[manifest["width"],manifest["height"]],
        "region":manifest["regions"]["thinColoredText"],"baseline":args.baseline,"uncompressed420Floor":args.floor,
        "calibration":calibration,"variants":results,"scope":"Reference-derived ink plus one-pixel halo, absolute original sRGB code errors per frame. Full ROI SSIM and original RGB/source-subsampling controls remain separate. Pointwise comparison uses exact integer error sums, not a tolerance or average-only nonregression claim. One synthetic clip is not codec/resource admission."}
    result["frameMapping"] = {"captureWarmupFrames":args.capture_warmup_frames,"sourcePrefixFrames":30,"sourceObservedCycleFrames":240,"observedFrames":args.observed_frames}
    result["analysisEngine"] = args.engine
    with args.output.open("x") as stream:
        json.dump(result,stream,indent=2)
        stream.write("\n")
    print(json.dumps({"output":str(args.output),"evaluatedPixels":len(mask),"framesPerVariant":args.observed_frames}))


if __name__ == "__main__":
    main()
