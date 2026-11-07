from pathlib import Path
from types import SimpleNamespace
import importlib.util,json,os,signal,sys
ROOT=Path('$CHECKOUT');SHA='ef7ed1071728a84ec439f7d0e78c4d42d5e7673f';PRIOR=Path('$MATCHED_FAILURE_RESULTS');OUT=Path('$PARTIAL_ANALYSIS_RESULTS');FIXTURE=Path('$FIXTURE60')
spec=importlib.util.spec_from_file_location('bounded',ROOT/'media/tools/run-matched-codecs.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
r=json.loads((PRIOR/'results.json').read_text());assert r['source']==SHA and r['terminal']and r['exitCode']==1 and len(r['cases'])==3
for step in r['steps']:
 assert step['groupGone']and not m.group_exists(step['pgid'])
assert not OUT.exists();OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
args=SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=1,case=[],warmup_seconds=1,observation_seconds=8,fps=60);runner=m.Runner(args);records=[]
def stop(a,b):raise KeyboardInterrupt()
def expired(a,b):raise TimeoutError('300 second partial numeric analysis budget')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);signal.signal(signal.SIGALRM,expired);signal.alarm(300)
try:
 baseline=next(c for c in r['cases']if c['specification']=='h264:compatible:3000');floor=PRIOR/'private/floor.rgb';hashes={str(floor):m.digest(floor)}
 command=[sys.executable,'-B','media/tools/analyze-text-workload.py','--fixture',str(FIXTURE),'--baseline',baseline['name'],'--floor','uncompressed420','--decoded','uncompressed420='+str(floor),'--observed-frames','480','--capture-warmup-frames','60','--engine','numpy','--output',str(OUT/'foreground.json')]
 for c in r['cases']:
  assert c['encodingAndDecodeOraclesComplete']and c['accessUnits']['observedFrames']==480
  path=Path(c['rgbPath']);assert m.digest(path)==c['normalizedRgbSha256'];hashes[str(path)]=m.digest(path);command+=['--decoded',c['name']+'='+str(path)]
 runner.run('foreground-completed-prefix',command,180,True);fg=json.loads((OUT/'foreground.json').read_text());assert fg['frameMapping']=={'captureWarmupFrames':60,'sourcePrefixFrames':60,'sourceObservedCycleFrames':480,'observedFrames':480}
 for c in r['cases']:
  v=fg['variants'][c['name']];assert v['sha256']==c['normalizedRgbSha256']and v['frames']==480
  value=json.loads(json.dumps(c));value['postprocessedForeground']={'meanAbsoluteForegroundRgbCodeError':v['meanAbsoluteForegroundRgbCodeError'],'worseAggregateFrames':len(v['foregroundErrorWorseThanBaselineFrames']),'worseChannelFrames':{key:len(rows)for key,rows in v['foregroundChannelErrorWorseThanBaselineFrames'].items()}};value['partialOraclesComplete']=True;records.append(value)
 assert all(m.digest(path)==value for path,value in hashes.items());runner.exit_code=0
except BaseException as error:
 (OUT/'private/failure.txt').write_text(type(error).__name__+': '+str(error)+'\n');print(type(error).__name__+': '+str(error),flush=True);runner.exit_code=130 if isinstance(error,KeyboardInterrupt)else 1
finally:
 signal.alarm(0);runner.terminal=True;runner.save();d=json.loads((OUT/'results.json').read_text());d.update({'scope':'Analysis-only completion for the three independently decoded streams before nativeQP28 startup failure. No encoder/build/test rerun, no retry/admission of the failed mode. Original incomplete-oracle flags and failure receipt remain unchanged. Not full matrix success, sustained/physical/app/hardware-decoder qualification.','priorReceiptSha256':m.digest(PRIOR/'results.json'),'helperSha256':m.digest(__file__),'analyzedRetainedCases':records,'failedSpecification':'hevc:low-latency-compatible:12000:qp28','nativeFailure':'encoder dropped output, flag2 after accepted exact integer readback'});m.write(OUT/'results.json',d)
raise SystemExit(runner.exit_code)
