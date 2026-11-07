from pathlib import Path
import gzip,hashlib,json,os
ROOT=Path('$EXPORT_CHECKOUT');D=ROOT/'media/results/2026-11-05-cadence-native';assert not D.exists();D.mkdir()
SOURCES={'generation':Path('$GENERATION_RESULTS'),'pilot12':Path('$PILOT12_RESULTS'),'budgets':Path('$BUDGETS_RESULTS')}
def sha(data):return hashlib.sha256(data).hexdigest()
def normalize(text):
 for a,b in sorted([(str(v),'$'+k.upper()+'_RESULTS')for k,v in SOURCES.items()]+[('$CHECKOUT','$CHECKOUT'),(str(ROOT),'$EXPORT_CHECKOUT'),('$ORIGINAL_FIXTURE','$ORIGINAL_FIXTURE'),('$USER','$USER')],key=lambda v:len(v[0]),reverse=True):
  text=text.replace('/private'+a,b).replace(a,b)
 return text
origins={};groups=[];cases=[]
def write(name,raw,compressed=False):
 data=normalize(raw.decode()).encode();p=D/name;p.parent.mkdir(exist_ok=True);p.write_bytes(gzip.compress(data,mtime=0)if compressed else data);origins[name]={'originalPrivateSha256':sha(raw),'normalizedUncompressedSha256':sha(data),'normalizedUncompressedBytes':len(data)}
for label,src in SOURCES.items():
 raw=(src/'results.json').read_bytes();r=json.loads(raw);assert r['source']=='1f94f7618dc3e11abb15ad858112c8f4686a2320'and r['terminal']and r['exitCode']==0
 write(label+'/results.json',raw)
 for step in r['steps']:
  assert step['groupGone']and not step['timedOut']and not step['cleanupSignals']
  assert step['exitCode']==(1 if label=='generation'and step['name'].startswith('refuse-')else 0)
  try:os.killpg(step['pgid'],0)
  except ProcessLookupError:pass
  else:raise ValueError('owned command group present')
  groups.append(step['pgid']);raw=(src/step['privateLog']).read_bytes();assert sha(raw)==step['logSha256'];write(label+'/'+step['name']+'.log.gz',raw,True)
 if label=='generation':
  for name in ('asset-validation.json','generator.json'):write(label+'/'+name,(src/name).read_bytes())
 else:
  for name in ('fixture.json','compiler-artifact.json'):write(label+'/'+name,(src/name).read_bytes())
  write(label+'/foreground.json.gz',(src/'foreground-run-1.json').read_bytes(),True)
  for case in r['cases']:
   assert case['oraclesComplete']and case['accessUnits']['observedFrames']==480 and case['accessUnits']['totalFrames']==540 and int(case['actualDecodedMetadata']['nb_read_frames'])==540
   for region in ('full','text','thin'):write(label+'/'+case['name']+'-'+region+'.ssim.gz',(src/'private'/case['name']/(region+'.ssim')).read_bytes(),True)
   cases.append({'batch':label,'specification':case['specification'],'observedAnnexBMbps':case['accessUnits']['meanAnnexBMbps'],'encodeP95Ms':case['encodeLatencyMs']['p95'],'probeCpuPercentOneCore':case['processResources']['averageCpuPercentOneCore'],'probePeakRssMiB':case['processResources']['maximumResidentBytes']/2**20,'foreground':case['foregroundAnalysis'],'textSsim':case['quality']['text']['channels']['All']})
for label,name in [('generation-driver','$GENERATION_RESULTS.py'),('asset-validator','/tmp/fincode-check-cadence-assets-1f94f76.py'),('exporter','/tmp/fincode-export-cadence-native.py')]:write(label+'-snapshot.py',Path(name).read_bytes())
write('refused-coordinator-plan.json',Path('/tmp/fincode-cadence-budget60-refused-plan.json').read_bytes())
(D/'summary.json').write_text(json.dumps({'source':'1f94f7618dc3e11abb15ad858112c8f4686a2320','fps':60,'warmupSeconds':1,'observedSeconds':8,'cases':cases,'independentlyAbsentOwnedGroups':groups,'productionAdmission':False,'scope':'Offscreen source calibration and two short native 1080p60 batches on one device and fixed generated8s clip. Five fresh streams decode540 frames each. Two H264 baselines are separate batch controls, not a five-session statistical study. No native receiver/capture/input/wire/physical endpoint/application CPU-memory-power or profile qualification. Expected generator refusals retain actual nonzero exits; the corrected out-of-bound coordinator plan started no output/build/native operation. No target/guard/default relaxation. Normalized snapshots were not executed.','normalization':'Owned paths use CHECKOUT/EXPORT_CHECKOUT, per-batch RESULTS, ORIGINAL_FIXTURE and USER placeholders; original versus normalized hashes remain distinct. No font/image/bitstream or private app source.'},indent=2)+'\n')
(D/'provenance.json').write_text(json.dumps({'exports':origins,'exporterOriginalSha256':sha(Path(__file__).read_bytes()),'scope':'Recorded logs/numeric JSON only. No native replay by export.'},indent=2)+'\n')
print(json.dumps({'cases':cases,'commandGroups':len(groups)},indent=2))
