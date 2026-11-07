from pathlib import Path
import csv,hashlib,json,os,subprocess
DEV=Path(os.environ['VOXL_SOURCE_WORKTREE']).resolve()
SHA='f1f08a424d7e405543f4088c522c19f5c49b22bc'
DEST=DEV/'media/results/2026-11-05-sustained-codecs'
LONG=Path(os.environ['VOXL_SUSTAINED_RESULTS']).resolve()
SHORT=Path(os.environ['VOXL_DIAGNOSTIC_RESULTS']).resolve()
NATIVE=Path(os.environ['VOXL_CONTRACT_RESULTS']).resolve()
FAIL=Path(os.environ['VOXL_INITIAL_CONTRACT_RESULTS']).resolve()
MAPPING={str(LONG):'$SUSTAINED',str(SHORT):'$DIAGNOSTIC',str(NATIVE):'$CONTRACTS',str(FAIL):'$INITIAL_FAILURE',os.environ['VOXL_FROZEN_CHECKOUT']:'$CHECKOUT',os.environ['VOXL_FIXTURE']:'$FIXTURE',str(Path.home()):'$USER'}
def digest(p):
 with p.open('rb')as s:return hashlib.file_digest(s,'sha256').hexdigest()
def normalized(value):
 if isinstance(value,str):
  for a,b in MAPPING.items():value=value.replace(a,b)
  return value
 if isinstance(value,list):return [normalized(v) for v in value]
 if isinstance(value,dict):return {k:normalized(v)for k,v in value.items()}
 return value
def write(name,d):
 with (DEST/name).open('x')as f:json.dump(normalized(d),f,indent=2,allow_nan=False);f.write('\n')
def absent(group):
 try:os.killpg(group,0);return False
 except ProcessLookupError:return True
for folder in [LONG,SHORT,NATIVE]:
 result=json.loads((folder/'results.json').read_text());assert result['terminal'] and result['exitCode']==0 and result['source']==SHA
 for step in result['steps']:assert step['groupGone'] and absent(step['pgid'])
 if folder!=NATIVE:assert not result['unexecutedCases']
assert subprocess.check_output(['git','diff','--name-only','f81e553ba5c1da15223c3374e57d44531c64ca8c',SHA,'--','media/src','media/vendor','media/Cargo.toml','media/Cargo.lock','media/.cargo','Cargo.toml','Cargo.lock','.cargo'],cwd=DEV,text=True,timeout=10)==''
DEST.mkdir()
validation={'source':SHA,'scope':'Only examples/tools/docs change; runtime/manifest/vendor/config inputs are unchanged. Initial and corrected contracts plus short and sustained comparisons are separate runs.','runs':{},'exports':{},'groupsIndependentlyAbsent':[]}
for label,folder in [('initial-contract-failure',FAIL),('native-contracts',NATIVE),('short-comparison',SHORT),('sustained-comparison',LONG)]:
 result=json.loads((folder/'results.json').read_text());validation['runs'][label]=result
 queue=folder/'queue.json'
 if queue.exists():
  q=json.loads(queue.read_text());assert q['terminal']
  for step in q['steps']:assert step['groupGone'] and absent(step['pgid']);validation['groupsIndependentlyAbsent'].append(step['pgid'])
  validation['exports'][label+'/queue.json']={'sha256':digest(queue),'json':q}
 for step in result['steps']:
  assert absent(step['pgid']);validation['groupsIndependentlyAbsent'].append(step['pgid'])
 for path in sorted(folder.glob('*.json')):
  if path.name in ['results.json','queue.json'] or path.name.startswith('foreground-') or path.name.startswith('run-'):continue
  if path.stat().st_size<200000:
   validation['exports'][label+'/'+path.name]={'sha256':digest(path),'json':json.loads(path.read_text())}
 if folder==FAIL or folder==NATIVE:
  for path in sorted(folder.glob('*.log')):
   if path.name=='build.log':continue
   validation['exports'][label+'/'+path.name]={'sha256':digest(path),'text':path.read_text()}
  path=folder/'run.py';validation['exports'][label+'/run.py']={'sha256':digest(path),'text':path.read_text()}
 else:
  for path in sorted((folder/'private').glob('*.log')):
   if path.name in ['build.log'] or path.name.startswith('run-1-') and not path.name.endswith('-decode.log'):continue
   if path.stat().st_size<15000:validation['exports'][label+'/'+path.name]={'sha256':digest(path),'text':path.read_text()}
  for case in result['cases']:
   # These arrays/derived summaries are published in compact CSVs below.
   case['accessUnits'].pop('observedFrameBytes');case['encodeLatencyMs'].pop('samples')
write('validation.json',validation)
summary={'source':SHA,'matrix':'One independent session per codec; 30s warmup + 300s observed at 1080p30. Same 8s generated content cycle, not a varied content corpus.','productionAdmission':False,'cases':[],'limits':json.loads((LONG/'results.json').read_text())['limits']}
raw=json.loads((LONG/'results.json').read_text())
foreground=json.loads((LONG/'foreground-run-1.json').read_text())
for case in raw['cases']:
 name=case['name'];folder=LONG/'private'/name
 case['phaseMetrics']={}
 case['accessUnits']['firstObservedFrameAnnexBBytes']=case['accessUnits']['observedFrameBytes'][0]
 case['accessUnits']['maximumObservedFrameAnnexBBytes']=max(case['accessUnits']['observedFrameBytes'])
 for phase,label in enumerate(['still','typing','scrolling','window-movement']):
  indices=[i for i in range(9000)if i%240//60==phase]
  sizes=[case['accessUnits']['observedFrameBytes'][i]for i in indices];lat=sorted(case['encodeLatencyMs']['samples'][i]for i in indices)
  case['phaseMetrics'][label]={'frames':len(indices),'meanAnnexBMbps':sum(sizes)*8*30/len(sizes)/1e6,'encodeP95Ms':lat[(len(lat)*95+99)//100-1]}
 with (DEST/(name+'-samples.csv')).open('x',newline='')as stream:
  writer=csv.writer(stream,lineterminator='\n');writer.writerow(['observedFrame','captureFrame','sourceFrame','annexBBytes','encodeCallbackMs'])
  writer.writerows((i,900+i,30+i%240,size,case['encodeLatencyMs']['samples'][i])for i,size in enumerate(case['accessUnits']['observedFrameBytes']))
 for label in ['full','text','thin']:
  text=(folder/(label+'.ssim')).read_text();(DEST/(name+'-'+label+'.ssim')).write_text(text)
 case['accessUnits'].pop('observedFrameBytes');case['encodeLatencyMs'].pop('samples')
 summary['cases'].append(case)
for label,value in foreground['variants'].items():
 with (DEST/(label+'-foreground.csv')).open('x',newline='')as stream:
  writer=csv.writer(stream,lineterminator='\n');writer.writerow(['observedFrame','captureFrame','sourceFrame','rErrorSum','gErrorSum','bErrorSum','maximumForegroundCodeError'])
  writer.writerows((row['observedFrame'],row['captureFrame'],row['sourceFrame'],*row['sumAbsoluteCodeErrorByRgbChannel'],row['maxAbsoluteForegroundCodeError'])for row in value.pop('rows'))
summary['foreground']=foreground
base,hevc=summary['cases'];assert base['specification']=='h264:compatible:3000'and hevc['specification']=='hevc:compatible:1500'
summary['observedAnnexBReductionFraction']=1-hevc['accessUnits']['observedAnnexBBytes']/base['accessUnits']['observedAnnexBBytes']
summary['byteArithmeticOnlyGbPerHour']={c['specification']:c['accessUnits']['meanAnnexBMbps']*.45 for c in summary['cases']}
write('summary.json',summary)
write('index.json',{'source':SHA,'scope':'No user screen, decoded images, font, encoded stream, credentials, SDP or private application source is published. Commands use path placeholders. Raw numeric rows remain reviewable.','files':{p.name:{'sha256':digest(p),'bytes':p.stat().st_size}for p in sorted(DEST.iterdir())if p.is_file()}})
print(json.dumps({'files':len(list(DEST.iterdir())),'bytes':sum(p.stat().st_size for p in DEST.iterdir()),'reductionFraction':summary['observedAnnexBReductionFraction'],'summaries':[{k:c[k]for k in ['name','probeWallSeconds','processResources','foregroundAnalysis','phaseMetrics']}for c in summary['cases']]},indent=2))
