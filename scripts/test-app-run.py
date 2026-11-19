"""Exercise desktop launch routing without starting Proton or a real terminal."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class AppRunTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="poe-app-run-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.app = self.root / "App Dir"
        (self.app / "usr/bin").mkdir(parents=True)
        self.capture = self.root / "launch.json"
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.write_program(self.app / "usr/bin/poe2-auto-flask", False)
        self.write_program(self.bin / "konsole", True)
        self.env = {
            **os.environ,
            "APPDIR": str(self.app),
            "APPIMAGE": str(self.root / "Test App.AppImage"),
            "PATH": f"{self.bin}:/usr/bin:/bin",
            "TEST_CAPTURE": str(self.capture),
        }
        self.env.pop("POE2_AUTO_FLASK_TERMINAL_LAUNCH", None)

    def write_program(self, path, terminal):
        path.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "from pathlib import Path\n"
            "Path(os.environ['TEST_CAPTURE']).write_text(json.dumps({"
            f"'terminal': {terminal!r}, "
            "'args': sys.argv[1:], "
            "'payload': os.environ.get('POE2_AUTO_FLASK_BUNDLED_HELPER')}))\n"
            "sys.exit(int(os.environ.get('TEST_EXIT', '0')))\n"
        )
        path.chmod(0o755)

    def run_app(self, *args):
        app_run = Path(__file__).resolve().parents[1] / "packaging/linux/AppRun"
        result = subprocess.run(
            ["sh", str(app_run), *args], env=self.env, capture_output=True, text=True
        )
        return result, json.loads(self.capture.read_text())

    def test_normal_desktop_launch_uses_gui_payload_without_terminal(self):
        result, launch = self.run_app("--game", "poe1", "--helper", "/a b/ui.exe")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(launch["terminal"])
        self.assertEqual(launch["args"], ["--game", "poe1", "--helper", "/a b/ui.exe"])
        self.assertEqual(
            launch["payload"], str(self.app / "usr/lib/poe2-auto-flask/poe2-auto-flask.exe")
        )

    def test_desktop_debug_launch_opens_terminal(self):
        result, launch = self.run_app("--debug", "--game", "poe1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(launch["terminal"])
        self.assertEqual(launch["args"], [
            "-e", str(self.app / "usr/bin/poe2-auto-flask-terminal"),
            self.env["APPIMAGE"], "--debug", "--game", "poe1",
        ])

    def test_debug_terminal_wrapper_does_not_open_another_terminal(self):
        self.env["POE2_AUTO_FLASK_TERMINAL_LAUNCH"] = "1"
        result, launch = self.run_app("--debug")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(launch["terminal"])
        self.assertEqual(launch["args"], ["--debug"])

    def test_launcher_failure_keeps_its_exit_status(self):
        self.env["TEST_EXIT"] = "17"
        result, _ = self.run_app()
        self.assertEqual(result.returncode, 17)


if __name__ == "__main__":
    unittest.main()
