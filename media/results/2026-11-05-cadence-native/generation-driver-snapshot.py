from pathlib import Path
from types import SimpleNamespace
import hashlib,importlib.util,json,os,signal,sys
ROOT=Path('$CHECKOUT');SHA='1f94f7618dc3e11abb15ad858112c8f4686a2320';OUT=Path('$GENERATION_RESULTS');NATIVE=Path('/tmp/fincode-codecs-low-latency-sustained-f098e46/results.json')
spec=importlib.util.spec_from_file_location('driver',ROOT/'media/tools/run-matched-codecs.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
prior=json.loads(NATIVE.read_text());assert prior['terminal'],'Drain the existing long native measurement before starting this generator.'
assert all(s['groupGone']and not module.group_exists(s['pgid'])for s in prior['steps'])
assert not OUT.exists();OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
args=SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=1,case=[],warmup_seconds=0,observation_seconds=1);runner=module.Runner(args)
def stop(a,b):raise KeyboardInterrupt()
def expired(a,b):raise TimeoutError('900 second source calibration budget')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);signal.signal(signal.SIGALRM,expired);signal.alarm(900)
try:
 runner.run('load',['uptime'],10)
 runner.run('swift-version',['xcrun','--sdk','macosx','swiftc','--version'],15)
 exe=OUT/'private/desktop-workload'
 runner.run('swift-build',['xcrun','--sdk','macosx','swiftc','-O','-swift-version','6','-strict-concurrency=complete','-warnings-as-errors','media/tools/desktop-workload.swift','-o',str(exe)],120,True)
 for label,fps in [('default30',30),('source60',60)]:
  runner.run('generate-'+label,[str(exe),str(OUT/'private'/label),'1920','1080',str(fps)],240,True)
 runner.run('validate-assets',[sys.executable,'-B','/tmp/fincode-check-cadence-assets-1f94f76.py'],180,True)
 for label,tail in [('fps120',['1920','1080','120']),('missing-height',['1920'])]:
  dest=OUT/'private'/('refuse-'+label)
  try:runner.run('refuse-'+label,[str(exe),str(dest),*tail],10,True)
  except ValueError:
   step=runner.steps[-1];assert step['exitCode']==1 and step['groupGone']and not step['timedOut']and not step['cleanupSignals']
  else:raise ValueError('invalid generation plan accepted')
  assert not dest.exists()
  text=(OUT/runner.steps[-1]['privateLog']).read_text();assert text.startswith('desktop-workload: ')and ('unsupported workload cadence'if label=='fps120'else'new-output-directory')in text
 module.write(OUT/'generator.json',{'source':SHA,'executableSha256':module.digest(exe),'generatorSourceSha256':module.digest(ROOT/'media/tools/desktop-workload.swift')})
 runner.exit_code=0
except BaseException as error:
 (OUT/'private/failure.txt').write_text(type(error).__name__+': '+str(error)+'\n');print(type(error).__name__+': '+str(error),flush=True);runner.exit_code=130 if isinstance(error,KeyboardInterrupt)else 1
finally:
 signal.alarm(0);runner.terminal=True;runner.save();d=json.loads((OUT/'results.json').read_text());d.update({'scope':'Swift generation/source calibration, default30 byte controls and two refused CLI plans only. No native codec, screen/window/input or performance budget claim.','helperSha256':module.digest(__file__),'validationHelperSha256':module.digest('/tmp/fincode-check-cadence-assets-1f94f76.py'),'totalScopeDeadlineSeconds':900});module.write(OUT/'results.json',d)
raise SystemExit(runner.exit_code)
