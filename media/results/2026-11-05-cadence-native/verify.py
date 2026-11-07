"""Check retained 60fps numeric evidence without encoding or decoding again."""
from pathlib import Path
import gzip
import hashlib
import json
import math
import re
import statistics

ROOT = Path(__file__).resolve().parent
def load(name):
    raw = (ROOT / name).read_bytes()
    return json.loads(gzip.decompress(raw) if name.endswith('.gz') else raw)
def equal(actual, expected):
    assert math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-12), (actual, expected)
def rank(values, fraction):
    return sorted(values)[math.ceil(len(values) * fraction) - 1]

index = load('index.json')['files']
assert {str(p.relative_to(ROOT)) for p in ROOT.rglob('*') if p.is_file()} == set(index) | {'index.json'}
for name, item in index.items():
    data = (ROOT / name).read_bytes()
    assert len(data) == item['bytes'] and hashlib.sha256(data).hexdigest() == item['sha256'], name
for name, item in load('provenance.json')['exports'].items():
    raw = (ROOT / name).read_bytes()
    data = gzip.decompress(raw) if name.endswith('.gz') else raw
    assert len(data) == item['normalizedUncompressedBytes']
    assert hashlib.sha256(data).hexdigest() == item['normalizedUncompressedSha256']
summary = load('summary.json')
assert summary['source'] == '1f94f7618dc3e11abb15ad858112c8f4686a2320'
assert summary['productionAdmission'] is False and len(summary['independentlyAbsentOwnedGroups']) == 50
generation = load('generation/results.json')
assert generation['terminal'] and generation['exitCode'] == 0 and len(generation['steps']) == 8
for step in generation['steps']:
    assert step['exitCode'] == (1 if step['name'].startswith('refuse-') else 0)
    assert step['groupGone'] and not step['timedOut'] and not step['cleanupSignals']
assets = load('generation/asset-validation.json')
assert all(v['byteIdentical'] and v['originalSha256'] == v['currentSha256'] for v in assets['default30ByteControls'].values())
assert assets['source60Clock'] == {'fps': 60, 'prefix': 60, 'cycle': 480, 'total': 540}
assert len(set(assets['consecutiveMotionFrameHashes'])) == 4
total_cases = 0
for batch in ('pilot12', 'budgets'):
    result = load(batch + '/results.json')
    assert result['terminal'] and result['exitCode'] == 0 and not result['unexecutedCases']
    assert result['plan']['fps'] == 60 and result['plan']['warmupSeconds'] == 1 and result['plan']['observationSeconds'] == 8
    assert all(s['exitCode'] == 0 and s['groupGone'] and not s['timedOut'] and not s['cleanupSignals'] for s in result['steps'])
    foreground = load(batch + '/foreground.json.gz')
    assert foreground['frameMapping'] == {'captureWarmupFrames': 60, 'sourcePrefixFrames': 60, 'sourceObservedCycleFrames': 480, 'observedFrames': 480}
    baseline = foreground['variants'][foreground['baseline']]['rows']
    for name, variant in foreground['variants'].items():
        rows = variant['rows']
        assert len(rows) == 480
        means = []
        for i, row in enumerate(rows):
            assert (row['observedFrame'], row['captureFrame'], row['sourceFrame']) == (i, i + 60, i + 60)
            means.append(sum(row['sumAbsoluteCodeErrorByRgbChannel']) / (3 * foreground['calibration']['evaluatedPixelsIncludingHalo']))
        equal(statistics.mean(means), variant['meanAbsoluteForegroundRgbCodeError'])
        if name != 'uncompressed420':
            aggregate = [i for i, (v, b) in enumerate(zip(rows, baseline)) if sum(v['sumAbsoluteCodeErrorByRgbChannel']) > sum(b['sumAbsoluteCodeErrorByRgbChannel'])]
            assert aggregate == variant['foregroundErrorWorseThanBaselineFrames']
            for channel, key in enumerate('rgb'):
                worse = [i for i, (v, b) in enumerate(zip(rows, baseline)) if v['sumAbsoluteCodeErrorByRgbChannel'][channel] > b['sumAbsoluteCodeErrorByRgbChannel'][channel]]
                assert worse == variant['foregroundChannelErrorWorseThanBaselineFrames'][key]
    for case in result['cases']:
        total_cases += 1
        item = next(c for c in summary['cases'] if c['batch'] == batch and c['specification'] == case['specification'])
        assert case['oraclesComplete'] and case['actualDecodedMetadata']['nb_read_frames'] == '540'
        units = case['accessUnits']
        sizes = units['observedFrameBytes']
        samples = case['encodeLatencyMs']['samples']
        assert len(sizes) == len(samples) == 480 and units['totalFrames'] == 540
        assert sum(sizes) == units['observedAnnexBBytes']
        equal(sum(sizes) / 1e6, units['meanAnnexBMbps'])
        equal(rank(samples, .95), case['encodeLatencyMs']['p95'])
        equal(item['encodeP95Ms'], case['encodeLatencyMs']['p95'])
        equal(item['observedAnnexBMbps'], units['meanAnnexBMbps'])
        assert item['foreground'] == case['foregroundAnalysis']
        variant = foreground['variants'][case['name']]
        assert len(variant['foregroundErrorWorseThanBaselineFrames']) == item['foreground']['worseAggregateFrames']
        for key in 'rgb':
            assert len(variant['foregroundChannelErrorWorseThanBaselineFrames'][key]) == item['foreground']['worseAggregateChannelFrames'][key]
        for region in ('full', 'text', 'thin'):
            text = gzip.decompress((ROOT / batch / (case['name'] + '-' + region + '.ssim.gz')).read_bytes()).decode().splitlines()
            assert len(text) == 480
            for channel, stats in case['quality'][region]['channels'].items():
                values = [float(re.search(r'\b' + channel + r':([^\s]+)', line)[1]) for line in text]
                equal(statistics.mean(values), stats['mean'])
                for i, phase in enumerate(('still', 'typing', 'scrolling', 'window-movement')):
                    assert stats['phaseFrames'][phase] == 120
                    equal(statistics.mean(values[i * 120:(i + 1) * 120]), stats['phaseMeans'][phase])
assert total_cases == 5
print(json.dumps({'verifiedFiles': len(index), 'nativeCases': total_cases, 'observedEncodeFrames': 2400, 'decodedFramesIncludingWarmup': 2700, 'nativeCommandsRerun': 0, 'productionAdmission': False}))
