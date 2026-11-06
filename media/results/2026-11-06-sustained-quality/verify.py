#!/usr/bin/env python3
"""Verify the retained sustained-pair numeric evidence, without viewing pixels."""
import gzip,hashlib,json,math
from pathlib import Path

ROOT=Path(__file__).resolve().parent
def derive(result,observations):
    assert result['terminal'] and result['exitCode']==0 and not result['unexecutedCases']
    plan=result['plan'];assert plan=={'fps':30,'warmupSeconds':30,'observationSeconds':300,'independentRuns':1,'cases':['h264:compatible:3000','hevc:compatible:8000:quality']}
    baseline=result['cases'][0];assert baseline['specification']=='h264:compatible:3000'
    control=observations['textSsim'][baseline['name']]
    foreground=observations['foreground']['variants'];rows=[]
    for case in result['cases']:
        assert case['oraclesComplete'] and int(case['actualDecodedMetadata']['nb_read_frames'])==9900
        units=case['accessUnits'];assert units['totalFrames']==9900 and units['observedFrames']==9000
        sizes=units['observedFrameBytes'];assert len(sizes)==9000 and sum(sizes)==units['observedAnnexBBytes']
        rate=sum(sizes)*8/300/1_000_000;assert math.isclose(rate,units['meanAnnexBMbps'],abs_tol=1e-12)
        latency=case['encodeLatencyMs'];assert len(latency['samples'])==9000
        p95=sorted(latency['samples'])[math.ceil(9000*.95)-1];assert math.isclose(p95,latency['p95'],abs_tol=1e-12)
        resource=case['processResources'];cpu=100*(resource['userSeconds']+resource['systemSeconds'])/resource['wallSeconds']
        assert math.isclose(cpu,resource['averageCpuPercentOneCore'],abs_tol=1e-12)
        text=observations['textSsim'][case['name']];assert len(text)==len(control)==9000
        rgb=foreground[case['name']];assert len(rgb['rows'])==9000
        control_rgb=foreground[baseline['name']]['rows'];assert len(control_rgb)==9000
        channels={name:sum(r['sumAbsoluteCodeErrorByRgbChannel'][i]>b['sumAbsoluteCodeErrorByRgbChannel'][i] for r,b in zip(rgb['rows'],control_rgb)) for i,name in enumerate(['r','g','b'])}
        worse=sum(sum(r['sumAbsoluteCodeErrorByRgbChannel'])>sum(b['sumAbsoluteCodeErrorByRgbChannel']) for r,b in zip(rgb['rows'],control_rgb))
        assert channels==case['foregroundAnalysis']['worseAggregateChannelFrames'] and worse==case['foregroundAnalysis']['worseAggregateFrames']
        error=sum(r['meanAbsoluteForegroundRgbCodeError'] for r in rgb['rows'])/9000
        assert math.isclose(error,case['foregroundAnalysis']['meanAbsoluteForegroundRgbCodeError'],abs_tol=1e-10)
        rows.append({'specification':case['specification'],'decodedFrames':9900,'observedFrames':9000,'videoPayloadMbps':rate,
            'payloadBytesSavedPercent':100*(1-units['observedAnnexBBytes']/baseline['accessUnits']['observedAnnexBBytes']),
            'encodeCallbackP95Ms':p95,'wholeProbeCpuPercentOneCore':cpu,'wholeProbePeakRssMiB':resource['maximumResidentBytes']/2**20,
            'textSsimFramesLowerThanControl':sum(r['All']<b['All'] for r,b in zip(text,control)),
            'textSsimWorstDelta':min(r['All']-b['All'] for r,b in zip(text,control)),
            'foregroundAggregateFramesWorse':worse,'foregroundChannelFramesWorse':channels,'foregroundMeanAbsoluteRgbError':error,
            'textSsimPhaseMeans':case['quality']['text']['channels']['All']['phaseMeans']})
    return rows

def main():
    index=json.loads((ROOT/'index.json').read_text())
    for relative,expected in index['files'].items():
        assert hashlib.sha256((ROOT/relative).read_bytes()).hexdigest()==expected,relative
    result=json.loads(gzip.decompress((ROOT/'results.json.gz').read_bytes()))
    observations=json.loads(gzip.decompress((ROOT/'observations.json.gz').read_bytes()))
    assert result['source']==index['encoderSourceRevision']
    assert derive(result,observations)==json.loads((ROOT/'summary.json').read_text())['cases']
    inventory=json.loads((ROOT/'inventory.json').read_text())
    assert [r['codec'] for r in inventory['codecs']]==['h264','hevc','av1']
    print('Verified sustained pair: 19,800 decoded frames, payload/latency/CPU, 18,000 text and foreground comparisons, inventory and indexed hashes. No profile admission.')

if __name__=='__main__':main()
