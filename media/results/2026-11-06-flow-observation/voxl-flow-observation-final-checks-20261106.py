from pathlib import Path
from types import SimpleNamespace
import importlib.util,os,signal,sys
sys.dont_write_bytecode=True
ROOT=Path('/Users/sondre/Documents/GitHub/voxl-priority-native-validation');SHA='0a252d18af96ade1bf13d6319c45c61edef87238';OUT=Path('/tmp/voxl-flow-observation-0a252d18')
spec=importlib.util.spec_from_file_location('bounded',Path('/Users/sondre/Documents/GitHub/voxl-priority-native-validation/media/tools/run-matched-codecs.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
r=m.Runner(SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=0,case=[],warmup_seconds=0,observation_seconds=0))
r.environment.update(CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_TARGET_DIR=str(ROOT/'media/target'),FINCODE_CHANNEL='dev',FINCODE_COMMIT_SHA=SHA)
if 'RUSTC_WRAPPER' in os.environ:r.environment['RUSTC_WRAPPER']=os.environ['RUSTC_WRAPPER']
else:r.environment.pop('RUSTC_WRAPPER',None)
def interrupted(*_):raise KeyboardInterrupt()
signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted)
try:
 r.run('load',['uptime'],10);r.run('toolchain',['cargo','--version'],15);r.run('diff',['git','diff','--check'],15)
 r.run('disk-headroom',['python3','-c','import shutil; free=shutil.disk_usage(".").free; print("freeBytes="+str(free)); assert free>=55_000_000_000'],15)
 cargo=['--locked','--manifest-path','media/Cargo.toml','--features','webrtc','--lib','-j2','-q','--message-format=short']
 r.run('test-build',['cargo','test']+cargo+['--no-run'],600,True)
 r.run('flow-contracts',['cargo','test']+cargo+['flow_observation_','--','--nocapture'],45,True)
 r.run('producer-debt-contracts',['cargo','test']+cargo+['producer_debt_','--','--nocapture'],45,True)
 r.run('pacing-epoch-contracts',['cargo','test']+cargo+['adaptive_','--','--nocapture'],90,True)
 r.run('strict-clippy',['cargo','clippy']+cargo+['--tests','--examples','--','-D','warnings'],600,True)
 r.exit_code=0
except BaseException as error:r.exit_code=130 if isinstance(error,KeyboardInterrupt) else 1;(OUT/'private/failure.txt').write_text(f'{type(error).__name__}: {error}\n')
finally:r.terminal=True;r.save();print('Reference worker checks exit='+str(r.exit_code),flush=True)
raise SystemExit(r.exit_code)
