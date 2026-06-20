"""Locust scenario: each user loops GET /json with no think time (closed
model). Concurrency, run time and host come from run.sh via the CLI
(-u/-r/-t/--host), so this file needs no templating."""
from locust import HttpUser, task, constant


class BenchUser(HttpUser):
    wait_time = constant(0)

    @task
    def get_json(self):
        self.client.get("/json", name="/json")
