from pathlib import Path
import csv,gzip,hashlib,io,json,math,os,statistics
ROOT=Path(os.environ['VOXL_SOURCE_WORKTREE']).resolve();PILOT=Path(os.environ['VOXL_PILOT_RESULTS']).resolve();REPEAT=Path(os.environ['VOXL_REPEATABILITY_RESULTS']).resolve();DEST=ROOT/'media/results/2026-11-05-codec-repeatability';SOURCE='f1f08a424d7e405543f4088c522c19f5c49b22bc'
assert not DEST.exists();DEST.mkdir()
def sha(data):return hashlib.sha256(data).hexdigest()
def load(p):return json.loads(p.read_text())
def scrub(value):
 if isinstance(value,str):
  for a,b in [(str(PILOT),'$PILOT'),(str(REPEAT),'$REPEAT'),(str(Path(os.environ['VOXL_FROZEN_CHECKOUT']).resolve()),'$CHECKOUT'),(str(Path.home()),'$USER')]:value=value.replace(a,b)
  return value
 if isinstance(value,list):return [scrub(v)for v in value]
 if isinstance(value,dict):return {k:scrub(v)for k,v in value.items()}
 return value
def write(name,d):(DEST/name).write_text(json.dumps(scrub(d),indent=2,allow_nan=False)+'\n')
index={}
def compressed(name,data,rows):
 blob=gzip.compress(data,compresslevel=9,mtime=0);assert gzip.decompress(blob)==data;(DEST/name).write_bytes(blob);index[name]={'bytes':len(blob),'sha256':sha(blob),'uncompressedBytes':len(data),'uncompressedSha256':sha(data),'dataRows':rows}
def csvbytes(header,rows):
 stream=io.StringIO(newline='');writer=csv.writer(stream,lineterminator='\n');writer.writerow(header);writer.writerows(rows);return stream.getvalue().encode()
def absent(pid):
 try:os.killpg(pid,0);return False
 except ProcessLookupError:return True
pilot,repeat=load(PILOT/'results.json'),load(REPEAT/'results.json')
for result in [pilot,repeat]:
 assert result['source']==SOURCE and result['terminal']and result['exitCode']==0 and not result['unexecutedCases']
 assert all(s['groupGone']and absent(s['pgid'])for s in result['steps'])
assert len(pilot['cases'])==2 and len(repeat['cases'])==8
assert load(PILOT/'compiler-artifact.json')['executableSha256']==load(REPEAT/'compiler-artifact.json')['executableSha256']
runs=[];groups=[s['pgid']for d in [pilot,repeat]for s in d['steps']];selections=[('pilot',PILOT,1,pilot)]+[('repeat',REPEAT,n,repeat)for n in range(1,5)]
for label,folder,number,result in selections:
 records=[c for c in result['cases']if c['independentRun']==number];assert len(records)==2
 foreground=load(folder/f'foreground-run-{number}.json');assert foreground['frameMapping']=={'captureWarmupFrames':900,'sourcePrefixFrames':30,'sourceObservedCycleFrames':240,'observedFrames':9000}
 run={'name':f'{label}-{number}','source':SOURCE,'chronologicalCaseOrder':[c['specification']for c in records],'cases':[],'foreground':{k:v for k,v in foreground.items()if k!='variants'},'foregroundVariants':{}}
 for c in records:
  assert c['oraclesComplete']and c['accessUnits']['totalFrames']==9900 and c['accessUnits']['observedFrames']==9000 and int(c['actualDecodedMetadata']['nb_read_frames'])==9900 and c['hardwareSelection']=='QueriedHardware'
  assert len(c['encodeLatencyMs']['samples'])==len(c['accessUnits']['observedFrameBytes'])==9000
  codec=c['specification'].split(':')[0];prefix=run['name']+'-'+codec
  if label=='repeat':
   data=csvbytes(['observedFrame','captureFrame','sourceFrame','annexBBytes','encodeCallbackMs'],((i,900+i,30+i%240,size,c['encodeLatencyMs']['samples'][i])for i,size in enumerate(c['accessUnits']['observedFrameBytes'])));compressed(prefix+'-samples.csv.gz',data,9000)
   for region in ['full','text','thin']:
    data=(folder/'private'/c['name']/(region+'.ssim')).read_bytes();assert len(data.splitlines())==9000;compressed(prefix+'-'+region+'.ssim.gz',data,9000)
  else:
   run.setdefault('existingRawFiles',[]).append('../2026-11-05-sustained-codecs/'+c['name']+'-samples.csv')
   run['existingRawFiles'] += ['../2026-11-05-sustained-codecs/'+c['name']+'-'+region+'.ssim'for region in ['full','text','thin']]
  # Preserve individual p95s and distributions; pooling frames is not a new run.
  record=json.loads(json.dumps(c));record['encodeLatencyMs'].pop('samples');record['accessUnits'].pop('observedFrameBytes');record.pop('rgbPath',None);record['numericPrefix']=prefix;run['cases'].append(record)
 for name,v in foreground['variants'].items():
  variant='uncompressed420'if name=='uncompressed420'else name.split('-')[2]
  if label=='repeat':
   data=csvbytes(['observedFrame','captureFrame','sourceFrame','rErrorSum','gErrorSum','bErrorSum','maximumForegroundCodeError'],((row['observedFrame'],row['captureFrame'],row['sourceFrame'],*row['sumAbsoluteCodeErrorByRgbChannel'],row['maxAbsoluteForegroundCodeError'])for row in v['rows']));compressed(run['name']+'-'+variant+'-foreground.csv.gz',data,9000)
  else:run.setdefault('existingRawFiles',[]).append('../2026-11-05-sustained-codecs/'+name+'-foreground.csv')
  value={k:q for k,q in v.items()if k not in ['rows','foregroundErrorWorseThanBaselineFrames','foregroundChannelErrorWorseThanBaselineFrames']};
  if name!='uncompressed420':
   value['worseAggregateFrames']=len(v['foregroundErrorWorseThanBaselineFrames']);value['worseChannelFrames']={k:len(q)for k,q in v['foregroundChannelErrorWorseThanBaselineFrames'].items()}
  run['foregroundVariants'][variant]=value
 runs.append(run)
summary={}
for codec in ['h264','hevc']:
 cases=[next(c for c in r['cases']if c['specification'].startswith(codec+':'))for r in runs];assert len(cases)==5
 def distribution(values):return {'median':statistics.median(values),'minimum':min(values),'maximum':max(values),'individual':values}
 sums={c['accessUnits']['observedAnnexBBytes']for c in cases};assert len(sums)==1
 summary[codec]={'independentSessions':5,'observedFrames':45000,'decodedFramesIncludingWarmup':49500,'annexBMeanMbps':distribution([c['accessUnits']['meanAnnexBMbps']for c in cases]),'individualSessionEncodeP95Ms':distribution([c['encodeLatencyMs']['p95']for c in cases]),'wholeProbeCpuPercentOneCore':distribution([c['processResources']['averageCpuPercentOneCore']for c in cases]),'wholeProbePeakRssMiB':distribution([c['processResources']['maximumResidentBytes']/1024**2 for c in cases]),'textRegionMeanYuvSsim':distribution([c['quality']['text']['channels']['All']['mean']for c in cases]),'thinForegroundMeanAbsoluteRgbError':distribution([c['foregroundAnalysis']['meanAbsoluteForegroundRgbCodeError']for c in cases]),'sameAggregateChannelNonregressionAll45000Frames':all(c['foregroundAnalysis']['worseAggregateFrames']==0 and not any(c['foregroundAnalysis']['worseAggregateChannelFrames'].values())for c in cases),'nativePriorityObservation':'Unspecified framework default; property not queried at this source.'}
reductions=[]
for r in runs:
 a=next(c for c in r['cases']if c['specification']=='h264:compatible:3000');b=next(c for c in r['cases']if c['specification']=='hevc:compatible:1500');reductions.append(1-b['accessUnits']['observedAnnexBBytes']/a['accessUnits']['observedAnnexBBytes'])
write('summary.json',{'source':SOURCE,'warmupSecondsPerSession':30,'observedSecondsPerSession':300,'geometry':[1920,1080,30],'sessionsPerCodec':5,'summary':summary,'pairedObservedAnnexBReductionFractions':reductions,'runs':runs,'rawRowCountsIncludingExistingPilot':{'encodePayload':90000,'regionSsim':270000,'foregroundIncludingFloor':135000},'productionAdmission':False,'limits':'One M5 Pro/macOS27.2 and one fixed 8s generated code/motion cycle. Fresh independent encoder processes; not independent content/devices. No statistical CI, controlled thermal/power, app/viewer/helpers/GPU/physical footprint/actual wire/relay or codec admission. Callback latency excludes source read/fill; CPU/RSS include I/O/init/warmup. Per-run p95 distributions are retained separately.'})
validation={}
for label,folder,result in [('pilot',PILOT,pilot),('repeat',REPEAT,repeat)]:
 value={k:v for k,v in result.items()if k!='cases'};value['originalResultsSha256']=sha((folder/'results.json').read_bytes());value['compilerArtifact']=load(folder/'compiler-artifact.json');value['fixture']=load(folder/'fixture.json');value['metadataLogs']={}
 for name in ['load','os','cpu','model','toolchain','ffmpeg-version','ffprobe-version']:
  path=folder/'private'/(name+'.log');value['metadataLogs'][name]={'sha256':sha(path.read_bytes()),'text':path.read_text()}
 validation[label]=value
write('validation.json',{'scope':'Two completed native measurement harnesses; no new tests claimed by aggregating five sessions','runs':validation,'comparisonGroupsIndependentlyAbsent':groups,'scopeOfAbsence':'Only owned command groups, not shared OS compression services','exporterSha256':sha(Path(__file__).read_bytes())})
write('index.json',{'source':SOURCE,'gzipFormat':'Deterministic gzip, mtime0; CSV headers and raw numeric SSIM rows. Existing pilot files are referenced, not duplicated.','compressedFiles':index,'jsonFiles':{name:{'sha256':sha((DEST/name).read_bytes()),'bytes':(DEST/name).stat().st_size}for name in ['summary.json','validation.json']}})
print(json.dumps({'sessionsPerCodec':5,'reductionFractions':reductions,'summary':summary,'ownedGroupsAbsent':len(groups),'compressedFiles':len(index),'compressedBytes':sum(v['bytes']for v in index.values())},indent=2))
