from pathlib import Path
from types import SimpleNamespace
import hashlib,importlib.util,json,os,re,signal,sys
ROOT=Path('$CHECKOUT');SHA='ef7ed1071728a84ec439f7d0e78c4d42d5e7673f';OUT=Path('$INITIAL_RESULTS');DRIVER=Path('$OLD_CHECKOUT/media/tools/run-matched-codecs.py');OLD=Path('$OLD_CHECKOUT/media/target/release/examples/codec_probe')
spec=importlib.util.spec_from_file_location('bounded',DRIVER);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
assert not OUT.exists();OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
args=SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=1,case=[],warmup_seconds=0,observation_seconds=1);runner=m.Runner(args);counts={};availability=[];controls=[];executable=None
old_artifact=json.loads(Path('$OLD_NATIVE_RESULTS/compiler-artifact.json').read_text());assert m.digest(OLD)==old_artifact['executableSha256']
def stop(a,b):raise KeyboardInterrupt()
def expire(a,b):raise TimeoutError('1200 second QP contract and availability budget')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);signal.signal(signal.SIGALRM,expire);signal.alarm(1200)
try:
 runner.run('load',['uptime'],10);runner.run('toolchain',['cargo','--version'],15)
 executable=runner.build()
 common=['--locked','--manifest-path','media/Cargo.toml','--lib','-j2','-q','--message-format=short']
 runner.run('unit-build',['cargo','test',*common,'--no-run'],300,True)
 for name,filter,count in [('qp-contracts','maximum_frame_qp_',2),('priority-compatibility','encoding_priority_requires_hardware_',1)]:
  log=runner.run(name,['cargo','test',*common,filter,'--','--nocapture'],45,True);matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log.read_text());assert matches==[(str(count),'0','0')];counts[name]={'passed':count,'failed':0,'ignored':0}
 runner.run('test-clippy',['cargo','clippy','--locked','--manifest-path','media/Cargo.toml','--lib','--tests','-j2','-q','--message-format=short','--','-D','warnings'],300,True)
 runner.run('profile-contracts',[sys.executable,'-B','media/tools/test-run-matched-codecs.py','CodecEvidenceTests.test_profile_priority_is_explicit_without_default_alias_or_codec_substitution','CodecEvidenceTests.test_maximum_qp_is_bounded_explicit_and_does_not_replace_the_requested_priority'],30,True);counts['profile-contracts']={'passed':2,'failed':0,'scope':'selected pure Python profile grammar only'}
 for codec in ['h264','hevc']:
  for tuning in ['compatible','low-latency-compatible']:
   for limit in [28,36]:
    name=f'{codec}-{tuning}-qp{limit}';video=OUT/'private'/(name+'.'+codec);framed=OUT/'private'/(name+'.vxfr');command=[str(executable),'hardware',str(video),codec,str(framed),'--frames=32','--width=320','--height=180','--fps=60','--bitrate-kbps=12000','--pixel-range=full','--color-tags=srgb','--tuning='+tuning,'--max-frame-qp='+str(limit),'--force-keyframe-period=16'];record={'name':name,'codec':codec,'tuning':tuning,'requestedMaxFrameQp':limit}
    try:log=runner.run(name,command,15,True)
    except ValueError:
     step=runner.steps[-1];text=(OUT/step['privateLog']).read_text();assert step['exitCode']==1 and not step['timedOut']and not step['cleanupSignals']
     assert re.search(r'Error: Platform \{ operation: "(?:maximum frame QP|query maximum frame QP)", code: -12900 \}',text),text
     record.update({'supported':False,'nativeUnsupportedProperty':True,'exitCode':step['exitCode']});availability.append(record);continue
    text=log.read_text();assert f'max_frame_qp={limit} qp_property_verified=true'in text
    units=m.access_units(framed,codec,32,0,60);assert units['totalFrames']==32 and units['keyframeIndices']==[0,16]
    decoded=runner.run(name+'-decode',['ffprobe','-v','error','-threads','2','-count_frames','-show_entries','stream=codec_name,width,height,nb_read_frames,color_range,color_space,color_transfer,color_primaries','-of','json',str(video)],30);meta=json.loads(decoded.read_text())['streams'];assert len(meta)==1 and meta[0]['nb_read_frames']=='32'and meta[0]['codec_name']==codec
    record.update({'supported':True,'nativePropertyVerified':True,'actualDecodedMetadata':meta[0],'accessUnits':units,'videoSha256':m.digest(video),'framedSha256':m.digest(framed)});availability.append(record)
 for mode,codec in [('hardware','h264'),('hardware','hevc'),('software','h264')]:
  pair=[]
  for tag,exe in [('old',OLD),('new',executable)]:
   name=f'control-{mode}-{codec}-{tag}';video=OUT/'private'/(name+'.'+codec);rgb=OUT/'private'/(name+'.rgb')
   log=runner.run(name,[str(exe),mode,str(video),codec,'--frames=32','--width=320','--height=180','--fps=30','--bitrate-kbps=2500'],15,True);assert 'max_frame_qp='not in log.read_text()
   runner.run(name+'-rgb',['ffmpeg','-nostdin','-hide_banner','-loglevel','error','-threads','2','-i',str(video),'-frames:v','32','-pix_fmt','rgb24','-f','rawvideo',str(rgb)],30);assert rgb.stat().st_size==320*180*3*32
   pair.append({'tag':tag,'decodedRgbSha256':m.digest(rgb),'encodedSha256':m.digest(video),'decodedFrames':32,'executableSha256':m.digest(exe)})
  assert pair[0]['decodedRgbSha256']==pair[1]['decodedRgbSha256'];controls.append({'mode':mode,'codec':codec,'decodedControlsMatch':True,'versions':pair})
 runner.run('diff',['git','diff','--check','45a9dcdd04c1aff032a0d4f7f5f7e5a070ace3c7',SHA],15)
 runner.exit_code=0
except BaseException as error:
 (OUT/'private/failure.txt').write_text(type(error).__name__+': '+str(error)+'\n');print(type(error).__name__+': '+str(error),flush=True);runner.exit_code=130 if isinstance(error,KeyboardInterrupt)else 1
finally:
 signal.alarm(0);runner.terminal=True;runner.save();d=json.loads((OUT/'results.json').read_text())
 for key in ['plan','cases','unsupportedCurrentNativeProfiles','unexecutedCases','limits']:d.pop(key,None)
 d.update({'scope':'Scoped Mac QP integer/range/hardware contracts, exact native setter/readback availability and tiny independent decode plus old/current decoded default controls only. Not a matched quality/performance experiment, actual frame-QP observation, physical endpoint or app profile admission. Native unsupported properties retain their actual nonzero exit.','selectedTests':counts,'availability':availability,'defaultControls':controls,'helperSha256':m.digest(__file__),'boundedDriverSha256':m.digest(DRIVER),'oldControlSource':'f098e462a62833b73d6a6103e4f50ff810fb7ffb','oldExecutableSha256':m.digest(OLD),'newExecutableSha256':m.digest(executable)if executable else None,'totalDeadlineSeconds':1200});m.write(OUT/'results.json',d)
raise SystemExit(runner.exit_code)
