package bench;

import static io.gatling.javaapi.core.CoreDsl.*;
import static io.gatling.javaapi.http.HttpDsl.*;

import io.gatling.javaapi.core.ScenarioBuilder;
import io.gatling.javaapi.core.Simulation;
import io.gatling.javaapi.http.HttpProtocolBuilder;
import java.time.Duration;

/**
 * Closed model: bench.users constant concurrent users loop GET /json for
 * bench.duration seconds. Parameters come from run.sh via -D system properties.
 */
public class BenchSimulation extends Simulation {

  public BenchSimulation() {
    String base = System.getProperty("bench.url", "http://localhost:18080");
    int users = Integer.parseInt(System.getProperty("bench.users", "50"));
    int duration = Integer.parseInt(System.getProperty("bench.duration", "30"));

    HttpProtocolBuilder httpProtocol = http.baseUrl(base).shareConnections();

    ScenarioBuilder scn = scenario("bench").forever().on(exec(http("json").get("/json")));

    setUp(scn.injectClosed(constantConcurrentUsers(users).during(Duration.ofSeconds(duration))))
        .protocols(httpProtocol)
        .maxDuration(Duration.ofSeconds(duration));
  }
}
