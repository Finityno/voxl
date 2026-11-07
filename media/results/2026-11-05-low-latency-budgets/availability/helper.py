from pathlib import Path
from types import SimpleNamespace
import importlib.util,json,signal,struct
ROOT=Path('$CHECKOUT');SHA='f098e462a62833b73d6a6103e4f50ff810fb7ffb';OUT=Path('$AVAILABILITY');assert not OUT.exists();OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
spec=importlib.util.spec_from_file_location('driver',ROOT/'media/tools/run-matched-codecs.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
args=SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=1,case=[],warmup_seconds=1,observation_seconds=8);runner=module.Runner(args);cases=[]
def stop(a,b):raise KeyboardInterrupt()
def expire(a,b):raise TimeoutError('360 second total availability scope')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);signal.signal(signal.SIGALRM,expire);signal.alarm(360)
try:
 exe=ROOT/'media/target/release/examples/codec_probe';artifact=json.loads(Path('/tmp/fincode-callback-errors-f098e46/compiler-artifact.json').read_text());assert artifact['source']==SHA and module.digest(exe)==artifact['executableSha256']
 for tuning,kbps in [('compatible',1500),('low-latency-compatible',3000),('low-latency-compatible',6000),('low-latency-compatible',12000)]:
  name=tuning+'-'+str(kbps);video=OUT/'private'/(name+'.hevc');framed=OUT/'private'/(name+'.vxfr')
  command=[str(exe),'hardware',str(video),'hevc',str(framed),'--warmup-frames=30','--frames=240','--width=1920','--height=1080','--fps=30','--pixel-range=full','--color-tags=srgb','--input-nv12=$FIXTURE/reference.nv12','--input-cycle-prefix=30','--tuning='+tuning,'--priority=default','--bitrate-kbps='+str(kbps),'--force-keyframe-period=60']
  try:log=runner.run(name,command,79,True)
  except ValueError:
   s=runner.steps[-1];assert s['name']==name and s['exitCode']==1 and not s['timedOut']and not s['cleanupSignals']and s['groupGone'];log=OUT/s['privateLog']
  step=runner.steps[-1];text=log.read_text();blob=framed.read_bytes();assert blob[:6]==b'VXFR\x01\x01';offset=6;sizes=[];timestamps=[]
  while offset<len(blob):
   assert offset+13<=len(blob);pts,key,size=struct.unpack_from('>qBI',blob,offset);offset+=13;assert key in [0,1]and 0<size<=8*1024*1024 and offset+size<=len(blob);sizes.append(size);timestamps.append(pts);offset+=size
  assert sum(sizes)==video.stat().st_size and timestamps==[i*1000000//30 for i in range(len(timestamps))]
  case={'tuning':tuning,'requestedKbps':kbps,'nativeExitCode':step['exitCode'],'completedFrames':len(sizes),'plannedFrames':270,'encodedSha256':module.digest(video),'framedSha256':module.digest(framed),'sizes':sizes,'reportedFailure':next((line for line in text.splitlines()if line.startswith('Error:')),None)}
  if step['exitCode']==0:assert len(sizes)==270 and case['reportedFailure']is None
  else:assert tuning=='low-latency-compatible'and case['reportedFailure']is not None
  if sizes:
   log=runner.run(name+'-decode',['/opt/homebrew/bin/ffprobe','-v','error','-threads','2','-count_frames','-select_streams','v:0','-show_entries','stream=codec_name,width,height,nb_read_frames','-of','json',str(video)],30,True)
   metadata=json.loads(log.read_text())['streams'];assert len(metadata)==1 and int(metadata[0]['nb_read_frames'])==len(sizes);case['actualDecode']=metadata[0]
  cases.append(case);module.write(OUT/'availability.json',{'source':SHA,'executableSha256':module.digest(exe),'cases':cases,'productionAdmission':False,'scope':'Availability-only requested-bitrate probe; no matched quality, sustained resources, speed, billed cost, or admission claim. Failed/partial cases remain failed.'})
 runner.exit_code=0
finally:
 signal.alarm(0);runner.terminal=True;runner.save();d=json.loads((OUT/'results.json').read_text());d.update({'scope':'Four bounded native availability checks, not performance/profile admission; supervisor success does not mean every native case succeeds.','helperSha256':module.digest(__file__)});module.write(OUT/'results.json',d)
raise SystemExit(runner.exit_code)
