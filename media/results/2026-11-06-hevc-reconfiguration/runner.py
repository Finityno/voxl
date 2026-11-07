from pathlib import Path
from types import SimpleNamespace
import importlib.util,os,signal,sys
sys.dont_write_bytecode=True
ROOT=Path('/Users/sondre/Documents/GitHub/voxl-priority-native-validation');SHA='cd1bd368d67a40d208f78e30dd25f05a41aead0e';OUT=Path('/tmp/voxl-hardware-admission-cd1bd368')
spec=importlib.util.spec_from_file_location('bounded',ROOT/'media/tools/run-matched-codecs.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
OUT.mkdir(mode=0o700);(OUT/'private').mkdir(mode=0o700)
r=m.Runner(SimpleNamespace(workspace=ROOT,revision=SHA,output=OUT,developer_dir=Path('/Applications/Xcode-beta.app/Contents/Developer'),runs=0,case=[],warmup_seconds=0,observation_seconds=0))
r.environment.update(CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0')
if 'RUSTC_WRAPPER' in os.environ:r.environment['RUSTC_WRAPPER']=os.environ['RUSTC_WRAPPER']
else:r.environment.pop('RUSTC_WRAPPER',None)
def interrupted(*_):raise KeyboardInterrupt()
signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted)
try:
 r.run('load',['uptime'],10);r.run('toolchain',['cargo','--version'],15);r.run('diff',['git','diff','--check'],15)
 cargo=['--locked','--manifest-path','media/Cargo.toml','--features','webrtc','--lib','-j2','-q','--message-format=short']
 r.run('test-build',['cargo','test']+cargo+['--no-run'],600,True)
 r.run('hevc-profile-contracts',['cargo','test']+cargo+['hevc_profile_','--','--nocapture'],45,True)
 r.run('strict-clippy',['cargo','clippy']+cargo+['--tests','--examples','--','-D','warnings'],600,True)
 r.exit_code=0
except BaseException as error:r.exit_code=130 if isinstance(error,KeyboardInterrupt) else 1;(OUT/'private/failure.txt').write_text(f'{type(error).__name__}: {error}\n')
finally:r.terminal=True;r.save();print('HEVC profile checks exit='+str(r.exit_code),flush=True)
raise SystemExit(r.exit_code)
