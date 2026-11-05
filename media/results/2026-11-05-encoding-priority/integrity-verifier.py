from pathlib import Path
import csv,gzip,hashlib,io,json,math,os,statistics

ROOT=Path(os.environ['VOXL_PRIORITY_EXPORT'])
def load(p):return json.loads(p.read_text())
def sha(b):return hashlib.sha256(b).hexdigest()
def rows(name):return list(csv.DictReader(io.StringIO(gzip.decompress((ROOT/name).read_bytes()).decode())))
index=load(ROOT/'index.json');summary=load(ROOT/'summary.json')
for name,m in index['files'].items():
 raw=(ROOT/name).read_bytes();assert sha(raw)==m['sha256']and len(raw)==m['bytes']
 if name.endswith('.gz'):assert sha(gzip.decompress(raw))==m['uncompressedSha256']
 assert (b'/' + b'Users' + b'/')not in (gzip.decompress(raw)if name.endswith('.gz')else raw),name
casecount=0;samplecount=0;ssimcount=0;fgcount=0
for label,cases in summary['pilots'].items():
 fg=load(ROOT/label/('foreground-run-1.json'if label=='priority-pilot'else'partial-foreground.json'))
 parsed={}
 for name,m in fg['variants'].items():
  data=rows(label+'/'+name+'-foreground.csv.gz');assert len(data)==240;fgcount+=len(data)
  sums=[[int(r[k])for k in ['rErrorSum','gErrorSum','bErrorSum']]for r in data];parsed[name]=sums
  assert math.isclose(sum(map(sum,sums))/(240*9792*3),m['meanAbsoluteForegroundRgbCodeError'],abs_tol=1e-12)
 for c in cases:
  name='run-1-'+c['case'].replace(':','-');data=rows(label+'/'+name+'-samples.csv.gz');samplecount+=len(data);assert len(data)==240
  assert sum(int(r['annexBBytes'])for r in data)==c['observedBytes']
  vals=sorted(float(r['encodeCallbackMs'])for r in data);assert vals[math.ceil(len(vals)*.95)-1]==c['encodeP95Ms']
  record=load(ROOT/label/(name+'.json'))
  for region in ['full','text','thin']:
   raw=gzip.decompress((ROOT/label/(name+'-'+region+'.ssim.gz')).read_bytes()).decode();vals=[float(line.split('All:')[1].split()[0])for line in raw.splitlines()];assert len(vals)==240;ssimcount+=len(vals)
   assert math.isclose(statistics.mean(vals),record['quality'][region]['channels']['All']['mean'],abs_tol=1e-12)
  base=parsed['run-1-h264-compatible-3000'];candidate=parsed[name]
  assert sum(sum(x)>sum(y)for x,y in zip(candidate,base))==c['aggregateWorseFramesThanH264Baseline']
  assert {k:sum(x[i]>y[i]for x,y in zip(candidate,base))for i,k in enumerate(['r','g','b'])}==c['channelWorseFramesThanH264Baseline']
  casecount+=1
 if label=='priority-pilot':
  comparisons=load(ROOT/label/'same-codec-comparisons.json')
  for codec in ['h264','hevc']:
   name='run-1-'+codec+'-compatible-'+('3000'if codec=='h264'else'1500');a=parsed[name];b=parsed[name+'-speed']
   assert sum(sum(x)>sum(y)for x,y in zip(b,a))==comparisons[codec]['speedWorseAggregateFramesVsSameCodecDefault']
   assert {k:sum(x[i]>y[i]for x,y in zip(b,a))for i,k in enumerate(['r','g','b'])}==comparisons[codec]['speedWorseChannelFramesVsSameCodecDefault']
api=load(ROOT/'api/results.json');queue=load(ROOT/'api/queue.json');native=load(ROOT/'api/native-priority.json')
assert api['exitCode']==0 and queue['exitCode']==1 and queue['steps'][0]['exitCode']==0
assert len(native['cases'])==8 and sum(c['availableAndDecoded']for c in native['cases'])==4
assert load(ROOT/'priority-pilot/results.json')['exitCode']==0
assert load(ROOT/'low-latency-pilot/results.json')['exitCode']==1
assert load(ROOT/'partial-analysis/results.json')['exitCode']==0
receipt={'source':summary['source'],'verifiedIndexEntries':len(index['files']),'casePayloadSumsP95sAndQualityRecomputed':casecount,'encodeRows':samplecount,'regionSsimRows':ssimcount,'foregroundRowsIncludingControl':fgcount,'initialQueueAndLowLatencyMatrixFailuresPreserved':True,'executedVerifierSha256':sha(Path(__file__).read_bytes()),'scope':'Offline artifact verification; no new encoder run, native provider execution or admission.'}
(ROOT/'integrity.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
