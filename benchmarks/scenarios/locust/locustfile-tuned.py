"""Tuned Locust scenario. Two changes that matter (see ../../CONFIGURATION.md):
  1. FastHttpUser (geventhttpclient) instead of the requests-based HttpUser —
     ~4x less CPU per request on the generator.
  2. run.sh launches it with `--processes -1` (one worker per core) to escape
     the single-process GIL ceiling.
Concurrency, run time and host still come from run.sh via the CLI."""
from locust import FastHttpUser, task, constant


class BenchUser(FastHttpUser):
    wait_time = constant(0)

    @task
    def get_json(self):
        self.client.get("/json", name="/json")
