#!/usr/bin/env python3
"""Keep this one training job's conversion/evaluation/install chain running locally."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
RUN = ROOT / "artifacts/posttraining"
parser = argparse.ArgumentParser()
parser.add_argument("--training-pid", type=int, required=True)
args = parser.parse_args()
lock = (RUN / "finish.lock").open("w")
fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
(RUN / "job.json").write_text(json.dumps(dict(training_pid=args.training_pid, watcher_pid=os.getpid(),
    started=time.time(), directory=str(RUN)), indent=2))
(RUN / "pipeline-status.json").write_text(json.dumps(dict(phase="training", training_pid=args.training_pid), indent=2))
while True:
    try: os.kill(args.training_pid, 0)
    except ProcessLookupError: break
    time.sleep(10)
status_path = RUN / "status.json"
status = json.loads(status_path.read_text()) if status_path.exists() else {}
if status.get("phase") != "trained" or status.get("kind") != "complete":
    (RUN / "pipeline-status.json").write_text(json.dumps(dict(phase="failed", detail="Training process exited without completing",
        training_status=status, updated=time.time()), indent=2))
    raise SystemExit("Training did not complete; conversion and installation were not run")
subprocess.run([sys.executable, str(ROOT/"scripts/finish-posttraining.py")], cwd=ROOT, check=True)
