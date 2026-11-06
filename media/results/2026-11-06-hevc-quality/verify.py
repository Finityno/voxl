#!/usr/bin/env python3
"""Recompute the published short-pilot numeric summaries; never decode or view pixels."""
import gzip,hashlib,json,math
from pathlib import Path

ROOT=Path(__file__).resolve().parent
def summary(result,observations):
    baseline=result['cases'][0]
    assert baseline['specification']=='h264:compatible:3000'
    rows=[]
    for case in result['cases']:
        assert case['oraclesComplete'] and case['accessUnits']['totalFrames']==270
        assert int(case['actualDecodedMetadata']['nb_read_frames'])==270
        sizes=case['accessUnits']['observedFrameBytes'];assert len(sizes)==240
        total=sum(sizes);assert total==case['accessUnits']['observedAnnexBBytes']
        rate=total*8/8/1_000_000
        assert math.isclose(rate,case['accessUnits']['meanAnnexBMbps'],abs_tol=1e-12)
        samples=case['encodeLatencyMs']['samples'];assert len(samples)==240
        p95=sorted(samples)[math.ceil(len(samples)*.95)-1]
        assert math.isclose(p95,case['encodeLatencyMs']['p95'],abs_tol=1e-12)
        resource=case['processResources'];cpu=100*(resource['userSeconds']+resource['systemSeconds'])/resource['wallSeconds']
        assert math.isclose(cpu,resource['averageCpuPercentOneCore'],abs_tol=1e-12)
        measured=observations['regions'][case['name']]['text']
        control=observations['regions'][baseline['name']]['text']
        assert len(measured)==len(control)==240
        worse=sum(row['All']<original['All'] for row,original in zip(measured,control))
        foreground=observations['foreground']['variants'][case['name']]
        assert len(foreground['rows'])==240
        errors=[row['meanAbsoluteForegroundRgbCodeError'] for row in foreground['rows']]
        assert math.isclose(sum(errors)/240,case['foregroundAnalysis']['meanAbsoluteForegroundRgbCodeError'],abs_tol=1e-10)
        rows.append({'name':case['name'],'specification':case['specification'],'payloadMbps':rate,
            'payloadBytesSavedPercent':100*(1-total/baseline['accessUnits']['observedAnnexBBytes']),
            'encodeCallbackP95Ms':p95,'wholeProbeCpuPercentOneCore':cpu,
            'wholeProbePeakRssMiB':resource['maximumResidentBytes']/2**20,
            'textSsimFramesLowerThanControl':worse,
            'textSsimWorstDelta':min(row['All']-original['All'] for row,original in zip(measured,control)),
            'textSsimPhaseMeans':case['quality']['text']['channels']['All']['phaseMeans'],
            'foregroundAggregateFramesWorse':case['foregroundAnalysis']['worseAggregateFrames'],
            'foregroundChannelFramesWorse':case['foregroundAnalysis']['worseAggregateChannelFrames']})
    return rows

def main():
    index=json.loads((ROOT/'index.json').read_text())
    for relative,expected in index['files'].items():
        assert hashlib.sha256((ROOT/relative).read_bytes()).hexdigest()==expected,relative
    published=json.loads((ROOT/'summary.json').read_text())
    for batch in ['initial','higher']:
        result=json.loads((ROOT/(batch+'-results.json')).read_text())
        assert result['terminal'] and result['exitCode']==0 and result['source']==index['sourceRevision']
        observations=json.loads(gzip.decompress((ROOT/(batch+'-observations.json.gz')).read_bytes()))
        assert summary(result,observations)==published['batches'][batch]
    print('Verified eight short streams, their numeric summaries and published file hashes; no performance admission.')

if __name__=='__main__':main()
