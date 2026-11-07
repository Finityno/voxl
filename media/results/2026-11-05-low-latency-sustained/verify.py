"""Recompute this package from its retained numeric records; no native rerun."""
from pathlib import Path
import csv
import gzip
import hashlib
import io
import json
import math
import re
import statistics

ROOT = Path(__file__).resolve().parent
def load(name):
    return json.loads((ROOT / name).read_text())
def equal(actual, expected):
    assert math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-12), (actual, expected)
def rank(values, fraction):
    return sorted(values)[math.ceil(len(values) * fraction) - 1]
def distribution(values, expected):
    assert values == expected['individual']
    equal(statistics.median(values), expected['median'])
    equal(min(values), expected['minimum'])
    equal(max(values), expected['maximum'])
def rows(name):
    return list(csv.DictReader(io.StringIO(gzip.decompress((ROOT / name).read_bytes()).decode())))

index = load('index.json')
for name, record in {**index['jsonFiles'], **index['supportFiles'], **index['compressedFiles']}.items():
    data = (ROOT / name).read_bytes()
    assert len(data) == record['bytes']
    assert hashlib.sha256(data).hexdigest() == record['sha256'], name
    if name in index['compressedFiles']:
        raw = gzip.decompress(data)
        assert len(raw) == record['uncompressedBytes']
        assert hashlib.sha256(raw).hexdigest() == record['uncompressedSha256']
        count = len(raw.splitlines()) - (1 if '.csv.' in name else 0)
        assert count == record['dataRows'], (name, count)
assert len(index['compressedFiles']) == 65
assert {p.name for p in ROOT.iterdir()} == set(index['jsonFiles']) | set(index['supportFiles']) | set(index['compressedFiles']) | {'index.json'}

summary = load('summary.json')
validation = load('validation.json')
assert summary['source'] == 'f098e462a62833b73d6a6103e4f50ff810fb7ffb'
assert summary['productionAdmission'] is False
assert (summary['fps'], summary['warmupSecondsPerSession'], summary['observedSecondsPerSession']) == (30, 30, 300)
assert len(summary['runs']) == 5
assert validation['measurement']['terminal'] and validation['measurement']['exitCode'] == 0
assert len(validation['independentlyAbsentOwnedGroups']) == 55
assert all(s['exitCode'] == 0 and s['groupGone'] and not s['timedOut'] and not s['cleanupSignals'] for s in validation['measurement']['steps'])
counts = {'encodePayload': 0, 'regionSsim': 0, 'foregroundIncludingFloor': 0}
reductions = []
for run in summary['runs']:
    number = run['number']
    foreground = {}
    for variant, stats in run['foregroundVariants'].items():
        data = rows(f'run-{number}-{variant}-foreground.csv.gz')
        assert len(data) == 9000
        sums = []
        for i, row in enumerate(data):
            assert (int(row['observedFrame']), int(row['captureFrame']), int(row['sourceFrame'])) == (i, 900 + i, 30 + i % 240)
            sums.append(tuple(int(row[f'{channel}ErrorSum']) for channel in 'rgb'))
        pixels = run['foreground']['calibration']['evaluatedPixelsIncludingHalo']
        means = [sum(value) / (pixels * 3) for value in sums]
        equal(statistics.mean(means), stats['meanAbsoluteForegroundRgbCodeError'])
        equal(rank(means, .95), stats['frameMeanErrorP95'])
        equal(max(means), stats['worstFrameMeanError'])
        if variant == 'uncompressed420':
            assert 'worseAggregateFrames' not in stats and 'worseChannelFrames' not in stats
        foreground[variant] = sums
        counts['foregroundIncludingFloor'] += len(data)
    cases = {}
    for case in run['cases']:
        codec = case['specification'].split(':')[0]
        cases[codec] = case
        assert case['oraclesComplete']
        assert case['hardwareSelection'] == ('QueriedHardware' if codec == 'h264' else 'RequiredHardware')
        assert case['actualDecodedMetadata']['nb_read_frames'] == '9900'
        prefix = case['numericPrefix']
        data = rows(prefix + '-samples.csv.gz')
        assert len(data) == 9000
        sizes, latency = [], []
        for i, row in enumerate(data):
            assert (int(row['observedFrame']), int(row['captureFrame']), int(row['sourceFrame'])) == (i, 900 + i, 30 + i % 240)
            sizes.append(int(row['annexBBytes']))
            latency.append(float(row['encodeCallbackMs']))
        equal(rank(latency, .5), case['encodeLatencyMs']['p50'])
        equal(rank(latency, .95), case['encodeLatencyMs']['p95'])
        equal(max(latency), case['encodeLatencyMs']['maximum'])
        assert sum(sizes) == case['accessUnits']['observedAnnexBBytes']
        equal(sum(sizes) * 8 / 300 / 1e6, case['accessUnits']['meanAnnexBMbps'])
        windows = [sum(sizes[i:i + 30]) * 8 / 1e6 for i in range(0, 9000, 30)]
        equal(rank(windows, .95), case['accessUnits']['p95OneSecondAnnexBMbps'])
        equal(max(windows), case['accessUnits']['peakOneSecondAnnexBMbps'])
        counts['encodePayload'] += len(data)
        for region in ('full', 'text', 'thin'):
            raw = gzip.decompress((ROOT / (prefix + '-' + region + '.ssim.gz')).read_bytes()).decode().splitlines()
            assert len(raw) == 9000
            for channel, stats in case['quality'][region]['channels'].items():
                values = [float(re.search(r'\b' + channel + r':([^\s]+)', line)[1]) for line in raw]
                equal(statistics.mean(values), stats['mean'])
                equal(min(values), stats['minimum'])
                for phase, label in enumerate(('still', 'typing', 'scrolling', 'window-movement')):
                    selected = [v for i, v in enumerate(values) if (i % 240) // 60 == phase]
                    assert len(selected) == stats['phaseFrames'][label]
                    equal(statistics.mean(selected), stats['phaseMeans'][label])
            counts['regionSsim'] += len(raw)
        observed = case['additionalRawRusageObservation']
        text = gzip.decompress((ROOT / observed['resourceLogProjection']).read_bytes()).decode()
        assert 'encode_samples_ms=' not in text
        assert int(re.search(r'^\s*(\d+)\s+peak memory footprint\s*$', text, re.M)[1]) == observed['peakProcessPhysicalFootprintBytes']
        resource = case['processResources']
        for field, key in (('real', 'wallSeconds'), ('user', 'userSeconds'), ('sys', 'systemSeconds')):
            equal(float(re.search(r'^' + field + r' ([0-9.]+)$', text, re.M)[1]), resource[key])
        equal(100 * (resource['userSeconds'] + resource['systemSeconds']) / resource['wallSeconds'], resource['averageCpuPercentOneCore'])
        assert int(re.search(r'^\s*(\d+)\s+maximum resident set size\s*$', text, re.M)[1]) == resource['maximumResidentBytes']
        base, current = foreground['h264'], foreground[codec]
        aggregate = sum(sum(v) > sum(b) for v, b in zip(current, base))
        channels = {key: sum(v[i] > b[i] for v, b in zip(current, base)) for i, key in enumerate('rgb')}
        stats = run['foregroundVariants'][codec]
        assert aggregate == stats['worseAggregateFrames'] == case['foregroundAnalysis']['worseAggregateFrames']
        assert channels == stats['worseChannelFrames'] == case['foregroundAnalysis']['worseAggregateChannelFrames']
    reductions.append(1 - cases['hevc']['accessUnits']['observedAnnexBBytes'] / cases['h264']['accessUnits']['observedAnnexBBytes'])
assert counts == summary['rawRowCounts'] == {'encodePayload': 90000, 'regionSsim': 270000, 'foregroundIncludingFloor': 135000}
assert reductions == summary['pairedPayloadReductionFractions']
for codec, stats in summary['summary'].items():
    cases = [next(c for c in run['cases'] if c['specification'].startswith(codec + ':')) for run in summary['runs']]
    for key, values in {
        'meanAnnexBMbps': [c['accessUnits']['meanAnnexBMbps'] for c in cases],
        'individualSessionEncodeP95Ms': [c['encodeLatencyMs']['p95'] for c in cases],
        'wholeProbeCpuPercentOneCore': [c['processResources']['averageCpuPercentOneCore'] for c in cases],
        'wholeProbePeakRssMiB': [c['processResources']['maximumResidentBytes'] / 2**20 for c in cases],
        'wholeProbePeakPhysicalFootprintMiB': [c['additionalRawRusageObservation']['peakProcessPhysicalFootprintBytes'] / 2**20 for c in cases],
        'textRegionMeanYuvSsim': [c['quality']['text']['channels']['All']['mean'] for c in cases],
        'thinForegroundMeanAbsoluteRgbError': [c['foregroundAnalysis']['meanAbsoluteForegroundRgbCodeError'] for c in cases],
    }.items():
        distribution(values, stats[key])
print(json.dumps({'verifiedCompressedFiles': 65, 'verifiedNumericRows': counts, 'pairedPayloadReductionFractions': reductions, 'nativeCommandsRerun': 0}))
