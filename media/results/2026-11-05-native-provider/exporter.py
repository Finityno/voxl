from pathlib import Path
import subprocess,json,hashlib,os,socket,tomllib,datetime
P=Path;APP=P('/Users/sondre/Documents/GitHub/fincode-desktop-access');MEDIA=P('/Users/sondre/Documents/GitHub/voxl-fincode-streaming');SHA='a519edf252ff0fcbcb65c4d74434228f0c4b52fb';MSHA='f81e553ba5c1da15223c3374e57d44531c64ca8c'
OUT=MEDIA/'media/results/2026-11-05-native-provider';OUT.mkdir(parents=True,exist_ok=True)
def digest(b):return hashlib.sha256(b).hexdigest()
def write(name,value): (OUT/name).write_text(json.dumps(value,indent=2)+'\n')
def git(repo,*args):return subprocess.check_output(['git',*args],cwd=repo,text=True,timeout=15).strip()
assert git(APP,'rev-parse','HEAD')==SHA and git(MEDIA,'rev-parse','HEAD')==MSHA
packs=[('shared-host','fincode-native-host-initial'),('provider-initial','fincode-native-provider-initial'),('provider-retest','fincode-native-provider-retest'),('provider-final','fincode-native-provider-final'),('provider-marker','fincode-native-provider-marker'),('api-initial','fincode-native-provider-api'),('api-final-failed-fixture','fincode-native-provider-api-final'),('api-final','fincode-native-provider-api-a519'),('media-wake','fincode-native-worker-wake'),('media-final','fincode-native-worker-media'),('native-mac-phone','fincode-native-host-regression-a519')]
allgroups=set();exportcount=0;native=P('/tmp/fincode-native-host-regression-a519')
for label,name in packs:
 folder=P('/tmp')/name;files=[]
 for record in ['results.json','queue.json']:
  obj=json.loads((folder/record).read_text());assert obj['terminal'] is True
  assert digest((folder/'run.py').read_bytes())==obj['helperSha256']
  for step in obj['steps']:
   if 'pgid' in step:
    assert step['groupGone'] is True;allgroups.add(step['pgid'])
   if 'logSha256' in step:
    log=folder/(('modifier' if step['name']=='modifier-observer' else step['name'])+'.log');assert digest(log.read_bytes())==step['logSha256'],(folder,step['name'])
 if label=='native-mac-phone':
  names=['run.py','results.json','queue.json','observations.json','modifier-observation.json','shutdown-confirmation.json','build-selection.json','modifier-build-selection.json']+[f.name for f in folder.glob('*.log') if f.name!='native-build.log' and not f.name.endswith('.redacted.log')]
 else:
  names=['run.py','results.json','queue.json']+[f.name for f in folder.glob('*.log')]
  if (folder/'Cargo.toml').exists():names+=['Cargo.toml','Cargo.lock','src/lib.rs']
 for name in sorted(set(names)):
  path=folder/name
  sanitized=path.with_name(path.stem+'.redacted.log') if path.suffix=='.log' else path
  if not sanitized.exists():sanitized=path
  b=sanitized.read_bytes();files.append({'name':name,'sha256':digest(b),'originalSha256':digest(path.read_bytes()),'redacted':sanitized!=path,'text':b.decode()});exportcount+=1
 write(label+'.json',{'origin':str(folder),'files':files})
# Include the generator so the partial API boundary fixture is reconstructible.
f=P('/tmp/prepare-native-provider-api.py');write('api-generator.json',{'name':f.name,'sha256':digest(f.read_bytes()),'text':f.read_text(),'limitation':'The archived fixture Cargo.toml includes copied workspace lints; each archived src/lib.rs is authoritative. Stub-module unimplemented lint is allowed only in the passing fixture, not in product code.'});exportcount+=1
# Every original process group is independently absent now.
receipts=[]
for pgid in sorted(allgroups):
 try:os.killpg(pgid,0);absent=False
 except ProcessLookupError:absent=True
 assert absent,pgid;receipts.append({'pgid':pgid,'absent':True})
# Rebind only the owned fixture HTTP loopback listener, never a shared service.
r=json.loads((native/'results.json').read_text());phone=next(s for s in r['steps'] if s['name']=='phone-tests');url=phone['extraEnvironment']['TEST_RUNNER_FINCODE_DESKTOP_AUTH_PROBE_URL'];assert url.startswith('http://127.0.0.1:');port=int(url.rsplit(':',1)[1]);sock=socket.socket();sock.bind(('127.0.0.1',port));sock.close()
assert not(native/'private/config.json').exists()
inventory=json.loads(subprocess.check_output(['/usr/bin/xcrun','simctl','list','devices','--json'],text=True,timeout=20));device=next(d for group in inventory['devices'].values() for d in group if d['udid']=='4A894872-174F-4DE8-8147-93D22AAB1058');assert device['state']=='Shutdown'
write('cleanup.json',{'checkedAtUTC':datetime.datetime.now(datetime.timezone.utc).isoformat(),'allRunnersTerminal':True,'textExports':exportcount,'processGroups':receipts,'fixtureHttpPort':port,'fixtureHttpPortRebound':True,'fixtureTokenConfigDeleted':True,'ownedSimulator':{'udid':device['udid'],'name':device['name'],'state':device['state']},'shutdownCommandExit':149,'shutdownNote':'simctl shutdown returned 149 because the owned Simulator was already Shutdown; both original confirmation and independent inventory verify Shutdown. This command is not represented as exit 0.','nativeWindowsExecuted':False,'rawScreenOrEncodedMediaExported':False,'xcresultPrivate':True})
# Lockfile semantic comparison, rather than a text-only source-ID assertion.
base='9c2f214672e8387acb6ab3632720f4d50a45e006';lock=lambda rev:tomllib.loads(git(APP,'show',rev+':Cargo.lock'))['package'];before={(x['name'],x['version']):x for x in lock(base)};after={(x['name'],x['version']):x for x in lock(SHA)};assert before.keys()==after.keys();changed=[]
old='c976515bc3b71789c472a614ae20f301ab5a334b'
for key in before:
 if before[key]!=after[key]:
  b,a=before[key],after[key];changed.append({'package':key[0],'version':key[1],'before':b,'after':a})
  if key[0]=='desktop_media':assert a==b|{'dependencies':b['dependencies']+['windows 0.61.3']}
  else:assert key[0] in ['rtc','webrtc','voxl-media'] and a==b|{'source':b['source'].replace(old,MSHA)}
assert {x['package'] for x in changed}=={'desktop_media','rtc','webrtc','voxl-media'}
write('pin-comparison.json',{'base':base,'application':SHA,'media':MSHA,'packageAndVersionSetUnchanged':True,'changes':changed})
# Narrow source equivalence keeps earlier source-specific passes in context.
paths=['crates/desktop_media/src/native_host','crates/desktop_media/src/macos','crates/desktop_media/src/statistics.rs','crates/desktop_media/src/geometry.rs'];equivalent=[]
for path in paths:
 b=git(APP,'rev-parse','5482fe96bfac0ed467f1b35c2f7a05498a11460e:'+path);a=git(APP,'rev-parse',SHA+':'+path);assert a==b;equivalent.append({'path':path,'earlierTreeOrBlob':b,'finalTreeOrBlob':a})
mediaeq=[]
for path in ['media/src/macos','media/src/webrtc.rs','media/src/webrtc','media/vendor','media/Cargo.lock']:
 b=git(MEDIA,'rev-parse',old+':'+path);a=git(MEDIA,'rev-parse',MSHA+':'+path);assert a==b;mediaeq.append({'path':path,'earlierTreeOrBlob':b,'finalTreeOrBlob':a})
write('comparison.json',{'appEarlier':'5482fe96bfac0ed467f1b35c2f7a05498a11460e','appFinal':SHA,'appEquivalentInputs':equivalent,'mediaEarlier':old,'mediaFinal':MSHA,'mediaEquivalentInputs':mediaeq,'coverageRule':'Earlier results remain attributed to their exact sources. Equal trees/blobs justify unchanged-input comparison only; they do not constitute new test executions or native Windows coverage.'})
print(json.dumps({'packs':len(packs),'textExports':exportcount,'absentGroups':len(allgroups),'output':str(OUT),'httpRebound':port,'simulator':device['state']}))
