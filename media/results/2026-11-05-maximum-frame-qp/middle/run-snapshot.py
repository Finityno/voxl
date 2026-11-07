from pathlib import Path
from types import SimpleNamespace
import importlib.util,json,signal,sys,os
ROOT=Path('$CHECKOUT');SHA='ef7ed1071728a84ec439f7d0e78c4d42d5e7673f';PRIOR=Path('$MATCHED_FAILURE_RESULTS');OUT=Path('$MIDDLE_RESULTS');FIXTURE=Path('$FIXTURE60')
spec=importlib.util.spec_from_file_location('bounded',ROOT/'media/tools/run-matched-codecs.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
prior=json.loads((PRIOR/'results.json').read_text());assert prior['source']==SHA and prior['terminal']and prior['exitCode']==1
for step in prior['steps']:assert step['groupGone']and not m.group_exists(step['pgid'])
assert not OUT.exists();OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
cases=['h264:compatible:3000','hevc:low-latency-compatible:12000:qp32','hevc:low-latency-compatible:12000:qp34'];args=SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=1,case=cases,warmup_seconds=1,observation_seconds=8,fps=60,analysis_engine='numpy',ffmpeg=Path('/opt/homebrew/bin/ffmpeg').resolve(),ffprobe=Path('/opt/homebrew/bin/ffprobe').resolve());runner=m.Runner(args);failures=[]
def stop(a,b):raise KeyboardInterrupt()
def expired(a,b):raise TimeoutError('600 second retained-baseline QP follow-up budget')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);signal.signal(signal.SIGALRM,expired);signal.alarm(600)
try:
 artifact=json.loads((PRIOR/'compiler-artifact.json').read_text());executable=Path(artifact['artifact']['executable']);assert m.digest(executable)==artifact['executableSha256']
 calibration=json.loads((PRIOR/'fixture.json').read_text());assert m.digest(FIXTURE/'manifest.json')==calibration['manifestSha256'];manifest=calibration['manifest'];observation=PRIOR/'private/observation-reference.nv12'
 baseline=json.loads(json.dumps(next(c for c in prior['cases']if c['specification']==cases[0])));baseline['retainedExecution']={'priorReceiptSha256':m.digest(PRIOR/'results.json'),'scope':'Earlier actual baseline process reused, no native baseline replay; timings are sequential individual samples, not a statistical paired-speed result.'};assert m.digest(baseline['rgbPath'])==baseline['normalizedRgbSha256'];runner.cases.append(baseline)
 # Inspect only the failed mode's already-written prefix. This does not retry it.
 failed=PRIOR/'private/run-1-hevc-low-latency-compatible-12000-qp28';decode=runner.run('failed-qp28-prefix-decode',[str(args.ffprobe),'-v','error','-threads','2','-count_frames','-show_entries','stream=codec_name,width,height,nb_read_frames','-of','json',str(failed/'encoded.hevc')],30);meta=json.loads(decode.read_text())['streams'];assert len(meta)==1 and 0<int(meta[0]['nb_read_frames'])<60;m.write(OUT/'failed-prefix.json',{'specification':'hevc:low-latency-compatible:12000:qp28','decodedPrefixFrames':int(meta[0]['nb_read_frames']),'expectedWarmupFrames':60,'expectedObservedFrames':480,'observedPhaseNotReached':True,'encodedSha256':m.digest(failed/'encoded.hevc'),'framedSha256':m.digest(failed/'frames.vxfr'),'scope':'Existing failed-stream prefix only, not a complete/available mode or repeated native attempt.'})
 for value in cases[1:]:
  try:runner.case(executable,value,1,FIXTURE,manifest,observation)
  except ValueError:
   step=runner.steps[-1];log=(OUT/step['privateLog']).read_text();assert step['exitCode']==1 and step['groupGone']and not step['timedOut']and not step['cleanupSignals']and 'encoder dropped output'in log
   failures.append({'specification':value,'actualExitCode':1,'cause':'Native dropped output; profile unavailable, not substituted.'})
 floor=PRIOR/'private/floor.rgb';command=[sys.executable,'-B','media/tools/analyze-text-workload.py','--fixture',str(FIXTURE),'--baseline',baseline['name'],'--floor','uncompressed420','--decoded','uncompressed420='+str(floor),'--observed-frames','480','--capture-warmup-frames','60','--engine','numpy','--output',str(OUT/'foreground.json')]
 for c in runner.cases:
  assert m.digest(c['rgbPath'])==c['normalizedRgbSha256'];command+=['--decoded',c['name']+'='+c['rgbPath']]
 runner.run('foreground-successful-candidates',command,180,True);fg=json.loads((OUT/'foreground.json').read_text());assert fg['frameMapping']=={'captureWarmupFrames':60,'sourcePrefixFrames':60,'sourceObservedCycleFrames':480,'observedFrames':480}
 for c in runner.cases:
  v=fg['variants'][c['name']];assert v['sha256']==c['normalizedRgbSha256']and v['frames']==480;c['postprocessedForeground']={'meanAbsoluteForegroundRgbCodeError':v['meanAbsoluteForegroundRgbCodeError'],'worseAggregateFrames':len(v['foregroundErrorWorseThanBaselineFrames']),'worseChannelFrames':{key:len(rows)for key,rows in v['foregroundChannelErrorWorseThanBaselineFrames'].items()}};c['oraclesComplete']=True
 runner.exit_code=1 if failures else 0
except BaseException as error:
 (OUT/'private/failure.txt').write_text(type(error).__name__+': '+str(error)+'\n');print(type(error).__name__+': '+str(error),flush=True);runner.exit_code=130 if isinstance(error,KeyboardInterrupt)else 1
finally:
 signal.alarm(0);runner.terminal=True;runner.save();d=json.loads((OUT/'results.json').read_text());d.update({'scope':'Two fresh middle-QP candidates against retained exact-source baseline, with independent full-stream decode/SSIM/foreground analysis and failedQP28-prefix inspection only. No build/unit/Clippy/default-baseline replay. Individual CPU/timing samples include startup and do not establish sustained/paired statistical performance or application/physical/hardware-decoder admission.','priorReceiptSha256':m.digest(PRIOR/'results.json'),'candidateFailures':failures,'helperSha256':m.digest(__file__),'totalDeadlineSeconds':600});m.write(OUT/'results.json',d)
raise SystemExit(runner.exit_code)
