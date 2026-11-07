from pathlib import Path
from types import SimpleNamespace
import hashlib,importlib.util,json,os,re,signal,sys
ROOT=Path('$CHECKOUT');SHA='ef7ed1071728a84ec439f7d0e78c4d42d5e7673f';OUT=Path('$NATIVE_RESULTS');DRIVER=Path('$OLD_CHECKOUT/media/tools/run-matched-codecs.py');OLD=Path('$OLD_CHECKOUT/media/target/release/examples/codec_probe')
spec=importlib.util.spec_from_file_location('bounded',DRIVER);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
assert not OUT.exists();OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
args=SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=1,case=[],warmup_seconds=0,observation_seconds=1,ffmpeg=Path('/opt/homebrew/bin/ffmpeg').resolve(),ffprobe=Path('/opt/homebrew/bin/ffprobe').resolve());runner=m.Runner(args);counts={};availability=[];controls=[];executable=None
old_artifact=json.loads(Path('$OLD_NATIVE_RESULTS/compiler-artifact.json').read_text());assert m.digest(OLD)==old_artifact['executableSha256']
def stop(a,b):raise KeyboardInterrupt()
def expire(a,b):raise TimeoutError('1200 second QP contract and availability budget')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);signal.signal(signal.SIGALRM,expire);signal.alarm(1200)
def availability_units(path,codec):
 sizes=[];keys=[]
 with path.open('rb')as stream:
  assert stream.read(6)==b'VXFR\x01'+bytes([0 if codec=='h264'else 1])
  for index in range(32):
   header=stream.read(13);assert len(header)==13
   pts=int.from_bytes(header[:8],'big',signed=True);flag=header[8];size=int.from_bytes(header[9:],'big');assert pts==index*1_000_000//60 and flag in [0,1]and 0<size<=8*1024*1024
   data=stream.read(size);assert len(data)==size;types=[];offset=0
   while True:
    offset=data.find(b'\0\0\0\1',offset)
    if offset<0:break
    assert offset+4<len(data);types.append(data[offset+4]&31 if codec=='h264'else(data[offset+4]>>1)&63);offset+=4
   key=5 in types if codec=='h264'else any(16<=v<=21 for v in types);assert bool(flag)==key
   if key:assert set([7,8]if codec=='h264'else[32,33,34])<=set(types);keys.append(index)
   sizes.append(size)
  assert not stream.read(1)
 return {'totalFrames':32,'observedFrames':32,'keyframeIndices':keys,'totalAnnexBBytes':sum(sizes),'frameBytes':sizes,'scope':'Availability-only32frames at60fps; no complete one-second bitrate window or performance percentile inferred.'}
try:
 artifact=json.loads(Path('$CONTRACTS_RESULTS/compiler-artifact.json').read_text());executable=Path(artifact['artifact']['executable']);assert m.digest(executable)==artifact['executableSha256']
 for codec in ['h264','hevc']:
  for tuning in ['compatible','low-latency-compatible']:
   for limit in [28,36]:
    name=f'{codec}-{tuning}-qp{limit}';video=OUT/'private'/(name+'.'+codec);framed=OUT/'private'/(name+'.vxfr');command=[str(executable),'hardware',str(video),codec,str(framed),'--frames=32','--width=320','--height=180','--fps=60','--bitrate-kbps=12000','--pixel-range=full','--color-tags=srgb','--tuning='+tuning,'--max-frame-qp='+str(limit),'--force-keyframe-period=16'];record={'name':name,'codec':codec,'tuning':tuning,'requestedMaxFrameQp':limit}
    try:
     if name=='h264-compatible-qp28':
      prior=Path('$CONTRACTS_RESULTS');receipt=json.loads((prior/'results.json').read_text());assert receipt['terminal']and receipt['exitCode']==1
      step=next(x for x in receipt['steps']if x['name']==name);assert step['exitCode']==0 and step['groupGone']
      video=prior/'private'/(name+'.'+codec);framed=prior/'private'/(name+'.vxfr');log=prior/step['privateLog'];assert m.digest(log)==step['logSha256'];record['retainedEarlierNativeExecution']={'receiptSha256':m.digest(prior/'results.json'),'nativeLogSha256':m.digest(log),'nativeGroupGone':True,'scope':'Decode/postprocessing only, no encoder or earlier test replay.'}
     else:log=runner.run(name,command,15,True)
    except ValueError:
     step=runner.steps[-1];text=(OUT/step['privateLog']).read_text();assert step['exitCode']==1 and not step['timedOut']and not step['cleanupSignals']
     assert re.search(r'Error: Platform \{ operation: "(?:maximum frame QP|query maximum frame QP)", code: -12900 \}',text),text
     record.update({'supported':False,'nativeUnsupportedProperty':True,'exitCode':step['exitCode']});availability.append(record);continue
    text=log.read_text();assert f'max_frame_qp={limit} qp_property_verified=true'in text
    units=availability_units(framed,codec);assert units['totalFrames']==32 and units['keyframeIndices']==[0,16]
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
 d.update({'scope':'Native continuation after retained subsecond-window bookkeeping failure; no unit/build/Clippy replay. Exact native setter/readback availability and tiny independent decode plus old/current decoded default controls only. Not a matched quality/performance experiment, actual frame-QP observation, physical endpoint or app profile admission. Native unsupported properties retain their actual nonzero exit.','selectedTests':counts,'earlierContractReceiptSha256':m.digest('$CONTRACTS_RESULTS/results.json'),'availability':availability,'defaultControls':controls,'helperSha256':m.digest(__file__),'boundedDriverSha256':m.digest(DRIVER),'oldControlSource':'f098e462a62833b73d6a6103e4f50ff810fb7ffb','oldExecutableSha256':m.digest(OLD),'newExecutableSha256':m.digest(executable)if executable else None,'totalDeadlineSeconds':1200});m.write(OUT/'results.json',d)
raise SystemExit(runner.exit_code)
