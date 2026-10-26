"""Reuse frozen archive/process helpers; 100's decisions and sources stay intact."""
import importlib.util
import sys
from pathlib import Path

old = Path(__file__).resolve().parents[1] / "100"
sys.path.insert(0, str(old))
spec = importlib.util.spec_from_file_location("research100_helpers", old / "run.py")
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)
sys.path.pop(0)

BASE = "24ca9583d8789cfdfe3ccdbfe3eeff614bf56429"
HISTORICAL = "e32523763ebf28c098ef8e83d09639672303e21f"
legacy.BASE = BASE
sha, write, inventory = legacy.sha, legacy.write, legacy.inventory
invoke, extract = legacy.invoke, legacy.extract
compact_report, archive_directory = legacy.compact_report, legacy.archive_directory
ENV = legacy.ENV

def normalize(row):
    extra = {"api_wall_ns", "counts", "phase_ns", "phase_calls", "observation_ns"}
    return legacy.normalized({k:v for k,v in row.items() if k not in extra or k=="counts"})
