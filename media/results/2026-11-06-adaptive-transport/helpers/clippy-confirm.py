from pathlib import Path
from types import SimpleNamespace
import importlib.util,os,signal,sys
sys.dont_write_bytecode=True
ROOT=Path('<voxl-checkout>');SHA='66bd7b014e9e43d39455ef21d70d524a2af92083';OUT=Path('/tmp/voxl-adaptive-clippy-66bd7b0')
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
 cargo=['--locked','--offline','--manifest-path','media/Cargo.toml','--features','webrtc','--lib','-j2','-q','--message-format=short']
 r.run('unchanged-core',['git','diff','--exit-code','0ec781781ba5b3063264f4f6d11dddf3f71c6bc1','HEAD','--','media/src','media/Cargo.toml','media/Cargo.lock'],15)
 r.run('strict-clippy',['cargo','clippy']+cargo+['--examples','--tests','--','-D','warnings'],300,True)
 r.exit_code=0
except BaseException as error:r.exit_code=130 if isinstance(error,KeyboardInterrupt) else 1;(OUT/'private/failure.txt').write_text(f'{type(error).__name__}: {error}\n')
finally:r.terminal=True;r.save();print('Adaptive contracts exit='+str(r.exit_code),flush=True)
raise SystemExit(r.exit_code)
