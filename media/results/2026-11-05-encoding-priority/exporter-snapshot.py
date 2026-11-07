from pathlib import Path
import csv,gzip,hashlib,io,json,os,statistics

ROOT=Path('$USER/Documents/GitHub/voxl-fincode-streaming')
DEST=ROOT/'media/results/2026-11-05-encoding-priority'
SHA='53cf75a21adcd3cb4d322a2492a320a767cf0495'
PATHS={'api':Path('/tmp/fincode-priority-api-53cf75a'),'receipt':Path('/tmp/fincode-priority-api-receipt-53cf75a'),'priority-pilot':Path('/tmp/fincode-codecs-priority-pilot-53cf75a'),'low-latency-pilot':Path('/tmp/fincode-codecs-low-latency-pilot-53cf75a'),'partial-analysis':Path('/tmp/fincode-codecs-low-latency-analysis-53cf75a')}
assert not DEST.exists();DEST.mkdir()
def sha(b):return hashlib.sha256(b).hexdigest()
def load(p):return json.loads(p.read_text())
aliases=[(str(v),'$'+k.upper().replace('-','_'))for k,v in PATHS.items()]+[('/private'+str(v),'$'+k.upper().replace('-','_'))for k,v in PATHS.items()]
aliases+= [('$USER/Documents/GitHub/voxl-priority-native-validation','$CHECKOUT'),('$USER/Documents/GitHub/voxl-desktop-native-validation','$LEGACY_CHECKOUT'),('/private/tmp/fincode-voxl-text-2a31641/fixture','$FIXTURE'),('/tmp/fincode-voxl-text-2a31641/fixture','$FIXTURE'),(str(Path.home()),'$USER')]
aliases.sort(key=lambda p:len(p[0]),reverse=True)
def scrub(t):
 for a,b in aliases:t=t.replace(a,b)
 return t
def dump(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(scrub(json.dumps(v,indent=2,allow_nan=False))+'\n')
def compressed(p,b):p.parent.mkdir(parents=True,exist_ok=True);blob=gzip.compress(b,compresslevel=9,mtime=0);assert gzip.decompress(blob)==b;p.write_bytes(blob)
groups=[];provenance={}
for label,root in PATHS.items():
 out=DEST/label
 for f in root.rglob('*.json'):
  if 'private'in f.relative_to(root).parts:continue
  data=load(f);rel=f.relative_to(root)
  provenance[label+'/'+str(rel)]={'originalSha256':sha(f.read_bytes())}
  if f.name in ['results.json','queue.json']:
   assert data['source']==SHA and data['terminal']
   for step in data['steps']:
    assert step['groupGone']
    try:os.killpg(step['pgid'],0)
    except ProcessLookupError:pass
    else:raise ValueError('command group still present')
    groups.append(step['pgid']);log=root/rel.parent/step['privateLog'];raw=log.read_bytes();assert sha(raw)==step['logSha256']
    target=out/rel.parent/'logs'/(step['name']+'.log.gz');compressed(target,scrub(raw.decode()).encode());step['publishedLog']=str(target.relative_to(out/rel.parent))
  if isinstance(data,dict):
   for c in data.get('cases',[]):
    if isinstance(c,dict)and 'encodeLatencyMs'in c:
     c['encodeLatencyMs'].pop('samples',None);c['accessUnits'].pop('observedFrameBytes',None)
   if 'encodeLatencyMs'in data:data['encodeLatencyMs'].pop('samples',None);data['accessUnits'].pop('observedFrameBytes',None)
   if 'variants'in data:
    for value in data['variants'].values():value.pop('rows',None)
  dump(out/rel,data)
 for f in [root/'run.py',root/'private/failure.txt']:
  if f.exists():
   target=out/f.name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(scrub(f.read_text()));provenance[label+'/'+f.name]={'originalSha256':sha(f.read_bytes())}

tables={}
for label in ['priority-pilot','low-latency-pilot']:
 root=PATHS[label];result=load(root/'results.json');tables[label]=[]
 fg=load(root/'foreground-run-1.json')if label=='priority-pilot'else load(PATHS['partial-analysis']/'foreground.json')
 if label=='low-latency-pilot':dump(DEST/label/'partial-foreground.json',{**fg,'variants':{k:{q:v for q,v in val.items()if q!='rows'}for k,val in fg['variants'].items()}})
 for c in result['cases']:
  assert c['encodingAndDecodeOraclesComplete']
  path=DEST/label/(c['name']+'-samples.csv.gz');stream=io.StringIO(newline='');writer=csv.writer(stream,lineterminator='\n');writer.writerow(['observedFrame','captureFrame','sourceFrame','annexBBytes','encodeCallbackMs'])
  writer.writerows((i,30+i,30+i%240,b,c['encodeLatencyMs']['samples'][i])for i,b in enumerate(c['accessUnits']['observedFrameBytes']));compressed(path,stream.getvalue().encode())
  for region in ['full','text','thin']:compressed(DEST/label/(c['name']+'-'+region+'.ssim.gz'),(root/'private'/c['name']/(region+'.ssim')).read_bytes())
  f=fg['variants'][c['name']]
  tables[label].append({'case':c['specification'],'observedFrames':240,'decodedFramesIncludingWarmup':int(c['actualDecodedMetadata']['nb_read_frames']),'observedBytes':c['accessUnits']['observedAnnexBBytes'],'meanAnnexBMbps':c['accessUnits']['meanAnnexBMbps'],'encodeP95Ms':c['encodeLatencyMs']['p95'],'meanForegroundRgbError':f['meanAbsoluteForegroundRgbCodeError'],'aggregateWorseFramesThanH264Baseline':len(f['foregroundErrorWorseThanBaselineFrames']),'channelWorseFramesThanH264Baseline':{k:len(v)for k,v in f['foregroundChannelErrorWorseThanBaselineFrames'].items()},'encodingPriority':c['encodingPriority'],'encodedSha256':c['encodedSha256'],'fullyCompletedOriginalMatrixCase':c['oraclesComplete'],'postFailureAnalysis':label=='low-latency-pilot'})
 for name,v in fg['variants'].items():
  stream=io.StringIO(newline='');w=csv.writer(stream,lineterminator='\n');w.writerow(['observedFrame','captureFrame','sourceFrame','rErrorSum','gErrorSum','bErrorSum','maximumForegroundCodeError'])
  w.writerows((r['observedFrame'],30+r['observedFrame'],r['sourceFrame'],*r['sumAbsoluteCodeErrorByRgbChannel'],r['maxAbsoluteForegroundCodeError'])for r in v['rows']);compressed(DEST/label/(name+'-foreground.csv.gz'),stream.getvalue().encode())
 if label=='priority-pilot':
  comparisons={}
  for codec in ['h264','hevc']:
   base='run-1-'+codec+'-compatible-'+('3000'if codec=='h264'else'1500');a=fg['variants'][base];b=fg['variants'][base+'-speed']
   comparisons[codec]={'speedWorseAggregateFramesVsSameCodecDefault':sum(sum(x['sumAbsoluteCodeErrorByRgbChannel'])>sum(y['sumAbsoluteCodeErrorByRgbChannel'])for x,y in zip(b['rows'],a['rows'])),'speedWorseChannelFramesVsSameCodecDefault':{ch:sum(x['sumAbsoluteCodeErrorByRgbChannel'][i]>y['sumAbsoluteCodeErrorByRgbChannel'][i]for x,y in zip(b['rows'],a['rows']))for i,ch in enumerate(['r','g','b'])}}
  dump(DEST/label/'same-codec-comparisons.json',comparisons)

api=load(PATHS['api']/'results.json');native=load(PATHS['api']/'native-priority.json');queue=load(PATHS['api']/'queue.json')
assert api['exitCode']==0 and queue['exitCode']==1 and queue['steps'][0]['exitCode']==0
assert len(native['cases'])==8 and sum(c['availableAndDecoded']for c in native['cases'])==4
assert all(load(PATHS['receipt']/n/'results.json')['exitCode']==0 for n in ['worker','controller'])
assert load(PATHS['priority-pilot']/'results.json')['exitCode']==0
ll=load(PATHS['low-latency-pilot']/'results.json');assert ll['exitCode']==1 and ll['steps'][-1]['name']=='run-1-hevc-low-latency-compatible-1500'
dump(DEST/'summary.json',{'source':SHA,'hardware':'Apple M5 Pro, Mac17,8; macOS27.2 26B5101f','api':{'rustSelectedTestsPassed':1,'pythonSelectedTestsPassed':5,'regularPriorityModesDecoded':4,'explicitPriorityLowLatencyCombinationsUnavailable':4,'defaultControlDecodedFramesTotal':192,'allSuccessfulNativeAvailabilityAndDefaultControlDecodedFrames':320,'initialQueueExitCode':1,'initialInnerExitCode':0,'correctedControllerAndWorkerExitCodes':[0,0]},'pilots':tables,'ownedCommandGroupsIndependentlyAbsent':groups,'productionAdmission':False,'scope':'One short 1s warmup +8s observed fixed generated 1080p30 clip per pilot case; not steady-state repeatability or platform/profile qualification. Original low-latency matrix failed, three completed cases separately analyzed without encoding again. Speed and H264 low-latency candidates regress foreground quality; no defaults/targets changed.'})
dump(DEST/'provenance.json',{'originalArtifacts':provenance,'executedExporterSha256':sha(Path(__file__).read_bytes()),'normalization':'Only paths are normalized in logs/helpers/JSON. Original hashes retained; CSV/SSIM numeric data unchanged. No bitstreams, font, screen/input, decoded images or private app source.'})
dump(DEST/'index.json',{'files':{str(p.relative_to(DEST)):{'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size,**({'uncompressedSha256':sha(gzip.decompress(p.read_bytes())),'uncompressedBytes':len(gzip.decompress(p.read_bytes()))}if p.name.endswith('.gz')else{})}for p in sorted(DEST.rglob('*'))if p.is_file()}})
print(json.dumps({'files':len(load(DEST/'index.json')['files']),'groups':len(groups),'pilotCases':tables}))
