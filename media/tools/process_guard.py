"""Bounded POSIX CLI children; retain the leader PID until its group is quiet."""
from dataclasses import dataclass
from enum import Enum
import os
import signal
import subprocess
import threading
import time


class End(Enum):
    EXIT = "exit"
    DEADLINE = "deadline"
    CANCELLED = "cancelled"


class Cancelled(Exception):
    pass


@dataclass(frozen=True)
class Completion:
    code: int
    child_code: int
    end: End
    pgid: int
    seconds: float
    signals: tuple[str, ...]
    leader_held_until_quiet: bool
    group_gone: bool


def observed_exit(pid):
    info = os.waitid(os.P_PID, pid, os.WEXITED | os.WNOWAIT | os.WNOHANG)
    if info is None:
        return None
    if info.si_pid != pid:
        raise RuntimeError("unexpected child identity")
    if info.si_code == os.CLD_EXITED:
        return info.si_status
    if info.si_code in (os.CLD_KILLED, os.CLD_DUMPED):
        return -info.si_status
    raise RuntimeError("unexpected child exit kind")


def members(group):
    result = subprocess.run(["ps", "-axo", "pid=,pgid=,stat="], text=True,
                            capture_output=True, check=True, timeout=2)
    return [(int(parts[0]), parts[2]) for row in result.stdout.splitlines()
            if len(parts := row.split()) == 3 and int(parts[1]) == group]


def quiet(pid):
    # WNOWAIT keeps this child waitable and its PID unavailable for reuse.
    # No other group member, including an orphan zombie, may remain.
    code = observed_exit(pid)
    if code is None:
        return False
    group = members(pid)
    return all(member == pid and state.startswith("Z") for member, state in group)


def settle(pid, seconds):
    until = time.monotonic() + seconds
    while True:
        if quiet(pid):
            return True
        if time.monotonic() >= until:
            return False
        time.sleep(0.05)


def run(command, *, cwd, env, output, seconds, natural_grace=1, term_grace=4, kill_grace=3):
    """Standalone single-threaded CLI use only; no asynchronous child reaper.

    Child signals are unblocked before exec. The parent blocks cancellation
    across spawn and cleanup, so every started group gets a bounded cleanup.
    There are no group signals after wait() releases the reserved leader PID.
    """
    if threading.current_thread() is not threading.main_thread() or threading.active_count() != 1:
        raise RuntimeError("process guard requires a single-threaded CLI")
    if signal.getsignal(signal.SIGCHLD) != signal.SIG_DFL:
        raise RuntimeError("process guard cannot share an asynchronous child reaper")
    if seconds <= 0 or min(natural_grace, term_grace, kill_grace) < 0:
        raise ValueError("invalid process bounds")
    watched = {signal.SIGINT, signal.SIGTERM}
    old_mask = signal.pthread_sigmask(signal.SIG_BLOCK, watched)
    handlers = {sig: signal.getsignal(sig) for sig in watched}
    started = time.monotonic()
    actions = []

    def cancel(*_):
        raise Cancelled("process command cancelled")

    for sig in watched:
        signal.signal(sig, cancel)
    try:
        # preexec_fn is deliberately restricted to this single-threaded CLI;
        # it restores the inherited mask before the trusted tool is executed.
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True,
                                   preexec_fn=lambda: signal.pthread_sigmask(signal.SIG_SETMASK, old_mask))
        end = End.EXIT
        try:
            signal.pthread_sigmask(signal.SIG_SETMASK, old_mask)
            until = time.monotonic() + seconds
            while observed_exit(process.pid) is None:
                if time.monotonic() >= until:
                    end = End.DEADLINE
                    break
                time.sleep(0.02)
        except (Cancelled, KeyboardInterrupt):
            end = End.CANCELLED
        finally:
            signal.pthread_sigmask(signal.SIG_BLOCK, watched)
            ready = end is End.EXIT and settle(process.pid, natural_grace)
            if not ready:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                    actions.append("SIGTERM")
                except ProcessLookupError:
                    pass
                ready = settle(process.pid, term_grace)
            if not ready:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                    actions.append("SIGKILL")
                except ProcessLookupError:
                    pass
                ready = settle(process.pid, kill_grace)
            if not ready:
                raise RuntimeError("owned process group did not become quiet")
            held_code = observed_exit(process.pid)
            child_code = process.wait(timeout=1)
            if held_code != child_code:
                raise RuntimeError("reserved child exit changed")
        try:
            os.killpg(process.pid, 0)
            gone = False
        except ProcessLookupError:
            gone = True
        code = child_code if end is End.EXIT else 124 if end is End.DEADLINE else 130
        return Completion(code, child_code, end, process.pid, time.monotonic() - started,
                          tuple(actions), True, gone)
    finally:
        for sig, handler in handlers.items():
            signal.signal(sig, handler)
        signal.pthread_sigmask(signal.SIG_SETMASK, old_mask)
