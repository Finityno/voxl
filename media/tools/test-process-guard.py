"""Selected lifecycle regression checks; all spawned code is owned and bounded."""
from pathlib import Path
import os
import signal
import sys
import tempfile
import unittest
from process_guard import End, run


class ProcessGuard(unittest.TestCase):
    def command(self, code, timeout=3):
        with tempfile.TemporaryDirectory(prefix="voxl-guard-") as folder:
            with (Path(folder)/"output.log").open("w") as output:
                result = run([sys.executable, "-c", code], cwd=folder, env=os.environ.copy(),
                             output=output, seconds=timeout, natural_grace=.4,
                             term_grace=.4, kill_grace=2)
        self.assertTrue(result.leader_held_until_quiet)
        self.assertTrue(result.group_gone)
        with self.assertRaises(ProcessLookupError):
            os.killpg(result.pgid, 0)
        return result

    def test_completed_child_is_reaped_without_signalling_a_recycled_group(self):
        result = self.command("raise SystemExit(7)")
        self.assertEqual((result.code, result.child_code, result.end, result.signals), (7, 7, End.EXIT, ()))

    def test_exited_leader_reserves_identity_during_natural_descendant_drain(self):
        result = self.command("import os,time; pid=os.fork(); time.sleep(.15) if pid==0 else None; os._exit(0)")
        self.assertEqual((result.code, result.end, result.signals), (0, End.EXIT, ()))

    def test_deadline_terminates_the_entire_owned_group(self):
        result = self.command("import os,time; os.fork(); time.sleep(20)", timeout=.15)
        self.assertEqual(result.code, 124)
        self.assertEqual(result.end, End.DEADLINE)
        self.assertIn("SIGTERM", result.signals)

    def test_cancellation_is_cleaned_before_original_handlers_are_restored(self):
        before = signal.getsignal(signal.SIGTERM)
        result = self.command("import os,signal,time; time.sleep(.05); os.kill(os.getppid(),signal.SIGTERM); time.sleep(20)")
        self.assertEqual(result.code, 130)
        self.assertEqual(result.end, End.CANCELLED)
        self.assertIn("SIGTERM", result.signals)
        self.assertEqual(signal.getsignal(signal.SIGTERM), before)

    def test_completed_leader_with_stubborn_descendant_still_requires_cleanup(self):
        result = self.command("import os,signal,time; pid=os.fork(); signal.signal(signal.SIGTERM,signal.SIG_IGN) if pid==0 else None; time.sleep(20) if pid==0 else None; os._exit(0)")
        self.assertEqual((result.code, result.child_code, result.end), (0, 0, End.EXIT))
        self.assertEqual(result.signals, ("SIGTERM", "SIGKILL"))


if __name__ == "__main__":
    unittest.main()
