#!/usr/bin/env python3
"""Recompute encoder experiment receipts from numeric records, without pixels."""
import gzip,hashlib,json,math,re
from pathlib import Path

ROOT=Path(__file__).resolve().parent
def read(name):return json.loads(gzip.decompress((ROOT/name).read_bytes()))
def metrics(case,frames):
    units=case['accessUnits'];sizes=units['observedFrameBytes'];assert len(sizes)==frames
    assert sum(sizes)==units['observedAnnexBBytes']
    payload=sum(sizes)*8*60/(frames*1_000_000)
    assert math.isclose(payload,units['meanAnnexBMbps'],abs_tol=1e-12)
    latency=case['encodeLatencyMs']['samples'];assert len(latency)==frames
    p95=sorted(latency)[math.ceil(frames*.95)-1];assert p95==case['encodeLatencyMs']['p95']
    pipeline=case['pipeline'];arrivals=pipeline['observedOutputArrivalsMs']
    assert len(arrivals)==len(pipeline['submissionLatenessMs'])==frames
    assert all(b>a for a,b in zip(arrivals,arrivals[1:]))
    fps=(frames-1)*1000/(arrivals[-1]-arrivals[0])
    assert math.isclose(fps,pipeline['observedCompletionFps'],abs_tol=1e-10)
    total=units['totalFrames'];assert pipeline['submittedFrames']==pipeline['completedFrames']==total
    assert 1<=pipeline['maximumPending']<=pipeline['requestedDepth']<=2
    assert case['oraclesComplete'] and int(case['actualDecodedMetadata']['nb_read_frames'])==total
    resource=case['processResources'];cpu=100*(resource['userSeconds']+resource['systemSeconds'])/resource['wallSeconds']
    assert math.isclose(cpu,resource['averageCpuPercentOneCore'],abs_tol=1e-10)
    return {'name':case['name'],'completionFps':fps,'receiptP95Ms':p95,'payloadMbps':payload,
        'wholeProbeCpuPercentOneCore':cpu,'peakProbeRssMiB':resource['maximumResidentBytes']/2**20}

def verify_batch(batch,frames):
    result=batch['result'];assert result['terminal'] and result['exitCode']==0 and not result['unexecutedCases']
    cases={v['name']:v for v in result['cases']};rows=[metrics(v,frames) for v in result['cases']]
    for pair in result['comparisons']:
        left,right=cases[pair['baseline']],cases[pair['candidate']]
        assert left['inputSha256']==right['inputSha256']==pair['fixedInputSha256']
        assert (left['normalizedRgbSha256']==right['normalizedRgbSha256'])==pair['sameNormalizedDecodedRgb']
        worse={}
        for region in ['full','text','thin']:
            a,b=batch['ssim'][left['name']][region],batch['ssim'][right['name']][region]
            assert len(a)==len(b)==frames
            worse[region]={key:sum(x[key]<y[key] for x,y in zip(b,a)) for key in ['Y','U','V','All']}
        assert worse==pair['worseSsimFrames']
        fg=batch['foreground'][f"pair-{pair['pair']}-{pair['codec']}"]['variants']
        a,b=fg[left['name']]['rows'],fg[right['name']]['rows'];assert len(a)==len(b)==frames
        aggregate=sum(x['meanAbsoluteForegroundRgbCodeError']>y['meanAbsoluteForegroundRgbCodeError'] for x,y in zip(b,a))
        channels=[sum(x['sumAbsoluteCodeErrorByRgbChannel'][channel]>y['sumAbsoluteCodeErrorByRgbChannel'][channel] for x,y in zip(b,a)) for channel in range(3)]
        assert aggregate==right['foregroundAnalysis']['worseAggregateFrames']
        assert dict(zip(['r','g','b'],channels))==right['foregroundAnalysis']['worseAggregateChannelFrames']
        assert pair['measuredQualityNonregression']==(not aggregate and not any(channels) and not any(v for region in worse.values() for v in region.values()))
    return rows

def decoded_hashes(name):
    text=gzip.decompress((ROOT/'frame-hashes'/name).read_bytes()).decode()
    rows=[row for row in text.splitlines() if row and not row.startswith('#')]
    parsed=[re.fullmatch(r'0,\s*(-?\d+),\s*(-?\d+),\s*1,\s*12441600,\s*([a-f0-9]{32})',row) for row in rows]
    assert len(parsed)==540 and all(parsed)
    assert [int(row[1]) for row in parsed]==[int(row[2]) for row in parsed]==list(range(540))
    return [row[3] for row in parsed]

def main():
    index=json.loads((ROOT/'index.json').read_text())
    for name,expected in index['files'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==expected,name
    summary=json.loads((ROOT/'summary.json').read_text())
    for name,frames in [('short',480),('long-hevc',3600)]:
        batch=read(name+'.json.gz');assert verify_batch(batch,frames)==summary[name]
    compatibility=read('compatibility.json.gz');assert compatibility['terminal'] and compatibility['exitCode']==0
    for pair in compatibility['pixelComparisons']+[compatibility['oldDefaultComparison']]:
        a=decoded_hashes(pair['baseline']+'-full-frame-hashes.log.gz');b=decoded_hashes(pair['candidate']+'-full-frame-hashes.log.gz')
        assert pair['decodedFrames']==540 and pair['equalFrames']==sum(x==y for x,y in zip(a,b))==540
    records=[read(name+'.json.gz')['result'] for name in ['short','long-hevc']]
    records+=[compatibility,read('probe-contracts.json.gz'),read('wake-contracts.json.gz')]
    count=0
    for record in records:
        assert record['exitCode']==0 and record['terminal']
        for step in record['steps']:
            assert step['exitCode']==step['childExitCode']==0 and step['groupGone'] and not step['timedOut'] and not step['cleanupSignals'];count+=1
    assert count==index['completedGroups']
    for name,expected in index['selectedTestLogs'].items():
        log=gzip.decompress((ROOT/name).read_bytes()).decode()
        if expected['kind']=='rust':assert re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)==[(str(expected['passed']),'0','0')]
        else:assert re.search(r'Ran '+str(expected['passed'])+r' tests? in [\d.]+s\s+OK',log)
    print(f'Verified 12 paired streams, full-frame compatibility, numeric quality/resources and {count} completed groups; no app/performance admission.')

if __name__=='__main__':main()
