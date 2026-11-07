from pathlib import Path
import csv,gzip,hashlib,io,json,math,os,statistics

ROOT=Path(os.environ['VOXL_REPEATABILITY_EXPORT'])
def sha(b):return hashlib.sha256(b).hexdigest()
def load(p):return json.loads(p.read_text())
index=load(ROOT/'index.json');summary=load(ROOT/'summary.json');files=0;rows=0
for name,meta in index['compressedFiles'].items():
 blob=(ROOT/name).read_bytes();raw=gzip.decompress(blob)
 assert len(blob)==meta['bytes'] and sha(blob)==meta['sha256']
 assert len(raw)==meta['uncompressedBytes'] and sha(raw)==meta['uncompressedSha256']
 assert len(raw.splitlines())-(name.endswith('.csv.gz'))==meta['dataRows']
 files+=1;rows+=meta['dataRows']
for name,meta in index['jsonFiles'].items():assert sha((ROOT/name).read_bytes())==meta['sha256']
def csvrows(name):
 p=ROOT/name
 data=gzip.decompress(p.read_bytes()).decode()if name.endswith('.gz')else p.read_text()
 return list(csv.DictReader(io.StringIO(data)))
for run in summary['runs']:
 pair={}
 for c in run['cases']:
  codec=c['specification'].split(':')[0];prefix=c['numericPrefix']
  sample=(prefix+'-samples.csv.gz')if run['name'].startswith('repeat')else next(n for n in run['existingRawFiles']if codec+'-compatible-' in n and n.endswith('-samples.csv'))
  data=csvrows(sample);assert len(data)==9000
  bytekey='annexBBytes';latencykey='encodeCallbackMs'
  assert sum(int(r[bytekey])for r in data)==c['accessUnits']['observedAnnexBBytes']
  latency=sorted(float(r[latencykey])for r in data)
  assert latency[math.ceil(.95*len(latency))-1]==c['encodeLatencyMs']['p95']
  for i,r in enumerate(data):assert int(r['observedFrame'])==i and int(r['captureFrame'])==900+i and int(r['sourceFrame'])==30+i%240
  for region in ['full','text','thin']:
   name=prefix+'-'+region+'.ssim.gz'if run['name'].startswith('repeat')else next(n for n in run['existingRawFiles']if codec+'-compatible-'in n and n.endswith('-'+region+'.ssim'))
   raw=gzip.decompress((ROOT/name).read_bytes()).decode()if name.endswith('.gz')else(ROOT/name).read_text()
   vals=[float(line.split('All:')[1].split()[0])for line in raw.splitlines()]
   assert len(vals)==9000 and math.isclose(statistics.mean(vals),c['quality'][region]['channels']['All']['mean'],abs_tol=1e-12)
  pair[codec]=c
 fg={}
 for variant,meta in run['foregroundVariants'].items():
  if run['name'].startswith('repeat'):name=run['name']+'-'+variant+'-foreground.csv.gz'
  else:name=next(n for n in run['existingRawFiles']if (variant=='uncompressed420'and 'uncompressed420'in n or variant!='uncompressed420'and variant+'-compatible-'in n)and n.endswith('-foreground.csv'))
  data=csvrows(name);assert len(data)==9000
  sums=[[int(r[k])for k in ['rErrorSum','gErrorSum','bErrorSum']]for r in data];fg[variant]=sums
  mean=sum(map(sum,sums))/(9000*9792*3)
  assert math.isclose(mean,meta['meanAbsoluteForegroundRgbCodeError'],abs_tol=1e-12)
  if variant=='uncompressed420':assert 'worseAggregateFrames'not in meta and 'worseChannelFrames'not in meta
 for codec in ['h264','hevc']:
  count=sum(sum(a)>sum(b)for a,b in zip(fg[codec],fg['h264']))
  channels={k:sum(a[i]>b[i]for a,b in zip(fg[codec],fg['h264']))for i,k in enumerate(['r','g','b'])}
  assert count==run['foregroundVariants'][codec]['worseAggregateFrames']
  assert channels==run['foregroundVariants'][codec]['worseChannelFrames']
for codec in ['h264','hevc']:
 values=[next(c for c in r['cases']if c['specification'].startswith(codec+':'))['encodeLatencyMs']['p95']for r in summary['runs']]
 assert values==summary['summary'][codec]['individualSessionEncodeP95Ms']['individual']
 assert statistics.median(values)==summary['summary'][codec]['individualSessionEncodeP95Ms']['median']
receipt={'source':summary['source'],'compressedFilesVerified':files,'compressedDataRowsVerified':rows,'allTenSessionPayloadSumsAndP95sRecomputed':True,'allThirtyRegionSsimMeansRecomputed':True,'allFifteenForegroundMeansAndTenPairwiseComparisonsRecomputed':True,'missingFloorComparisonFieldsAbsent':True,'scope':'Offline numeric artifact verification only; no native session or new codec admission.'}
(ROOT/'integrity.json').write_text(json.dumps(receipt,indent=2)+'\n')
index['supportFiles']={name:{'sha256':sha((ROOT/name).read_bytes()),'bytes':(ROOT/name).stat().st_size}for name in ['README.md','exporter.py','export-correction.json','integrity.json']}
(ROOT/'index.json').write_text(json.dumps(index,indent=2)+'\n')
print(json.dumps(receipt))
