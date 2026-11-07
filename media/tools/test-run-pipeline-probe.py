import importlib.util
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location("pipeline_probe",Path(__file__).with_name("run-pipeline-probe.py"))
probe=importlib.util.module_from_spec(spec);spec.loader.exec_module(probe)


class PipelineQualityTests(unittest.TestCase):
    def test_each_frame_and_channel_regression_remains_visible(self):
        with tempfile.TemporaryDirectory() as folder:
            a,b=Path(folder)/"a.ssim",Path(folder)/"b.ssim"
            a.write_text('n:1 Y:0.8 U:0.9 V:0.9 All:0.8\nn:2 Y:0.8 U:0.9 V:0.9 All:0.8\n')
            b.write_text('n:1 Y:0.7 U:1.0 V:0.9 All:0.8\nn:2 Y:0.9 U:0.8 V:0.9 All:0.7\n')
            self.assertEqual(probe.ssim_regressions(a,b,2),{'Y':1,'U':1,'V':0,'All':1})
            self.assertEqual(probe.ssim_regressions(a,a,2),{'Y':0,'U':0,'V':0,'All':0})
            with self.assertRaises(ValueError):probe.ssim_regressions(a,b,3)
            b.write_text('n:1 Y:0.8 U:0.9 All:0.8\nn:2 Y:0.8 U:0.9 V:0.9 All:0.8\n')
            with self.assertRaises(ValueError):probe.ssim_regressions(a,b,2)


if __name__=="__main__":unittest.main()
