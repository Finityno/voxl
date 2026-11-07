"""Replay exact staged component evidence without rerunning capture or traffic."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess
proof=Path(__file__).resolve().parent;root=next(v for v in proof.parents if (v/'media/src/webrtc.rs').is_file())
parser=argparse.ArgumentParser();parser.add_argument('--archive',required=True,type=Path);raw=parser.parse_args().archive.resolve()
def read(f):return json.loads(f.read_text())
def digest(f):return hashlib.sha256(f.read_bytes()).hexdigest()
def checked_files(base,items):
 for item in items:
  path=(base/item['path']).resolve();assert path.is_relative_to(base) and path.stat().st_size==item['bytes'] and digest(path)==item['sha256']
tracked=set(subprocess.check_output(['git','ls-files','--',str(proof.relative_to(root))],cwd=root,text=True,timeout=10).splitlines())
files=read(proof/'files.json');checked_files(proof,files)
assert all(str((proof/v['path']).relative_to(root)) in tracked for v in files)
evidence=read(proof/'evidence.json');assert evidence['selectedRustPassed']==22 and evidence['strictClippy']
assert not any(evidence[key] for key in ['applicationDefaultEnabled','productionAdmission','newDependenciesOrFeatures'])
artifacts=read(proof/'artifacts.json');assert len(artifacts)==evidence['archiveOriginals']==22;checked_files(raw,artifacts)
groups=read(proof/'cleanup.json');assert len(groups)==evidence['ownedGroups']==len({v['pgid'] for v in groups})==15 and all(v['independentlyAbsent'] for v in groups)
for item in read(proof/'receipts.json'):
 label,receipt=item['job'],item['receipt'];assert receipt==read(raw/label/'results.json') and receipt['terminal'] and receipt['exitCode']==(1 if label=='failed-compile' else 0)
 assert digest(raw/(label+'-helper.py'))==receipt['helperSha256'] and digest(raw/'bounded-runner.py')==receipt['runnerSha256']
 for i,step in enumerate(receipt['steps']):
  assert step['exitCode']==step['childExitCode']==(101 if label=='failed-compile' and i==len(receipt['steps'])-1 else 0)
  assert step['groupGone'] and not step['timedOut'] and not step['cleanupSignals'] and step['sourceCleanBefore'] and step['sourceCleanAfter']
  assert digest(raw/label/step['privateLog'])==step['logSha256']
current=read(raw/'current/results.json');assert current['source']==evidence['source'] and current['scope']=='staged_native_worker_quiet_resume_component'
counts={}
for name in evidence['counts']:
 log=(raw/'current/private'/(name+'.log')).read_text();match=re.search(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;',log);assert match;counts[name]=int(match.group(1))
assert counts==evidence['counts'] and sum(counts.values())==22
assert any(v['name']=='media-tests-strict-clippy' and v['exitCode']==0 for v in current['steps'])
def fields(line):return {key:(value if value in ['true','false'] else int(value)) for key,value in re.findall(r'([a-z_]+)=([0-9]+|true|false)',line)}
log=(raw/'current/private/probe-network.log').read_text()
for key,prefix in [('network','quiet_resume_probe '),('idle','quiet_resume_idle '),('faults','quiet_resume_failure '),('tinyThenLarge','quiet_worker_small_delta '),('baseline','quiet_resume_baseline ')]:
 assert [fields(line) for line in log.splitlines() if line.lstrip('.').startswith(prefix)]==evidence[key]
assert len(evidence['baseline'])==len(evidence['tinyThenLarge'])==1
baseline=evidence['baseline'][0];assert baseline['quiet_rtp_packets']==0 and baseline['held_media_kbps']==1500 and baseline['estimate_age_us']>=2000000 and baseline['resume_bytes']==64005
assert baseline['send_us']<500000 and baseline['receiver_us']>1500000 and baseline['queue_max_us']>1000000 and baseline['overflow']==0
assert len(evidence['network'])==len(evidence['idle'])==len(evidence['faults'])==2
for row in evidence['network']:
 assert row['padding_packets']==32 and row['padding_rtp_bytes']==8704 and row['probe_us']<500000 and row['total_resume_us']<750000 and row['queue_max_us']<750000 and row['overflow']==0
 assert row['measured_rtp_bps']==8432*8000000//row['receiver_span_us'] and row['resume_bytes']<=row['frame_budget_bytes']*4//5
 if row['capacity_bps']==3000000:assert row['resume_bytes']==64005 and row['target_kbps']>=1000
 else:assert row['capacity_bps']==250000 and row['target_kbps']==250 and row['resume_bytes']<10000
for row in evidence['idle']:assert row['observation_ms']==3000 and row['rtp_packets']==0 and (row['forward_udp_bytes'],row['reverse_udp_bytes'],row['forward_datagrams'],row['reverse_datagrams'])==(384,444,6,6)
for row in evidence['faults']:assert row['elapsed_us']<750000 and row['fake_video_frames']==0 and row['dropped']==(0 if row['authority_close']=='true' else 1)
assert {v['authority_close'] for v in evidence['faults']}=={'true','false'}
tiny=evidence['tinyThenLarge'][0];assert tiny['small_rtp_packets']==2 and tiny['padding_packets']==32 and tiny['resume_bytes']==64005 and tiny['total_us']<750000
subprocess.run(['git','diff','--exit-code',evidence['source'],'HEAD','--','media/src','media/vendor/rtc/src','media/Cargo.toml','media/Cargo.lock','Cargo.toml','Cargo.lock'],cwd=root,check=True,timeout=15)
print('Verified 22 selected tests, strict Clippy, staged tiny/large dispatch, original-budget/epoch/loss/close gates; 22 originals and 15 independently absent groups. Synthetic component only; no native quality/resource or production admission.')
