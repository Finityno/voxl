from pathlib import Path
import csv,gzip,hashlib,io,json,math,os,statistics
ROOT=Path('$USER/Documents/GitHub/voxl-fincode-streaming');DEST=ROOT/'media/results/2026-11-05-low-latency-budgets';SHA='f098e462a62833b73d6a6103e4f50ff810fb7ffb'
PATHS={'availability':Path('/tmp/fincode-hevc-latency-bitrates-f098e46'),'matched-3m':Path('/tmp/fincode-codecs-low-latency-budget-pilot-f098e46'),'matched-6m-12m':Path('/tmp/fincode-codecs-low-latency-higher-pilot-f098e46')}
assert not DEST.exists()
for p in PATHS.values():
 r=json.loads((p/'results.json').read_text());assert r['source']==SHA and r['terminal']and r['exitCode']==0
DEST.mkdir()
def sha(b):return hashlib.sha256(b).hexdigest()
def load(p):return json.loads(p.read_text())
aliases=[(str(v),'$'+k.upper().replace('-','_'))for k,v in PATHS.items()]+[('/private'+str(v),'$'+k.upper().replace('-','_'))for k,v in PATHS.items()]
aliases += [('$USER/Documents/GitHub/voxl-priority-native-validation','$CHECKOUT'),('/private/tmp/fincode-voxl-text-2a31641/fixture','$FIXTURE'),('/tmp/fincode-voxl-text-2a31641/fixture','$FIXTURE'),('$USER','$USER')];aliases.sort(key=lambda p:len(p[0]),reverse=True)
def scrub(t):
 for a,b in aliases:t=t.replace(a,b)
 return t
def dump(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(scrub(json.dumps(v,indent=2,allow_nan=False))+'\n')
def compressed(p,raw):
 p.parent.mkdir(parents=True,exist_ok=True);blob=gzip.compress(raw,compresslevel=9,mtime=0);assert gzip.decompress(blob)==raw;p.write_bytes(blob)
def csvdata(header,rows):
 f=io.StringIO(newline='');w=csv.writer(f,lineterminator='\n');w.writerow(header);w.writerows(rows);return f.getvalue().encode()
groups=[];tables={};provenance={};counts={'samples':0,'ssim':0,'foreground':0}
for label,root in PATHS.items():
 out=DEST/label;result=load(root/'results.json')
 assert not result['unexecutedCases']
 for step in result['steps']:
  assert step['exitCode']==0 and step['groupGone']and not step['timedOut']and not step['cleanupSignals']and step['sourceCleanBefore']and step['sourceCleanAfter']
  try:os.killpg(step['pgid'],0)
  except ProcessLookupError:pass
  else:raise ValueError('owned command group present')
  groups.append(step['pgid']);raw=(root/step['privateLog']).read_bytes();assert sha(raw)==step['logSha256'];compressed(out/'logs'/(step['name']+'.log.gz'),scrub(raw.decode()).encode())
 for f in root.glob('*.json'):
  data=load(f);provenance[label+'/'+f.name]={'originalSha256':sha(f.read_bytes())}
  if isinstance(data,dict):
   for c in data.get('cases',[]):
    if isinstance(c,dict)and 'encodeLatencyMs'in c:c['encodeLatencyMs'].pop('samples',None);c['accessUnits'].pop('observedFrameBytes',None)
   if 'encodeLatencyMs'in data:data['encodeLatencyMs'].pop('samples',None);data['accessUnits'].pop('observedFrameBytes',None)
   if 'variants'in data:
    for v in data['variants'].values():v.pop('rows',None)
  dump(out/f.name,data)
 if label=='availability':
  helper=Path('/tmp/fincode-probe-hevc-latency-bitrates.py');(out/'helper.py').write_text(scrub(helper.read_text()));continue
 fg=load(root/'foreground-run-1.json');baseline=fg['variants']['run-1-h264-compatible-3000']['rows'];tables[label]=[]
 for c in result['cases']:
  assert c['oraclesComplete']and len(c['encodeLatencyMs']['samples'])==240 and int(c['actualDecodedMetadata']['nb_read_frames'])==270
  samples=c['encodeLatencyMs']['samples'];sizes=c['accessUnits']['observedFrameBytes'];assert sum(sizes)==c['accessUnits']['observedAnnexBBytes']and sorted(samples)[math.ceil(.95*len(samples))-1]==c['encodeLatencyMs']['p95']
  compressed(out/(c['name']+'-samples.csv.gz'),csvdata(['observedFrame','captureFrame','sourceFrame','annexBBytes','encodeCallbackMs'],((i,30+i,30+i%240,b,samples[i])for i,b in enumerate(sizes))));counts['samples']+=len(samples)
  for region in ['full','text','thin']:
   raw=(root/'private'/c['name']/(region+'.ssim')).read_bytes();vals=[float(l.split('All:')[1].split()[0])for l in raw.decode().splitlines()];assert len(vals)==240 and math.isclose(statistics.mean(vals),c['quality'][region]['channels']['All']['mean'],abs_tol=1e-12);compressed(out/(c['name']+'-'+region+'.ssim.gz'),raw);counts['ssim']+=len(vals)
  v=fg['variants'][c['name']];rows=v['rows'];worse=sum(sum(r['sumAbsoluteCodeErrorByRgbChannel'])>sum(b['sumAbsoluteCodeErrorByRgbChannel'])for r,b in zip(rows,baseline));channels={k:sum(r['sumAbsoluteCodeErrorByRgbChannel'][i]>b['sumAbsoluteCodeErrorByRgbChannel'][i]for r,b in zip(rows,baseline))for i,k in enumerate(['r','g','b'])};assert worse==len(v['foregroundErrorWorseThanBaselineFrames'])and channels=={k:len(x)for k,x in v['foregroundChannelErrorWorseThanBaselineFrames'].items()}
  tables[label].append({'case':c['specification'],'observedBytes':c['accessUnits']['observedAnnexBBytes'],'meanAnnexBMbps':c['accessUnits']['meanAnnexBMbps'],'encodeP95Ms':c['encodeLatencyMs']['p95'],'meanForegroundRgbError':v['meanAbsoluteForegroundRgbCodeError'],'aggregateWorseFrames':worse,'channelWorseFrames':channels,'allAggregateChannelsNoWorseOnThisClip':worse==0 and not any(channels.values()),'quality':c['quality'],'processResources':c['processResources'],'scope':'Compared only with this pilot H264 baseline. Not a per-pixel/glyph proof or full profile/resource admission.'})
 for name,v in fg['variants'].items():
  assert len(v['rows'])==240
  mean=sum(sum(r['sumAbsoluteCodeErrorByRgbChannel'])for r in v['rows'])/(240*9792*3);assert math.isclose(mean,v['meanAbsoluteForegroundRgbCodeError'],abs_tol=1e-12)
  compressed(out/(name+'-foreground.csv.gz'),csvdata(['observedFrame','captureFrame','sourceFrame','rErrorSum','gErrorSum','bErrorSum','maximumForegroundCodeError'],((r['observedFrame'],30+r['observedFrame'],r['sourceFrame'],*r['sumAbsoluteCodeErrorByRgbChannel'],r['maxAbsoluteForegroundCodeError'])for r in v['rows'])));counts['foreground']+=240
dump(DEST/'summary.json',{'source':SHA,'pilots':tables,'rawRowCounts':counts,'ownedGroupsIndependentlyAbsent':groups,'productionAdmission':False,'scope':'Availability follow-up and two independent short matched pilots, each1s warmup+8s observation at1080p30 on one fixed clip/M5Pro. Different requested bitrate/profile settings are candidate tradeoffs, not isolated causal attribution. Prior dropped-output/quality failures remain; no targets/defaults/app pins changed. No new unit tests in these measurement runs.'})
dump(DEST/'provenance.json',{'originalArtifacts':provenance,'executedExporterSha256':sha(Path(__file__).read_bytes()),'normalization':'Only owned paths. Original hashes distinct from normalized files. Numeric rows unchanged; full sums/p95s/SSIM/foreground comparisons recomputed. No binary/bitstream/font/screen/decoded image/credential/private app source.'})
dump(DEST/'index.json',{'files':{str(p.relative_to(DEST)):{'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size,**({'uncompressedSha256':sha(gzip.decompress(p.read_bytes()))}if p.name.endswith('.gz')else{})}for p in sorted(DEST.rglob('*'))if p.is_file()}})
print(json.dumps({'cases':[{k:v for k,v in c.items()if k not in ['quality','processResources']}for cases in tables.values()for c in cases],'ownedGroups':len(groups),'rows':counts}))
