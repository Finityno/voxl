from pathlib import Path
import gzip,hashlib,json,os
ROOT=Path('$EXPORT_CHECKOUT');D=ROOT/'media/results/2026-11-05-maximum-frame-qp';D.mkdir(exist_ok=True)
S={k:Path('/tmp')/v for k,v in {'initial':'fincode-qp-ef7ed10','contracts':'fincode-qp-ef7ed10-retest','native':'fincode-qp-ef7ed10-native','matched-failure':'fincode-qp60-ef7ed10','partial-analysis':'fincode-qp60-partial-ef7ed10','middle':'fincode-qp60-middle-ef7ed10'}.items()};SOURCE='ef7ed1071728a84ec439f7d0e78c4d42d5e7673f'
def sha(b):return hashlib.sha256(b).hexdigest()
def normalize(t):
 pairs=[(str(v),'$'+k.upper().replace('-','_')+'_RESULTS')for k,v in S.items()]+[('$CHECKOUT','$CHECKOUT'),('$OLD_CHECKOUT','$OLD_CHECKOUT'),('$FIXTURE60','$FIXTURE60'),('$OLD_NATIVE_RESULTS','$OLD_NATIVE_RESULTS'),(str(ROOT),'$EXPORT_CHECKOUT'),('$USER','$USER')]
 for a,b in sorted(pairs,key=lambda v:len(v[0]),reverse=True):t=t.replace('/private'+a,b).replace(a,b)
 return t
origins={};states={};groups=[]
def write(name,raw):
 data=normalize(raw.decode()).encode();p=D/name;p.parent.mkdir(exist_ok=True);blob=gzip.compress(data,mtime=0)if name.endswith('.gz')else data;assert not p.exists()or p.read_bytes()==blob;p.write_bytes(blob);origins[name]={'originalPrivateSha256':sha(raw),'normalizedUncompressedSha256':sha(data),'normalizedUncompressedBytes':len(data)}
for label,path in S.items():
 raw=(path/'results.json').read_bytes();r=json.loads(raw);assert r['source']==SOURCE and r['terminal'];assert r['exitCode']==(1 if label in ['initial','contracts','matched-failure']else 0);states[label]=r;write(label+'/results.json',raw)
 for step in r['steps']:
  assert step['groupGone']and not step['timedOut']and not step['cleanupSignals']
  try:os.killpg(step['pgid'],0)
  except ProcessLookupError:pass
  else:raise ValueError('owned command group still present')
  groups.append(step['pgid']);b=(path/step['privateLog']).read_bytes();assert sha(b)==step['logSha256'];write(label+'/'+step['name']+'.log.gz',b)
 for name in ['compiler-artifact.json','fixture.json','foreground.json','failed-prefix.json']:
  file=path/name
  if file.exists():write(label+'/'+name+('.gz'if name=='foreground.json'else''),file.read_bytes())
 failure=path/'private/failure.txt'
 if failure.exists():write(label+'/supervisor-failure.txt',failure.read_bytes())
 helper=Path('$CHECKOUT/media/tools/run-matched-codecs.py')if label=='matched-failure'else Path(str(path)+'.py');assert helper.exists();assert sha(helper.read_bytes())==r['helperSha256'];write(label+'/run-snapshot.py',helper.read_bytes())
# Retain each successful HD stream's numeric SSIM exactly once. Reused baseline
# metrics in the middle follow-up are references to the original native process.
for label in ['matched-failure','middle']:
 for case in states[label].get('cases',[]):
  folder=S[label]/'private'/case['name']
  if not folder.exists():continue
  for region in ['full','text','thin']:write(label+'/'+case['name']+'-'+region+'.ssim.gz',(folder/(region+'.ssim')).read_bytes())
complete=states['partial-analysis']['analyzedRetainedCases']+[c for c in states['middle']['cases']if c['specification']!='h264:compatible:3000'];assert len(complete)==5
cases=[]
for c in complete:
 assert c['actualDecodedMetadata']['nb_read_frames']=='540'and c['accessUnits']['observedFrames']==480
 cases.append({'specification':c['specification'],'observedAnnexBMbps':c['accessUnits']['meanAnnexBMbps'],'encodeP95Ms':c['encodeLatencyMs']['p95'],'processResources':c['processResources'],'foreground':c['postprocessedForeground'],'textSsim':c['quality']['text']['channels']['All'],'encodedSha256':c['encodedSha256'],'normalizedRgbSha256':c['normalizedRgbSha256'],'nativeProperty':c['maximumFrameQp']})
contract=states['contracts']['selectedTests'];assert contract['qp-contracts']['passed']==2 and contract['priority-compatibility']['passed']==1 and contract['profile-contracts']['passed']==2
native=states['native'];assert len(native['availability'])==8 and all(c['supported']and c['nativePropertyVerified']and c['actualDecodedMetadata']['nb_read_frames']=='32'for c in native['availability']);assert len(native['defaultControls'])==3 and all(c['decodedControlsMatch']for c in native['defaultControls'])
summary={'source':SOURCE,'oldControlSource':'f098e462a62833b73d6a6103e4f50ff810fb7ffb','selectedRustTestsPassed':3,'selectedPythonProfileTestsPassed':2,'tinyNativeAvailableCombinations':8,'tinyAvailabilityDecodedFrames':256,'oldCurrentDefaultDecodedPairs':3,'oldCurrentDefaultDecodedFrames':192,'hdCompletedStreams':5,'hdDecodedFramesIncludingWarmup':2700,'hdObservedFrames':2400,'hdCases':cases,'failedHdSpecification':'hevc:low-latency-compatible:12000:qp28','failedHdPrefix':json.loads((S['middle']/'failed-prefix.json').read_text()),'priorSupervisorFailures':{'initial':'Missing harness ffmpeg metadata; no compilation occurred.','contracts':'After successful build/Clippy/selected tests and one32-frame native stream, an availability postprocessor incorrectly required a full one-second bitrate window. Separate continuation decodes that retained stream without encoder/test replay.','matched-failure':'QP28 accepted typed readback but failed with native dropped-output flag2 during warmup. Three earlier independently decoded streams analyzed separately without native replay.'},'independentlyAbsentOwnedGroups':groups,'productionAdmission':False,'consumingApplicationMediaPin':'f098e462a62833b73d6a6103e4f50ff810fb7ffb','scope':'Component experiment only, one device/generated clip. Middle candidates reuse the earlier exact-source baseline; no statistical paired timing/sustained/physical/application/hardware-decoder quality/resource/billed-cost proof. QP32/34/36 show no payload/foreground improvement; QP28 HD startup failure remains unavailable. Integer readback does not observe actual per-frame QP. No fallback to another requested setting, default/target/rate/frame-bound relaxation or app-pin update.'};(D/'summary.json').write_text(normalize(json.dumps(summary,indent=2))+'\n');write('exporter-snapshot.py',Path(__file__).read_bytes());(D/'provenance.json').write_text(json.dumps({'exports':origins,'normalization':'Owned paths replaced by named placeholders; original versus normalized hashes distinct. Published helper/exporter snapshots were not executed. No image/font/bitstream/credential/private app source.'},indent=2)+'\n');print(json.dumps({'rustPassed':3,'pythonPassed':2,'tinyCombinations':8,'decodedDefaultPairs':3,'hdCompletedStreams':5,'ownedGroupsAbsent':len(groups),'cases':[{'specification':c['specification'],'Mbps':c['observedAnnexBMbps'],'p95ms':c['encodeP95Ms'],'foreground':c['foreground']}for c in cases]},indent=2))
