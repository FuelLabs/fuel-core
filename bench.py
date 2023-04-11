# Primitive benchmarking script to test the performance of repeated dry-runs

import subprocess as sp
import asyncio
import json
import time

async def main():
    with open("tx.json") as f:
        tx = json.load(f)

    sp.run(["cargo", "build", "--release", "--bin", "fuel-core-client"])

    start = time.time()

    pending = []
    for _ in range(10_000):
        p = await asyncio.create_subprocess_exec(
            "target/release/fuel-core-client",
            "transaction",
            "dry-run",
            json.dumps(tx, separators=(",", ":")),
            stdout=asyncio.subprocess.DEVNULL,
        )
        pending.append(p.wait())
        print("started", len(pending))

    completed = []
    while pending:
        done, pending = await asyncio.wait(pending, return_when=asyncio.FIRST_COMPLETED)
        for p in done:
            completed.append(p)
        print("completed", len(completed), len(pending))

    print(time.time() - start)

if __name__ == "__main__":
    asyncio.run(main())
