from pathlib import Path
from types import SimpleNamespace
import importlib.util,json,signal,struct
ROOT=Path('$CHECKOUT');SHA='f098e462a62833b73d6a6103e4f50ff810fb7ffb'
PRIOR=Path('$INITIAL');OUT=Path('$COMPLETION');assert not OUT.exists();OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
spec=importlib.util.spec_from_file_location('driver',ROOT/'media/tools/run-matched-codecs.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
args=SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=1,case=[],warmup_seconds=0,observation_seconds=1);runner=module.Runner(args)
def stop(a,b):raise KeyboardInterrupt()
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
try:
 prior=json.loads((PRIOR/'results.json').read_text());assert prior['source']==SHA and prior['terminal']and prior['exitCode']==1
 assert prior['selectedTests']=={'callback_':{'passed':3,'failed':0,'ignored':0},'color_configuration_':{'passed':4,'failed':0,'ignored':0}}
 assert all(s['groupGone']and not module.group_exists(s['pgid'])for s in prior['steps'])
 for s in prior['steps']:assert module.digest(PRIOR/s['privateLog'])==s['logSha256']
 assert all(s['exitCode']==0 for s in prior['steps'][:-1]);step=prior['steps'][-1];assert step['name']=='diagnose-hevc-low-latency'and step['exitCode']==1 and not step['timedOut']and not step['cleanupSignals']
 log=(PRIOR/step['privateLog']).read_text();assert 'Error: Platform { operation: "encoder dropped output", code: 2 }'in log
 video=PRIOR/'private/reproduced.hevc';framed=PRIOR/'private/reproduced.vxfr';blob=framed.read_bytes();assert blob[:6]==b'VXFR\x01\x01';offset=6;sizes=[];timestamps=[]
 while offset<len(blob):
  assert offset+13<=len(blob);pts,key,size=struct.unpack_from('>qBI',blob,offset);offset+=13;assert key in [0,1]and size>0 and offset+size<=len(blob);sizes.append(size);offset+=size;timestamps.append(pts)
 assert timestamps==[i*1000000//30 for i in range(len(timestamps))]and sum(sizes)==video.stat().st_size
 decoded=runner.run('decode-retained-prefix',['/opt/homebrew/bin/ffprobe','-v','error','-threads','2','-count_frames','-select_streams','v:0','-show_entries','stream=codec_name,width,height,nb_read_frames','-of','json',str(video)],30,True)
 metadata=json.loads(decoded.read_text())['streams'];assert len(metadata)==1 and int(metadata[0]['nb_read_frames'])==len(timestamps)
 controls=json.loads((PRIOR/'default-controls.json').read_text());assert controls['allThreeDecodedPairsMatch']and len(controls['controls'])==6
 module.write(OUT/'diagnosis.json',{'source':SHA,'nativeExitCode':1,'reportedFailure':'Platform { operation: "encoder dropped output", code: 2 }','completePrefixFrames':len(timestamps),'plannedFrames':270,'actualPrefixDecode':metadata[0],'encodedSha256':module.digest(video),'framedSha256':module.digest(framed),'sizes':sizes,'timestampsMicros':timestamps,'productionAdmission':False,'scope':'Retained prefix from failed low-latency HEVC case; no encode replay or complete profile qualification.'})
 module.write(OUT/'receipt-correction.json',{'source':SHA,'initialSupervisorExitCode':1,'reason':'Postprocessing started at byte 0 instead of skipping the six-byte VXFR version/codec header. Native build/Clippy, seven selected tests, default decoded controls and the native dropped-output result are valid and retained.','originalArtifacts':{name:module.digest(PRIOR/name)for name in ['results.json','run.py','default-controls.json','compiler-artifact.json','previous-artifact.json']},'allOriginalGroupsIndependentlyAbsent':[s['pgid']for s in prior['steps']],'noBuildTestOrEncoderRerun':True,'scope':'Correct analysis of existing failure evidence; no missing result converted into pass.'})
 runner.run('diff',['git','diff','--check','21a80a6c26cbdffea6a019a1665ca83f97565adf',SHA],10)
 runner.exit_code=0
finally:
 runner.terminal=True;runner.save();d=json.loads((OUT/'results.json').read_text());d.update({'scope':'Bounded receipt correction and independent decode of retained failure prefix only; original supervisor/native case failures remain.','helperSha256':module.digest(__file__)});module.write(OUT/'results.json',d)
raise SystemExit(runner.exit_code)
