"""Recompute retained numeric QP evidence; no encoder/decoder or tests rerun."""
from pathlib import Path
import gzip
import hashlib
import json
import math
import re
import statistics

ROOT=Path(__file__).resolve().parent
def raw(name):
    value=(ROOT/name).read_bytes()
    return gzip.decompress(value)if name.endswith('.gz')else value
def load(name):return json.loads(raw(name))
def equal(actual,expected):
    assert math.isclose(actual,expected,rel_tol=1e-12,abs_tol=1e-12),(actual,expected)
index=load('index.json')['files']
assert {str(p.relative_to(ROOT))for p in ROOT.rglob('*')if p.is_file()}==set(index)|{'index.json'}
for name,item in index.items():
    value=(ROOT/name).read_bytes()
    assert len(value)==item['bytes']and hashlib.sha256(value).hexdigest()==item['sha256'],name
for name,item in load('provenance.json')['exports'].items():
    value=raw(name)
    assert len(value)==item['normalizedUncompressedBytes']and hashlib.sha256(value).hexdigest()==item['normalizedUncompressedSha256']
summary=load('summary.json')
assert summary['source']=='ef7ed1071728a84ec439f7d0e78c4d42d5e7673f'
assert summary['productionAdmission']is False and len(summary['independentlyAbsentOwnedGroups'])==79
states={name:load(name+'/results.json')for name in ('initial','contracts','native','matched-failure','partial-analysis','middle')}
for name,state in states.items():
    assert state['terminal']and state['exitCode']==(1 if name in ('initial','contracts','matched-failure')else 0)
    for step in state['steps']:
        expected=1 if name=='matched-failure'and step['name'].endswith('-qp28')else 0
        assert step['exitCode']==expected and step['groupGone']and not step['timedOut']and not step['cleanupSignals']
assert states['contracts']['selectedTests']['qp-contracts']['passed']==2
assert states['contracts']['selectedTests']['priority-compatibility']['passed']==1
assert states['contracts']['selectedTests']['profile-contracts']['passed']==2
assert len(states['native']['availability'])==8
assert all(c['supported']and c['nativePropertyVerified']and c['actualDecodedMetadata']['nb_read_frames']=='32'for c in states['native']['availability'])
assert len(states['native']['defaultControls'])==3
for control in states['native']['defaultControls']:
    a,b=control['versions']
    assert control['decodedControlsMatch']and a['decodedRgbSha256']==b['decodedRgbSha256']and a['decodedFrames']==b['decodedFrames']==32
completed=[(c,'matched-failure','partial-analysis')for c in states['partial-analysis']['analyzedRetainedCases']]
completed += [(c,'middle','middle')for c in states['middle']['cases']if c['specification']!='h264:compatible:3000']
assert len(completed)==5 and len({c['name']for c,_,_ in completed})==5
for case,batch,analysis in completed:
    assert case['actualDecodedMetadata']['nb_read_frames']=='540'
    units=case['accessUnits'];latency=case['encodeLatencyMs']['samples'];sizes=units['observedFrameBytes']
    assert len(latency)==len(sizes)==480 and units['totalFrames']==540
    assert sum(sizes)==units['observedAnnexBBytes']
    equal(sum(sizes)/1e6,units['meanAnnexBMbps'])
    equal(sorted(latency)[math.ceil(.95*len(latency))-1],case['encodeLatencyMs']['p95'])
    fg=load(analysis+'/foreground.json.gz');variant=fg['variants'][case['name']];baseline=fg['variants'][fg['baseline']]['rows']
    assert fg['frameMapping']=={'captureWarmupFrames':60,'sourcePrefixFrames':60,'sourceObservedCycleFrames':480,'observedFrames':480}
    assert len(variant['rows'])==480 and variant['sha256']==case['normalizedRgbSha256']
    sums=[]
    for i,row in enumerate(variant['rows']):
        assert(row['observedFrame'],row['captureFrame'],row['sourceFrame'])==(i,i+60,i+60)
        sums.append(row['sumAbsoluteCodeErrorByRgbChannel'])
    observed=case['postprocessedForeground']
    equal(sum(sum(v)for v in sums)/(480*3*fg['calibration']['evaluatedPixelsIncludingHalo']),observed['meanAbsoluteForegroundRgbCodeError'])
    worse=[i for i,(v,b)in enumerate(zip(sums,baseline))if sum(v)>sum(b['sumAbsoluteCodeErrorByRgbChannel'])]
    assert worse==variant['foregroundErrorWorseThanBaselineFrames']and len(worse)==observed['worseAggregateFrames']
    for i,channel in enumerate('rgb'):
        worse=[n for n,(v,b)in enumerate(zip(sums,baseline))if v[i]>b['sumAbsoluteCodeErrorByRgbChannel'][i]]
        assert worse==variant['foregroundChannelErrorWorseThanBaselineFrames'][channel]and len(worse)==observed['worseChannelFrames'][channel]
    for region in ('full','text','thin'):
        lines=raw(batch+'/'+case['name']+'-'+region+'.ssim.gz').decode().splitlines();assert len(lines)==480
        for channel,stats in case['quality'][region]['channels'].items():
            values=[float(re.search(r'\b'+channel+r':([^\s]+)',line)[1])for line in lines]
            equal(statistics.mean(values),stats['mean'])
            for i,phase in enumerate(('still','typing','scrolling','window-movement')):
                assert stats['phaseFrames'][phase]==120
                equal(statistics.mean(values[i*120:(i+1)*120]),stats['phaseMeans'][phase])
    item=next(c for c in summary['hdCases']if c['specification']==case['specification'])
    assert item['foreground']==observed and item['normalizedRgbSha256']==case['normalizedRgbSha256']
    equal(item['observedAnnexBMbps'],units['meanAnnexBMbps'])
failed=load('middle/failed-prefix.json')
assert failed['decodedPrefixFrames']==2 and failed['observedPhaseNotReached']and failed['expectedWarmupFrames']==60
assert 'encoder dropped output'in raw('matched-failure/run-1-hevc-low-latency-compatible-12000-qp28.log.gz').decode()
hevc=[c for c in summary['hdCases']if c['specification'].startswith('hevc:')]
assert len(hevc)==4 and len({c['normalizedRgbSha256']for c in hevc})==1
assert all(c['foreground']['worseAggregateFrames']==12 and c['foreground']['worseChannelFrames']=={'r':12,'g':13,'b':12}for c in hevc)
print(json.dumps({'verifiedFiles':len(index),'tinyNativeCombinations':8,'defaultDecodedPairs':3,'hdCompletedStreams':5,'hdDecodedFrames':2700,'failedQp28PrefixFrames':2,'productionAdmission':False,'nativeCommandsRerun':0}))
