from pathlib import Path
from types import SimpleNamespace
import importlib.util,json,os,re,signal,sys
ROOT=Path('$CHECKOUT');SHA='2dd18efdfe6f4cd98966045a6fbb2ba14503c936';OUT=Path('$RESULTS');assert not OUT.exists();OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
spec=importlib.util.spec_from_file_location('driver',ROOT/'media/tools/run-matched-codecs.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
args=SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=None,runs=1,case=[],warmup_seconds=0,observation_seconds=1);runner=module.Runner(args);counts={}
def stop(a,b):raise KeyboardInterrupt()
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
try:
 for label,file,expected in [('parsers','test-run-matched-codecs.py',7),('foreground','test-analyze-text-workload.py',6)]:
  log=runner.run(label,[sys.executable,'-B','media/tools/'+file,'-v'],30)
  text=log.read_text();match=re.search(r'Ran (\d+) tests?',text);assert match and int(match.group(1))==expected and re.search(r'^OK$',text,re.M);counts[label]={'passed':expected,'skipped':0}
 for label,options,expected in [('fps',['--fps','120'],2),('observed',['--fps','60','--observation-seconds','301'],1),('warmup',['--fps','60','--warmup-seconds','31'],1)]:
  output=OUT/'private'/('reject-'+label)
  try:runner.run('reject-'+label,[sys.executable,'-B','media/tools/run-matched-codecs.py','--revision',SHA,'--output',str(output),*options],10)
  except ValueError:
   step=runner.steps[-1];assert step['name']=='reject-'+label and step['exitCode']==expected and step['groupGone']and not step['timedOut']and not step['cleanupSignals']
  else:raise ValueError('invalid plan was accepted')
  assert not output.exists(),'refused plan must not create an output or start a build/encoder'
 runner.run('diff',['git','diff','--check','5e2b60b20ce0b4d012bff5499c52e5fbbb5fd08f',SHA],10)
 runner.exit_code=0
except BaseException as error:
 (OUT/'private/failure.txt').write_text(type(error).__name__+': '+str(error)+'\n');print(type(error).__name__+': '+str(error),flush=True);runner.exit_code=1
finally:
 runner.terminal=True;runner.save();d=json.loads((OUT/'results.json').read_text());d.update({'scope':'Pure evidence-clock/phase/mapping contracts and three invalid-plan refusals only. No native encoder, generator, screen, input or performance qualification. Does not take the measurement lock.','selectedTests':counts,'helperSha256':module.digest(__file__)});module.write(OUT/'results.json',d)
raise SystemExit(runner.exit_code)
