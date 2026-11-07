"""Verify retained scalar quiet-resume evidence, not decoder/quality admission."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess
p=Path(__file__).resolve().parent;root=next(v for v in p.parents if (v/'media/src/webrtc.rs').is_file())
parser=argparse.ArgumentParser();parser.add_argument('--archive',required=True,type=Path);args=parser.parse_args();raw=args.archive.resolve()
def read(f):return json.loads(f.read_text())
def digest(f):return hashlib.sha256(f.read_bytes()).hexdigest()
tracked=set(subprocess.check_output(['git','ls-files','--',str(p.relative_to(root))],cwd=root,text=True,timeout=10).splitlines())
for item in read(p/'files.json'):
    f=(p/item['path']).resolve();assert f.is_relative_to(p) and str(f.relative_to(root)) in tracked and f.stat().st_size==item['bytes'] and digest(f)==item['sha256']
e=read(p/'evidence.json');assert e['selectedRustPassed']==19 and e['strictClippy'] and not any(e[key] for key in ['productionAdmission','applicationIntegrated','newDependenciesOrFeatures'])
artifacts=read(p/'artifacts.json');assert len(artifacts)==e['archiveOriginals']
for item in artifacts:
    f=(raw/item['path']).resolve();assert f.is_relative_to(raw) and f.stat().st_size==item['bytes'] and digest(f)==item['sha256']
for item in read(raw/'retention-initial-index.json'):
    f=(raw/item['path']).resolve();assert f.is_relative_to(raw) and f.stat().st_size==item['bytes'] and digest(f)==item['sha256']
groups=read(p/'cleanup.json');assert len({v['pgid'] for v in groups})==e['ownedGroups'] and all(v['independentlyAbsent'] for v in groups)
for saved in read(p/'receipts.json'):
    label=saved['job'];v=saved['receipt'];assert v==read(raw/label/'results.json') and v['terminal'] and v['exitCode']==(1 if label.startswith('failed-') else 0)
    assert digest(raw/(label+'-helper.py'))==v['helperSha256'] and digest(raw/'bounded-runner.py')==v['runnerSha256']
    for i,step in enumerate(v['steps']):
        assert step['exitCode']==step['childExitCode']==(101 if label.startswith('failed-') and i==len(v['steps'])-1 else 0)
        assert step['groupGone'] and not step['timedOut'] and not step['cleanupSignals'] and step['sourceCleanBefore'] and step['sourceCleanAfter']
        assert digest(raw/label/step['privateLog'])==step['logSha256']
current=read(raw/'current/results.json');assert current['source']==e['source'] and current['scope']=='owned_encrypted_udp_quiet_resume_probe'
counts={}
for name in e['counts']:
    output=(raw/'current/private'/(name+'.log')).read_text();match=re.search(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;',output);assert match;counts[name]=int(match.group(1))
assert counts==e['counts'] and sum(counts.values())==19
assert any(v['name']=='media-tests-strict-clippy' and v['exitCode']==0 for v in current['steps'])
def fields(line):return {key:(value if value in ['true','false'] else int(value)) for key,value in re.findall(r'([a-z_]+)=([0-9]+|true|false)',line)}
log=(raw/'current/private/probe-network.log').read_text()
for key,prefix in [('network','quiet_resume_probe '),('idle','quiet_resume_idle '),('faults','quiet_resume_failure ')]:
    parsed=[fields(line) for line in log.splitlines() if line.lstrip('.').startswith(prefix)];assert parsed==e[key] and len(parsed)==2
baseline=fields(next(line for line in (raw/'current/private/quiet-resume-baseline.log').read_text().splitlines() if line.startswith('quiet_resume_baseline ')));assert baseline==e['baseline']
assert baseline['quiet_rtp_packets']==0 and baseline['held_media_kbps']==1500 and baseline['estimate_age_us']>=2000000 and baseline['resume_bytes']==64005
assert baseline['send_us']<500000 and baseline['receiver_us']>1500000 and baseline['queue_max_us']>1000000 and baseline['overflow']==0
for row in e['network']:
    assert row['padding_packets']==32 and row['padding_rtp_bytes']==8704 and row['probe_us']<500000 and row['total_resume_us']<750000 and row['overflow']==0
    assert row['measured_rtp_bps']==(8704-272)*8000000//row['receiver_span_us'] and row['resume_bytes']<=row['frame_budget_bytes']*4//5
    if row['capacity_bps']==3000000:assert row['resume_bytes']==64005 and row['target_kbps']>=1000
    else:assert row['capacity_bps']==250000 and row['target_kbps']==250 and row['resume_bytes']<10000
for row in e['idle']:
    assert row['observation_ms']==3000 and row['rtp_packets']==0 and row['forward_udp_bytes']>0 and row['reverse_udp_bytes']>0 and row['forward_datagrams']>0 and row['reverse_datagrams']>0
for row in e['faults']:
    assert row['elapsed_us']<750000 and row['fake_video_frames']==0 and row['dropped']==(0 if row['authority_close']=='true' else 1)
assert {v['authority_close'] for v in e['faults']}=={'true','false'}
subprocess.run(['git','diff','--exit-code',e['source'],'HEAD','--','media/src','media/vendor/rtc/src','media/Cargo.toml','media/Cargo.lock','Cargo.toml','Cargo.lock'],cwd=root,check=True,timeout=15)
print(f"Verified 19 selected Rust tests, strict library/test Clippy and current quiet-resume baseline/probe/debt/loss/close/expiry evidence; {e['ownedGroups']} recorded groups independently absent. Slow resume uses a smaller payload; no native decoder, equal-quality savings, physical endpoint, full application integration, CPU/RAM/GPU/billed traffic or production admission.")
