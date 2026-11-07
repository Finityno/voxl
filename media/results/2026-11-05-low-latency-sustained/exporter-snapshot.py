from pathlib import Path
import csv,gzip,hashlib,io,json,math,os,re,statistics
ROOT=Path('$EXPORT_CHECKOUT');MEASURED=Path('$RESULTS');DEST=ROOT/'media/results/2026-11-05-low-latency-sustained';SOURCE='f098e462a62833b73d6a6103e4f50ff810fb7ffb'
def load(p):return json.loads(p.read_text())
def sha(b):return hashlib.sha256(b).hexdigest()
result=load(MEASURED/'results.json');assert result['source']==SOURCE and result['terminal']and result['exitCode']==0 and not result['unexecutedCases'];assert len(result['cases'])==10
groups=[]
for step in result['steps']:
 assert step['exitCode']==0 and step['groupGone']and not step['timedOut']and not step['cleanupSignals']
 assert sha((MEASURED/step['privateLog']).read_bytes())==step['logSha256']
 try:os.killpg(step['pgid'],0)
 except ProcessLookupError:pass
 else:raise ValueError('owned command group present')
 groups.append(step['pgid'])
assert not DEST.exists();DEST.mkdir()
def scrub(t):
 for a,b in [('/private'+str(MEASURED),'$RESULTS'),(str(MEASURED),'$RESULTS'),('$CHECKOUT','$CHECKOUT'),('$FIXTURE','$FIXTURE'),('$FIXTURE','$FIXTURE'),('$USER','$USER')]:t=t.replace(a,b)
 return t
def dump(name,v):(DEST/name).write_text(scrub(json.dumps(v,indent=2,allow_nan=False))+'\n')
index={};row_counts={'encodePayload':0,'regionSsim':0,'foregroundIncludingFloor':0}
def compressed(name,data,rows):
 blob=gzip.compress(data,compresslevel=9,mtime=0);assert gzip.decompress(blob)==data;(DEST/name).write_bytes(blob);index[name]={'sha256':sha(blob),'bytes':len(blob),'uncompressedSha256':sha(data),'uncompressedBytes':len(data),'dataRows':rows}
def csvdata(header,rows):
 f=io.StringIO(newline='');w=csv.writer(f,lineterminator='\n');w.writerow(header);w.writerows(rows);return f.getvalue().encode()
runs=[]
for number in range(1,6):
 cases=[c for c in result['cases']if c['independentRun']==number];assert len(cases)==2
 fg=load(MEASURED/f'foreground-run-{number}.json');assert fg['frameMapping']=={'captureWarmupFrames':900,'sourcePrefixFrames':30,'sourceObservedCycleFrames':240,'observedFrames':9000}
 run={'number':number,'chronologicalOrder':[c['specification']for c in cases],'cases':[],'foreground':{k:v for k,v in fg.items()if k!='variants'},'foregroundVariants':{}}
 baseline=fg['variants'][fg['baseline']]['rows']
 for c in cases:
  assert c['specification']in ['h264:compatible:3000','hevc:low-latency-compatible:12000']and c['oraclesComplete']and c['normalizedRgbIntermediateRemovedAfterAnalysis']
  assert c['accessUnits']['totalFrames']==9900 and c['accessUnits']['observedFrames']==9000 and int(c['actualDecodedMetadata']['nb_read_frames'])==9900
  samples=c['encodeLatencyMs']['samples'];sizes=c['accessUnits']['observedFrameBytes'];assert len(samples)==len(sizes)==9000
  assert sum(sizes)==c['accessUnits']['observedAnnexBBytes']and sorted(samples)[math.ceil(.95*len(samples))-1]==c['encodeLatencyMs']['p95']
  prefix='run-'+str(number)+'-'+c['specification'].split(':')[0]
  compressed(prefix+'-samples.csv.gz',csvdata(['observedFrame','captureFrame','sourceFrame','annexBBytes','encodeCallbackMs'],((i,900+i,30+i%240,b,samples[i])for i,b in enumerate(sizes))),9000);row_counts['encodePayload']+=9000
  for region in ['full','text','thin']:
   raw=(MEASURED/'private'/c['name']/(region+'.ssim')).read_bytes();vals=[float(line.split('All:')[1].split()[0])for line in raw.decode().splitlines()];assert len(vals)==9000 and math.isclose(statistics.mean(vals),c['quality'][region]['channels']['All']['mean'],abs_tol=1e-12)
   compressed(prefix+'-'+region+'.ssim.gz',raw,9000);row_counts['regionSsim']+=9000
  value=json.loads(json.dumps(c));value['encodeLatencyMs'].pop('samples');value['accessUnits'].pop('observedFrameBytes');value.pop('rgbPath',None);value['numericPrefix']=prefix
  log=(MEASURED/'private'/(c['name']+'.log')).read_text();match=re.search(r'^\s*(\d+)\s+peak memory footprint\s*$',log,re.M)
  value['additionalRawRusageObservation']={'peakProcessPhysicalFootprintBytes':int(match.group(1))if match and int(match.group(1))>0 else None,'source':'macOS time -lp peak memory footprint field','logSha256':sha(log.encode()),'scope':'Postprocessing an existing recorded rusage field. Peak probe process only, not baseline-subtracted app footprint; excludes shared OS helpers/GPU/viewer.'}
  run['cases'].append(value)
 for name,v in fg['variants'].items():
  variant='uncompressed420'if name=='uncompressed420'else name.split('-')[2];rows=v['rows'];assert len(rows)==9000
  mean=sum(sum(r['sumAbsoluteCodeErrorByRgbChannel'])for r in rows)/(9000*fg['calibration']['evaluatedPixelsIncludingHalo']*3);assert math.isclose(mean,v['meanAbsoluteForegroundRgbCodeError'],abs_tol=1e-12)
  value={k:q for k,q in v.items()if k not in ['rows','foregroundErrorWorseThanBaselineFrames','foregroundChannelErrorWorseThanBaselineFrames']}
  if name!='uncompressed420':
   count=sum(sum(r['sumAbsoluteCodeErrorByRgbChannel'])>sum(b['sumAbsoluteCodeErrorByRgbChannel'])for r,b in zip(rows,baseline));channels={key:sum(r['sumAbsoluteCodeErrorByRgbChannel'][i]>b['sumAbsoluteCodeErrorByRgbChannel'][i]for r,b in zip(rows,baseline))for i,key in enumerate(['r','g','b'])}
   assert count==len(v['foregroundErrorWorseThanBaselineFrames'])and channels=={k:len(q)for k,q in v['foregroundChannelErrorWorseThanBaselineFrames'].items()};value['worseAggregateFrames']=count;value['worseChannelFrames']=channels
  run['foregroundVariants'][variant]=value
  compressed('run-'+str(number)+'-'+variant+'-foreground.csv.gz',csvdata(['observedFrame','captureFrame','sourceFrame','rErrorSum','gErrorSum','bErrorSum','maximumForegroundCodeError'],((r['observedFrame'],r['captureFrame'],r['sourceFrame'],*r['sumAbsoluteCodeErrorByRgbChannel'],r['maxAbsoluteForegroundCodeError'])for r in rows)),9000);row_counts['foregroundIncludingFloor']+=9000
 runs.append(run)
def distribution(values):return {'individual':values,'median':statistics.median(values),'minimum':min(values),'maximum':max(values)}
summary={}
for codec in ['h264','hevc']:
 cases=[next(c for c in run['cases']if c['specification'].startswith(codec+':'))for run in runs];physical=[c['additionalRawRusageObservation']['peakProcessPhysicalFootprintBytes']for c in cases]
 summary[codec]={'independentSessions':5,'observedFrames':45000,'decodedFramesIncludingWarmup':49500,'meanAnnexBMbps':distribution([c['accessUnits']['meanAnnexBMbps']for c in cases]),'individualSessionEncodeP95Ms':distribution([c['encodeLatencyMs']['p95']for c in cases]),'wholeProbeCpuPercentOneCore':distribution([c['processResources']['averageCpuPercentOneCore']for c in cases]),'wholeProbePeakRssMiB':distribution([c['processResources']['maximumResidentBytes']/1024**2 for c in cases]),'wholeProbePeakPhysicalFootprintMiB':distribution([v/1024**2 for v in physical])if all(v is not None for v in physical)else None,'textRegionMeanYuvSsim':distribution([c['quality']['text']['channels']['All']['mean']for c in cases]),'thinForegroundMeanAbsoluteRgbError':distribution([c['foregroundAnalysis']['meanAbsoluteForegroundRgbCodeError']for c in cases]),'sameOrLowerForegroundAggregateAndChannelsAll45000Frames':all(c['foregroundAnalysis']['worseAggregateFrames']==0 and not any(c['foregroundAnalysis']['worseAggregateChannelFrames'].values())for c in cases)}
reductions=[]
for run in runs:
 a=next(c for c in run['cases']if c['specification'].startswith('h264:'));b=next(c for c in run['cases']if c['specification'].startswith('hevc:'));reductions.append(1-b['accessUnits']['observedAnnexBBytes']/a['accessUnits']['observedAnnexBBytes'])
dump('summary.json',{'source':SOURCE,'fps':30,'geometry':[1920,1080],'warmupSecondsPerSession':30,'observedSecondsPerSession':300,'independentSessionsPerCodec':5,'summary':summary,'pairedPayloadReductionFractions':reductions,'runs':runs,'rawRowCounts':row_counts,'productionAdmission':False,'scope':'Five fresh encoder processes per codec on one M5Pro/macOS27.2 and one fixed generated8s coding/motion cycle. Static-prefix warmup; repeated cycles are not independent content/devices. No statistical confidence interval, controlled thermal/power, full app/viewer/helper/GPU/physical footprint, hardware decode, capture/input, wire/relay cost or1080p60 admission. Targets/defaults unchanged.'})
validation={k:v for k,v in result.items()if k!='cases'};validation['originalResultSha256']=sha((MEASURED/'results.json').read_bytes());validation['compilerArtifact']=load(MEASURED/'compiler-artifact.json');validation['fixture']=load(MEASURED/'fixture.json');validation['metadataLogs']={}
for name in ['load','os','cpu','model','toolchain','ffmpeg-version','ffprobe-version']:
 p=MEASURED/'private'/(name+'.log');validation['metadataLogs'][name]={'sha256':sha(p.read_bytes()),'text':p.read_text()}
dump('validation.json',{'measurement':validation,'independentlyAbsentOwnedGroups':groups,'exporterSha256':sha(Path(__file__).read_bytes()),'scopeOfAbsence':'Owned command groups only, not shared OS compression services. No new unit tests credited by aggregating native sessions.'})
dump('index.json',{'compressedFiles':index,'jsonFiles':{name:{'bytes':(DEST/name).stat().st_size,'sha256':sha((DEST/name).read_bytes())}for name in ['summary.json','validation.json']}})
print(json.dumps({'summary':summary,'reductions':reductions,'rawRows':row_counts,'compressedFiles':len(index),'groups':len(groups)},indent=2))
