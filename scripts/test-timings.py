#!/usr/bin/env python3
"""Run the Rust test suites and report where their time goes.

Runs nextest under the `timings` profile, writes one `<seconds>s <binary>
<test>` line per test, sorted by duration, to the log file (default
`test.log`), and prints the totals per binary and per module. Per-test
durations come from a parallel run, so they include contention: compare runs
of the same machine, not absolute numbers.
"""

import collections
import resource
import time
import subprocess
import sys
import xml.etree.ElementTree as ET

LOG = sys.argv[1] if len(sys.argv) > 1 else "test.log"
REPORT = "target/nextest/timings/junit.xml"

started = time.monotonic()
run = subprocess.run(["cargo", "nextest", "run", "--workspace", "--all-targets", "--profile", "timings"])
wall = time.monotonic() - started
cpu = resource.getrusage(resource.RUSAGE_CHILDREN)

tests = []
for case in ET.parse(REPORT).getroot().iter("testcase"):
    binary = case.get("classname").split("::")[0]
    tests.append((float(case.get("time")), binary, case.get("name")))
tests.sort(key=lambda t: (t[0], t[1], t[2]))

with open(LOG, "w") as log:
    for seconds, binary, name in tests:
        log.write(f"{seconds:.3f}s {binary} {name}\n")


def table(title, totals):
    print(f"\n{title}")
    print(f"{'total':>9} {'tests':>6}  name")
    for key, (seconds, count) in sorted(totals.items(), key=lambda kv: -kv[1][0])[:25]:
        print(f"{seconds:8.1f}s {count:6}  {key}")


by_binary = collections.defaultdict(lambda: [0.0, 0])
by_module = collections.defaultdict(lambda: [0.0, 0])
for seconds, binary, name in tests:
    for totals, key in (
        (by_binary, binary),
        (by_module, f"{binary} {'::'.join(name.split('::')[:-1][:3])}"),
    ):
        totals[key][0] += seconds
        totals[key][1] += 1

print(f"\n{len(tests)} tests, {sum(t[0] for t in tests):.1f}s summed; log in {LOG}")
print(f"run: {wall:.1f}s wall, {cpu.ru_utime:.1f}s user + {cpu.ru_stime:.1f}s system CPU (includes any build)")
table("Per binary", by_binary)
table("Per module", by_module)
sys.exit(run.returncode)
