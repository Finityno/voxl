from pathlib import Path
from types import SimpleNamespace
import importlib.util,json,os,re,signal,struct,sys
ROOT=Path('$CHECKOUT');SHA='f098e462a62833b73d6a6103e4f50ff810fb7ffb';OUT=Path(__file__).parent
spec=importlib.util.spec_from_file_location('driver',ROOT/'media/tools/run-matched-codecs.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
args=SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=1,case=[],warmup_seconds=0,observation_seconds=1);runner=module.Runner(args);counts={}
def stop(a,b):raise KeyboardInterrupt()
def expired(a,b):raise TimeoutError('300 second total scope budget')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);signal.signal(signal.SIGALRM,expired);signal.alarm(300)
try:
 runner.run('load',['uptime'],10);runner.run('toolchain',['cargo','--version'],10)
 common=['--locked','--manifest-path','media/Cargo.toml','--release','-j2','-q']
 log=runner.run('build',['cargo','build',*common,'--example','codec_probe','--message-format=json'],120,True)
 artifacts=[json.loads(line)for line in log.read_text().splitlines()if line.startswith('{')];artifacts=[a for a in artifacts if a.get('reason')=='compiler-artifact'and a.get('target',{}).get('name')=='codec_probe'and a.get('executable')];assert len(artifacts)==1;exe=Path(artifacts[0]['executable']);assert exe.parent==ROOT/'media/target/release/examples'
 module.write(OUT/'compiler-artifact.json',{'source':SHA,'artifact':artifacts[0],'executableSha256':module.digest(exe)})
 runner.run('test-build',['cargo','test',*common,'--lib','--no-run','--message-format=short'],90,True)
 runner.run('clippy',['cargo','clippy',*common,'--lib','--example','codec_probe','--tests','--message-format=short','--','-D','warnings'],90,True)
 for label in ['callback_','color_configuration_']:
  log=runner.run('test-'+label,['cargo','test',*common,'--lib',label,'--','--nocapture'],30,True)
  matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log.read_text());assert len(matches)==1 and int(matches[0][0])>0
  counts[label]={'passed':int(matches[0][0]),'failed':int(matches[0][1]),'ignored':int(matches[0][2])}
 old=OUT/'private/previous-probe';assert module.digest(old)==json.loads((OUT/'previous-artifact.json').read_text())['executableSha256']
 controls=[]
 for codec,mode in [('h264','hardware'),('hevc','hardware'),('h264','software')]:
  decoded=[]
  for version,binary in [('previous',old),('current',exe)]:
   name=codec+'-'+mode+'-'+version;video=OUT/'private'/(name+'.'+codec);raw=OUT/'private'/(name+'.nv12')
   runner.run(name,[str(binary),mode,str(video),codec,'--frames=32','--width=320','--height=180','--fps=30','--bitrate-kbps=500','--pattern=color-bars','--pixel-range=full','--color-tags=srgb','--force-keyframe-period=10'],8,True)
   runner.run(name+'-decode',['/opt/homebrew/bin/ffmpeg','-nostdin','-hide_banner','-loglevel','error','-threads','2','-filter_threads','1','-i',str(video),'-vf','scale=out_range=pc:flags=accurate_rnd+bitexact,format=nv12','-fps_mode','passthrough','-f','rawvideo',str(raw)],8,True)
   assert raw.stat().st_size==32*320*180*3//2;decoded.append(module.digest(raw));controls.append({'codec':codec,'mode':mode,'version':version,'decodedFrames':32,'decodedSha256':decoded[-1],'encodedSha256':module.digest(video)})
  assert decoded[0]==decoded[1]
 module.write(OUT/'default-controls.json',{'controls':controls,'allThreeDecodedPairsMatch':True,'scope':'Generated tiny-bar decoded-output control, not performance/text/device admission.'})
 name='diagnose-hevc-low-latency';video=OUT/'private/reproduced.hevc';framed=OUT/'private/reproduced.vxfr'
 command=[str(exe),'hardware',str(video),'hevc',str(framed),'--warmup-frames=30','--frames=240','--width=1920','--height=1080','--fps=30','--pixel-range=full','--color-tags=srgb','--input-nv12=$FIXTURE/reference.nv12','--input-cycle-prefix=30','--tuning=low-latency-compatible','--priority=default','--bitrate-kbps=1500','--force-keyframe-period=60']
 try:log=runner.run(name,command,79,True)
 except ValueError:
  step=runner.steps[-1];assert step['name']==name and step['exitCode']==1 and not step['timedOut']and not step['cleanupSignals']and step['groupGone'];log=OUT/step['privateLog']
 step=runner.steps[-1];text=log.read_text();blob=framed.read_bytes();offset=0;total=0;timestamps=[]
 while offset<len(blob):
  assert offset+13<=len(blob);pts,key,size=struct.unpack_from('>qBI',blob,offset);offset+=13;assert key in [0,1]and size>0 and offset+size<=len(blob);total+=size;offset+=size;timestamps.append(pts)
 assert total==video.stat().st_size and timestamps==[i*1000000//30 for i in range(len(timestamps))]
 diagnosis={'source':SHA,'nativeExitCode':step['exitCode'],'completedFrames':len(timestamps),'plannedFrames':270,'productionAdmission':False,'logSha256':module.digest(log),'encodedSha256':module.digest(video),'framedSha256':module.digest(framed),'scope':'Same fixture/geometry/timing/bitrate as the prior failed pilot. A native failure remains a failed codec case; the diagnostic supervisor may complete.'}
 if step['exitCode']==1:assert 'Error: Platform { operation: "encoder dropped output", code: 2 }'in text or 'Error: Platform { operation: "encoder output callback", code:'in text or 'Error: InvalidFrame'in text or 'Error: Backpressure'in text;diagnosis['reportedFailure']=next(line for line in text.splitlines()if line.startswith('Error:'))
 else:assert len(timestamps)==270;diagnosis['reportedFailure']=None
 if timestamps:
  log=runner.run(name+'-prefix-decode',['/opt/homebrew/bin/ffprobe','-v','error','-threads','2','-count_frames','-select_streams','v:0','-show_entries','stream=codec_name,width,height,nb_read_frames','-of','json',str(video)],30,True)
  decoded=json.loads(log.read_text())['streams'];assert len(decoded)==1 and int(decoded[0]['nb_read_frames'])==len(timestamps);diagnosis['actualPrefixDecode']=decoded[0]
 module.write(OUT/'diagnosis.json',diagnosis)
 runner.run('diff',['git','diff','--check','21a80a6c26cbdffea6a019a1665ca83f97565adf',SHA],10)
 runner.exit_code=0
except BaseException as error:
 (OUT/'private/failure.txt').write_text(type(error).__name__+': '+str(error)+'\n');print(type(error).__name__+': '+str(error),flush=True);runner.exit_code=130 if isinstance(error,KeyboardInterrupt)else 1
finally:
 signal.alarm(0);runner.terminal=True;runner.save();d=json.loads((OUT/'results.json').read_text());d.update({'scope':'Scoped Mac callback failure/wake contracts, existing color closure contracts, default decoded controls and one failed-case diagnosis. No screenshots/capture/input/performance qualification.','selectedTests':counts,'helperSha256':module.digest(__file__),'totalScopeDeadlineSeconds':300});module.write(OUT/'results.json',d)
raise SystemExit(runner.exit_code)
