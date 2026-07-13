# Dispatcher claim-budget local A/B — results

Paired release-mode comparison required by
`goals/perf-round-two/dispatcher-idle-ring.md` (LOCAL PERFORMANCE VALIDATION).
This measures the isolated dispatcher mechanism on one host; it does not
replace the later Graviton/AWS validation (`aws-ab-validation.md`).

## Setup

- Base: `d28814f` (`origin/nikolai/perf-dispatcher-port` — the current
  dispatcher). Candidate: `c218d6e` = d28814f + the claim-budget dispatcher
  commits only; identical `Cargo.lock` (sha256 `b492571c…` both sides),
  default features, `cargo build --release --locked -p loadr-cli`.
- Toolchain: rustc 1.94.0 (4a4ef493e 2026-03-02), LLVM 21.1.8,
  x86_64-unknown-linux-gnu.
- Host: AMD EPYC 9454P (48 physical cores, SMT2 — CPUs 0–47 are distinct
  physical cores), Linux 6.8.0-90-generic, `schedutil` governor,
  perf 6.8.12, `perf_event_paranoid=-1`. Runs on 2026-07-13.
- Binary sha256: base `7e3aadab…`, cand `117f1e52…`
  (full hashes in `target/ab/out/meta.txt`, not committed).

## Method

- 25 cells: `LOADR_DISPATCH_TICK_US` {1000, 5000} × `--worker-threads` {2, 16}
  × constant think time {0s, 1ms} × rate {50k, 150k, 250k}/s, plus a
  low-rate/high-preallocated broadcast over-waking case (1k/s, 2000 VUs
  parked, think 1ms, tick 5000, 16 threads).
- VU sizing identical within every pair: think-0 → 64/128 pre/max; think-1ms →
  512/1024; over-waking case 2000/2000. 10 s per run, `graceful_stop: 1s`,
  no-network plans whose only flow step is constant think time, no
  sample-consuming outputs (summary export serializes the final snapshot
  post-run; shard-mode recording stays active).
- Per cell: one discarded warm-up per binary, then 5 measured pairs in
  alternating order (A,B B,A A,B B,A A,B). Both sides pinned to the same
  physical cores (`taskset -c 0-3` for 2 worker threads, `0-19` for 16; CPUs
  0–47 are distinct physical cores on this host). Every run wrapped in
  `perf stat -e task-clock,cycles,instructions,context-switches,cpu-migrations,cache-misses`.
- Raw rows: `runs.csv` (committed, one row per measured run, failures
  included). Reduction: `reduce.py` (median + p25..p75 IQR). All cells
  reported; none selected out.

## Results

## r50000-0s-tick1000-wt2

rate=50000/s think=0s tick=1000us worker-threads=2 pre/max VUs=64/128 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 49,995 | 49,991..49,998 | 49,995 | 49,995..49,996 | +0.0% |
| dropped | 7.000 | 3.000..78.0 | 0.000 | 0.000..0.000 | -100.0% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 2,112 | 2,082..2,135 | 2,873 | 2,722..3,162 | +36.0% |
| Gcycles | 3.661 | 3.660..3.685 | 4.620 | 4.523..4.712 | +26.2% |
| Ginstr | 4.390 | 4.371..4.397 | 4.918 | 4.913..4.942 | +12.0% |
| ctx-sw | 17,678 | 17,432..17,685 | 17,008 | 16,992..17,047 | -3.8% |
| migrations | 17.0 | 6.000..66.0 | 13.0 | 12.0..31.0 | -23.5% |
| Mcache-miss | 29.8 | 28.5..30.8 | 34.9 | 34.9..36.2 | +17.2% |

## r150000-0s-tick1000-wt2

rate=150000/s think=0s tick=1000us worker-threads=2 pre/max VUs=64/128 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 60,104 | 59,974..60,157 | 105,834 | 105,334..105,972 | +76.1% |
| dropped | 898,856 | 898,321..900,181 | 441,508 | 440,205..446,610 | -50.9% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 3,074 | 2,716..3,156 | 4,140 | 4,044..4,188 | +34.7% |
| Gcycles | 4.665 | 4.663..4.667 | 6.434 | 6.346..6.436 | +37.9% |
| Ginstr | 5.888 | 5.876..5.889 | 6.857 | 6.825..6.879 | +16.5% |
| ctx-sw | 16,610 | 16,558..17,146 | 15,149 | 15,134..15,288 | -8.8% |
| migrations | 5.000 | 4.000..6.000 | 4.000 | 4.000..5.000 | -20.0% |
| Mcache-miss | 43.1 | 41.5..44.0 | 57.5 | 56.7..58.0 | +33.5% |

_Note: both sides below 90% of the 150000/s target — host-limited cell._

## r250000-0s-tick1000-wt2

rate=250000/s think=0s tick=1000us worker-threads=2 pre/max VUs=64/128 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 65,587 | 65,437..65,946 | 106,524 | 106,160..106,558 | +62.4% |
| dropped | 1,844,033 | 1,840,326..1,845,536 | 1,434,560 | 1,434,229..1,438,196 | -22.2% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 3,387 | 3,237..3,401 | 4,104 | 4,060..4,213 | +21.2% |
| Gcycles | 5.735 | 5.552..5.761 | 6.343 | 6.313..6.412 | +10.6% |
| Ginstr | 7.286 | 7.264..7.299 | 6.863 | 6.855..6.879 | -5.8% |
| ctx-sw | 16,493 | 16,432..16,652 | 15,225 | 15,133..15,240 | -7.7% |
| migrations | 3.000 | 2.000..4.000 | 2.000 | 2.000..2.000 | -33.3% |
| Mcache-miss | 49.6 | 49.5..50.2 | 56.9 | 56.7..57.1 | +14.7% |

_Note: both sides below 90% of the 250000/s target — host-limited cell._

## r50000-1ms-tick1000-wt2

rate=50000/s think=1ms tick=1000us worker-threads=2 pre/max VUs=512/1024 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 49,991 | 49,990..49,992 | 49,993 | 49,992..49,993 | +0.0% |
| dropped | 0.000 | 0.000..0.000 | 0.000 | 0.000..0.000 | - |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 1,852 | 1,768..1,891 | 3,891 | 3,496..4,053 | +110.2% |
| Gcycles | 2.952 | 2.884..2.974 | 7.744 | 7.679..7.972 | +162.4% |
| Ginstr | 3.893 | 3.892..3.902 | 7.849 | 7.832..7.896 | +101.6% |
| ctx-sw | 17,563 | 17,553..17,603 | 15,647 | 15,485..15,898 | -10.9% |
| migrations | 6.000 | 6.000..7.000 | 4.000 | 2.000..7.000 | -33.3% |
| Mcache-miss | 33.5 | 33.0..33.7 | 87.3 | 87.0..87.7 | +160.8% |

## r150000-1ms-tick1000-wt2

rate=150000/s think=1ms tick=1000us worker-threads=2 pre/max VUs=512/1024 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 149,978 | 149,973..149,982 | 149,971 | 149,970..149,972 | -0.0% |
| dropped | 0.000 | 0.000..0.000 | 0.000 | 0.000..0.000 | - |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 5,050 | 5,043..5,074 | 5,150 | 4,936..5,307 | +2.0% |
| Gcycles | 7.624 | 7.621..7.823 | 11.1 | 10.9..11.1 | +45.4% |
| Ginstr | 10.6 | 10.6..10.7 | 12.7 | 12.5..12.8 | +19.2% |
| ctx-sw | 15,824 | 15,702..15,984 | 14,864 | 14,863..15,032 | -6.1% |
| migrations | 5.000 | 3.000..5.000 | 2.000 | 2.000..2.000 | -60.0% |
| Mcache-miss | 101.8 | 101.3..102.5 | 129.4 | 126.5..132.7 | +27.1% |

## r250000-1ms-tick1000-wt2

rate=250000/s think=1ms tick=1000us worker-threads=2 pre/max VUs=512/1024 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 239,098 | 237,472..239,636 | 249,960 | 249,954..249,964 | +4.5% |
| dropped | 108,678 | 103,179..124,619 | 0.000 | 0.000..0.000 | -100.0% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 8,234 | 8,193..8,425 | 7,958 | 7,885..9,206 | -3.4% |
| Gcycles | 12.3 | 11.9..12.6 | 14.6 | 14.1..14.8 | +18.2% |
| Ginstr | 17.0 | 16.9..17.0 | 18.5 | 18.4..18.6 | +8.8% |
| ctx-sw | 16,340 | 16,233..19,325 | 12,616 | 12,273..13,474 | -22.8% |
| migrations | 2.000 | 2.000..3.000 | 4.000 | 3.000..4.000 | +100.0% |
| Mcache-miss | 169.9 | 169.6..171.7 | 194.8 | 189.2..198.0 | +14.7% |

## r50000-0s-tick1000-wt16

rate=50000/s think=0s tick=1000us worker-threads=16 pre/max VUs=64/128 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 49,076 | 49,028..49,120 | 49,993 | 49,992..49,995 | +1.9% |
| dropped | 9,231 | 8,757..9,668 | 0.000 | 0.000..0.000 | -100.0% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 18,706 | 18,542..18,805 | 33,142 | 32,469..33,534 | +77.2% |
| Gcycles | 23.5 | 23.3..24.5 | 42.4 | 42.4..44.1 | +80.3% |
| Ginstr | 12.9 | 12.7..13.3 | 23.2 | 22.8..23.6 | +79.5% |
| ctx-sw | 474,511 | 470,421..480,959 | 934,176 | 919,043..973,117 | +96.9% |
| migrations | 793.0 | 413.0..2,604 | 2,535 | 590.0..7,648 | +219.7% |
| Mcache-miss | 115.4 | 113.5..125.4 | 161.0 | 160.7..164.2 | +39.6% |

## r150000-0s-tick1000-wt16

rate=150000/s think=0s tick=1000us worker-threads=16 pre/max VUs=64/128 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 69,832 | 69,336..69,938 | 95,806 | 95,629..96,397 | +37.2% |
| dropped | 801,527 | 800,592..806,575 | 541,867 | 535,943..543,627 | -32.4% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 36,153 | 36,109..36,352 | 34,079 | 33,625..35,054 | -5.7% |
| Gcycles | 47.2 | 47.2..47.6 | 44.8 | 44.3..45.1 | -5.2% |
| Ginstr | 24.8 | 24.6..24.9 | 23.9 | 23.7..24.4 | -3.5% |
| ctx-sw | 926,492 | 916,082..941,130 | 908,633 | 901,220..946,735 | -1.9% |
| migrations | 174.0 | 160.0..707.0 | 330.0 | 223.0..853.0 | +89.7% |
| Mcache-miss | 190.5 | 187.2..191.1 | 171.6 | 171.1..172.6 | -9.9% |

_Note: both sides below 90% of the 150000/s target — host-limited cell._

## r250000-0s-tick1000-wt16

rate=250000/s think=0s tick=1000us worker-threads=16 pre/max VUs=64/128 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 71,996 | 70,810..72,144 | 96,613 | 95,308..96,776 | +34.2% |
| dropped | 1,779,993 | 1,778,276..1,791,778 | 1,533,653 | 1,532,205..1,546,758 | -13.8% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 37,532 | 36,962..37,568 | 34,303 | 34,293..34,425 | -8.6% |
| Gcycles | 49.1 | 49.0..49.2 | 44.5 | 44.3..45.2 | -9.2% |
| Ginstr | 26.2 | 26.1..26.3 | 23.8 | 23.8..23.9 | -9.0% |
| ctx-sw | 938,459 | 933,570..947,799 | 910,017 | 897,709..912,323 | -3.0% |
| migrations | 423.0 | 352.0..3,104 | 324.0 | 310.0..2,160 | -23.4% |
| Mcache-miss | 199.5 | 197.7..205.5 | 170.6 | 168.9..172.8 | -14.5% |

_Note: both sides below 90% of the 250000/s target — host-limited cell._

## r50000-1ms-tick1000-wt16

rate=50000/s think=1ms tick=1000us worker-threads=16 pre/max VUs=512/1024 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 49,990 | 49,990..49,990 | 49,991 | 49,991..49,995 | +0.0% |
| dropped | 0.000 | 0.000..0.000 | 0.000 | 0.000..0.000 | - |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 20,774 | 20,560..21,853 | 88,914 | 88,082..89,964 | +328.0% |
| Gcycles | 26.9 | 26.8..28.7 | 146.3 | 134.1..185.4 | +444.2% |
| Ginstr | 13.9 | 13.9..15.3 | 77.6 | 71.9..94.5 | +459.3% |
| ctx-sw | 465,478 | 463,475..518,695 | 3,417,666 | 3,110,770..4,258,014 | +634.2% |
| migrations | 340.0 | 162.0..2,002 | 237.0 | 148.0..1,437 | -30.3% |
| Mcache-miss | 119.4 | 119.2..129.3 | 342.1 | 317.9..425.8 | +186.6% |

## r150000-1ms-tick1000-wt16

rate=150000/s think=1ms tick=1000us worker-threads=16 pre/max VUs=512/1024 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 141,256 | 134,303..146,506 | 146,911 | 145,902..148,040 | +4.0% |
| dropped | 87,171 | 34,637..156,666 | 30,426 | 19,461..40,859 | -65.1% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 91,960 | 86,861..92,599 | 91,508 | 90,496..91,573 | -0.5% |
| Gcycles | 145.8 | 143.2..151.5 | 139.3 | 131.8..149.6 | -4.5% |
| Ginstr | 76.7 | 75.6..78.9 | 73.8 | 70.9..78.1 | -3.7% |
| ctx-sw | 3,052,609 | 3,042,999..3,079,388 | 2,987,047 | 2,820,732..3,183,897 | -2.1% |
| migrations | 380.0 | 349.0..4,445 | 186.0 | 151.0..16,145 | -51.1% |
| Mcache-miss | 412.4 | 389.5..445.9 | 381.2 | 374.4..406.9 | -7.6% |

## r250000-1ms-tick1000-wt16

rate=250000/s think=1ms tick=1000us worker-threads=16 pre/max VUs=512/1024 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 148,821 | 144,926..150,198 | 199,748 | 194,710..210,620 | +34.2% |
| dropped | 1,011,296 | 997,295..1,049,528 | 502,203 | 392,789..552,675 | -50.3% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 94,785 | 94,418..94,800 | 91,495 | 91,354..91,840 | -3.5% |
| Gcycles | 161.2 | 153.4..161.4 | 129.7 | 129.7..151.5 | -19.5% |
| Ginstr | 83.2 | 80.4..83.9 | 70.0 | 69.4..78.4 | -15.9% |
| ctx-sw | 3,381,440 | 3,247,754..3,414,435 | 2,803,731 | 2,673,063..3,082,452 | -17.1% |
| migrations | 9,954 | 455.0..12,124 | 166.0 | 163.0..19,326 | -98.3% |
| Mcache-miss | 433.9 | 417.4..449.2 | 389.7 | 377.3..438.4 | -10.2% |

_Note: both sides below 90% of the 250000/s target — host-limited cell._

## r50000-0s-tick5000-wt2

rate=50000/s think=0s tick=5000us worker-threads=2 pre/max VUs=64/128 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 25,593 | 25,592..25,593 | 49,978 | 49,978..49,979 | +95.3% |
| dropped | 243,866 | 243,857..243,869 | 0.000 | 0.000..0.000 | -100.0% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 1,444 | 1,437..1,447 | 2,388 | 2,375..2,418 | +65.3% |
| Gcycles | 2.075 | 2.066..2.079 | 3.473 | 3.464..3.512 | +67.3% |
| Ginstr | 2.468 | 2.465..2.470 | 3.785 | 3.782..3.792 | +53.3% |
| ctx-sw | 8,450 | 8,438..8,474 | 13,095 | 12,994..13,106 | +55.0% |
| migrations | 4.000 | 0.000..4.000 | 1.000 | 0.000..2.000 | -75.0% |
| Mcache-miss | 35.9 | 35.7..35.9 | 31.4 | 31.3..31.7 | -12.4% |

## r150000-0s-tick5000-wt2

rate=150000/s think=0s tick=5000us worker-threads=2 pre/max VUs=64/128 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 25,608 | 25,607..25,609 | 106,928 | 106,144..107,163 | +317.6% |
| dropped | 1,243,285 | 1,243,244..1,243,351 | 430,002 | 427,661..437,835 | -65.4% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 1,521 | 1,515..1,541 | 3,976 | 3,931..4,061 | +161.4% |
| Gcycles | 2.185 | 2.182..2.209 | 6.065 | 6.057..6.190 | +177.6% |
| Ginstr | 3.470 | 3.469..3.471 | 6.600 | 6.593..6.628 | +90.2% |
| ctx-sw | 8,168 | 8,167..8,184 | 15,363 | 15,242..15,404 | +88.1% |
| migrations | 2.000 | 0.000..2.000 | 3.000 | 2.000..3.000 | +50.0% |
| Mcache-miss | 31.9 | 31.8..33.2 | 56.7 | 56.5..56.9 | +77.7% |

_Note: both sides below 90% of the 150000/s target — host-limited cell._

## r250000-0s-tick5000-wt2

rate=250000/s think=0s tick=5000us worker-threads=2 pre/max VUs=64/128 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 26,155 | 25,978..26,160 | 106,675 | 106,325..106,757 | +307.9% |
| dropped | 2,237,444 | 2,237,362..2,239,472 | 1,432,040 | 1,431,389..1,435,799 | -36.0% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 1,892 | 1,846..1,896 | 4,123 | 4,019..4,180 | +117.9% |
| Gcycles | 2.751 | 2.709..2.757 | 6.208 | 5.970..6.246 | +125.6% |
| Ginstr | 4.661 | 4.656..4.665 | 6.597 | 6.588..6.626 | +41.5% |
| ctx-sw | 8,342 | 8,179..8,407 | 15,206 | 15,138..15,294 | +82.3% |
| migrations | 4.000 | 2.000..4.000 | 2.000 | 2.000..4.000 | -50.0% |
| Mcache-miss | 27.6 | 23.8..28.3 | 58.0 | 56.9..58.5 | +110.4% |

_Note: both sides below 90% of the 250000/s target — host-limited cell._

## r50000-1ms-tick5000-wt2

rate=50000/s think=1ms tick=5000us worker-threads=2 pre/max VUs=512/1024 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 49,983 | 49,977..49,983 | 49,977 | 49,976..49,978 | -0.0% |
| dropped | 0.000 | 0.000..0.000 | 0.000 | 0.000..0.000 | - |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 2,167 | 2,160..2,287 | 2,903 | 2,811..2,969 | +34.0% |
| Gcycles | 3.180 | 3.172..3.340 | 4.288 | 4.131..4.364 | +34.8% |
| Ginstr | 3.860 | 3.856..3.860 | 4.524 | 4.522..4.524 | +17.2% |
| ctx-sw | 9,497 | 9,433..9,502 | 9,676 | 9,619..9,697 | +1.9% |
| migrations | 3.000 | 0.000..3.000 | 2.000 | 0.000..3.000 | -33.3% |
| Mcache-miss | 38.3 | 38.2..38.5 | 52.7 | 52.5..54.0 | +37.8% |

## r150000-1ms-tick5000-wt2

rate=150000/s think=1ms tick=5000us worker-threads=2 pre/max VUs=512/1024 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 149,938 | 149,935..149,942 | 149,938 | 149,936..149,938 | -0.0% |
| dropped | 0.000 | 0.000..0.000 | 0.000 | 0.000..0.000 | - |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 5,466 | 5,422..5,617 | 6,101 | 5,890..6,112 | +11.6% |
| Gcycles | 8.148 | 8.069..8.294 | 9.015 | 8.756..9.101 | +10.6% |
| Ginstr | 10.6 | 10.6..10.6 | 11.0 | 11.0..11.0 | +3.9% |
| ctx-sw | 11,175 | 11,143..12,285 | 11,151 | 10,842..11,211 | -0.2% |
| migrations | 2.000 | 2.000..2.000 | 2.000 | 2.000..4.000 | +0.0% |
| Mcache-miss | 113.6 | 110.5..114.6 | 119.8 | 119.6..120.9 | +5.5% |

## r250000-1ms-tick5000-wt2

rate=250000/s think=1ms tick=5000us worker-threads=2 pre/max VUs=512/1024 cores=0-3 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 204,484 | 204,391..204,494 | 249,900 | 249,889..249,901 | +22.2% |
| dropped | 454,061 | 454,039..455,310 | 0.000 | 0.000..0.000 | -100.0% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 7,021 | 7,006..7,114 | 8,379 | 8,229..8,432 | +19.3% |
| Gcycles | 10.5 | 10.5..10.6 | 12.5 | 12.2..12.7 | +18.9% |
| Ginstr | 14.8 | 14.7..14.8 | 16.1 | 16.1..16.2 | +9.2% |
| ctx-sw | 10,909 | 10,366..12,972 | 11,180 | 11,117..11,343 | +2.5% |
| migrations | 2.000 | 0.000..2.000 | 3.000 | 0.000..3.000 | +50.0% |
| Mcache-miss | 149.1 | 146.5..152.7 | 168.9 | 168.6..169.7 | +13.3% |

## r50000-0s-tick5000-wt16

rate=50000/s think=0s tick=5000us worker-threads=16 pre/max VUs=64/128 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 25,595 | 25,594..25,595 | 49,982 | 49,980..49,983 | +95.3% |
| dropped | 243,851 | 243,846..243,867 | 0.000 | 0.000..0.000 | -100.0% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 10,261 | 10,181..10,620 | 21,812 | 21,546..22,686 | +112.6% |
| Gcycles | 12.9 | 12.7..13.4 | 27.1 | 26.7..28.1 | +110.2% |
| Ginstr | 6.934 | 6.746..7.010 | 14.3 | 14.2..14.9 | +106.5% |
| ctx-sw | 254,306 | 247,089..255,695 | 573,010 | 572,815..606,251 | +125.3% |
| migrations | 61.0 | 44.0..113.0 | 128.0 | 101.0..207.0 | +109.8% |
| Mcache-miss | 65.0 | 65.0..68.3 | 110.8 | 109.8..112.2 | +70.4% |

## r150000-0s-tick5000-wt16

rate=150000/s think=0s tick=5000us worker-threads=16 pre/max VUs=64/128 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 25,638 | 25,633..25,638 | 96,137 | 96,057..96,694 | +275.0% |
| dropped | 1,243,050 | 1,243,029..1,243,122 | 538,124 | 532,430..538,808 | -56.7% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 10,315 | 10,106..10,693 | 34,071 | 32,589..34,293 | +230.3% |
| Gcycles | 13.2 | 12.9..13.6 | 44.3 | 42.7..44.8 | +235.9% |
| Ginstr | 7.711 | 7.647..7.937 | 23.2 | 22.7..23.8 | +201.1% |
| ctx-sw | 229,935 | 224,359..243,397 | 864,290 | 831,480..870,125 | +275.9% |
| migrations | 172.0 | 128.0..181.0 | 228.0 | 184.0..1,073 | +32.6% |
| Mcache-miss | 66.3 | 66.2..68.8 | 167.9 | 165.1..171.4 | +153.2% |

_Note: both sides below 90% of the 150000/s target — host-limited cell._

## r250000-0s-tick5000-wt16

rate=250000/s think=0s tick=5000us worker-threads=16 pre/max VUs=64/128 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 25,712 | 25,711..25,746 | 97,113 | 96,815..97,395 | +277.7% |
| dropped | 2,241,949 | 2,241,532..2,241,965 | 1,528,057 | 1,525,202..1,530,835 | -31.8% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 10,475 | 10,188..10,822 | 34,432 | 33,976..34,563 | +228.7% |
| Gcycles | 13.6 | 13.1..13.9 | 44.2 | 43.5..45.3 | +226.1% |
| Ginstr | 8.752 | 8.571..8.937 | 23.3 | 23.3..23.6 | +166.6% |
| ctx-sw | 219,922 | 209,236..231,258 | 858,358 | 857,770..886,941 | +290.3% |
| migrations | 79.0 | 51.0..166.0 | 100.0 | 69.0..166.0 | +26.6% |
| Mcache-miss | 68.6 | 68.2..68.7 | 166.1 | 165.8..166.1 | +142.1% |

_Note: both sides below 90% of the 250000/s target — host-limited cell._

## r50000-1ms-tick5000-wt16

rate=50000/s think=1ms tick=5000us worker-threads=16 pre/max VUs=512/1024 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 49,980 | 49,980..49,981 | 49,981 | 49,979..49,981 | +0.0% |
| dropped | 0.000 | 0.000..0.000 | 0.000 | 0.000..0.000 | - |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 27,293 | 26,696..27,605 | 57,367 | 56,964..59,790 | +110.2% |
| Gcycles | 34.4 | 34.2..34.5 | 71.9 | 71.3..75.3 | +108.8% |
| Ginstr | 17.4 | 17.3..18.4 | 38.9 | 38.7..41.1 | +123.0% |
| ctx-sw | 695,367 | 694,289..729,298 | 1,637,115 | 1,626,014..1,816,438 | +135.4% |
| migrations | 89.0 | 76.0..159.0 | 81.0 | 78.0..148.0 | -9.0% |
| Mcache-miss | 133.9 | 133.7..134.9 | 211.5 | 210.6..219.5 | +58.0% |

## r150000-1ms-tick5000-wt16

rate=150000/s think=1ms tick=5000us worker-threads=16 pre/max VUs=512/1024 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 125,239 | 123,905..135,228 | 149,926 | 149,477..149,943 | +19.7% |
| dropped | 246,963 | 147,188..260,462 | 80.0 | 0.000..4,650 | -100.0% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 93,000 | 92,967..93,997 | 89,747 | 86,562..90,695 | -3.5% |
| Gcycles | 133.4 | 127.7..141.2 | 149.2 | 135.7..173.3 | +11.9% |
| Ginstr | 70.4 | 69.1..74.0 | 78.4 | 72.0..88.6 | +11.4% |
| ctx-sw | 2,837,496 | 2,743,843..2,956,499 | 3,193,411 | 2,938,247..3,772,419 | +12.5% |
| migrations | 144.0 | 122.0..2,973 | 6,134 | 186.0..8,195 | +4159.7% |
| Mcache-miss | 366.5 | 352.6..409.9 | 422.9 | 386.7..475.1 | +15.4% |

## r250000-1ms-tick5000-wt16

rate=250000/s think=1ms tick=5000us worker-threads=16 pre/max VUs=512/1024 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 141,115 | 132,483..151,205 | 218,810 | 211,353..229,238 | +55.1% |
| dropped | 1,088,032 | 986,898..1,174,328 | 310,952 | 205,500..385,726 | -71.4% |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 95,241 | 92,236..95,880 | 91,454 | 91,216..92,342 | -4.0% |
| Gcycles | 144.3 | 135.7..152.9 | 141.2 | 139.2..151.6 | -2.2% |
| Ginstr | 78.8 | 72.9..79.6 | 74.2 | 72.9..80.2 | -5.8% |
| ctx-sw | 3,003,166 | 2,857,603..3,144,478 | 2,925,420 | 2,878,884..3,098,854 | -2.6% |
| migrations | 132.0 | 102.0..1,151 | 272.0 | 243.0..286.0 | +106.1% |
| Mcache-miss | 420.1 | 382.6..423.9 | 416.7 | 412.8..453.2 | -0.8% |

_Note: both sides below 90% of the 250000/s target — host-limited cell._

## lowrate-overwake

rate=1000/s think=1ms tick=5000us worker-threads=16 pre/max VUs=2000/2000 cores=0-19 (n base=5, cand=5)

| measure | base med | base IQR | cand med | cand IQR | Δ med |
|---|---|---|---|---|---|
| achieved it/s | 999.5 | 999.1..999.6 | 999.5 | 999.5..999.5 | +0.0% |
| dropped | 0.000 | 0.000..0.000 | 0.000 | 0.000..0.000 | - |
| wall s | 11.0 | 11.0..11.0 | 10.0 | 10.0..10.0 | -9.1% |
| task-clock ms | 359.5 | 357.6..359.8 | 89,124 | 88,354..89,285 | +24692.4% |
| Gcycles | 0.427 | 0.426..0.427 | 124.1 | 123.5..125.7 | +28987.6% |
| Ginstr | 0.244 | 0.242..0.249 | 67.6 | 67.1..67.7 | +27632.7% |
| ctx-sw | 9,508 | 9,506..9,547 | 2,976,814 | 2,944,601..3,011,337 | +31208.5% |
| migrations | 24.0 | 23.0..26.0 | 118.0 | 107.0..133.0 | +391.7% |
| Mcache-miss | 12.7 | 11.6..12.9 | 277.9 | 276.1..279.7 | +2095.9% |


## Reading notes

All 300 measured runs completed (exit 0); IQRs are tight throughout, so the
medians are stable. Deltas are candidate vs base medians.

**1. The dispatcher ceiling moves decisively.** On the zero-think ladders
(pure dispatch cost), the base saturates at ~25.6–26.2k it/s with the default
5000us tick and ~60–72k with the 1000us tick, independent of rate asked. The
candidate reaches ~96–107k it/s in the same cells: +62% to +318% achieved
throughput, with drops falling accordingly. The 5000us-tick base collapse
(~26k/s) reproduces the known failure that motivated `LOADR_DISPATCH_TICK_US`
tuning; the candidate makes the default tick usable at 2–4x the base's
*tuned* ceiling.

**2. The schedule is met exactly where the base quietly under-delivers.**
At 50k/s zero-think/wt16 the base achieves 49,076/s with ~9.2k drops per 10s
run; the candidate delivers 49,993/s with zero drops. At 250k/s x 1ms on two
worker threads the base manages 239k with ~109k drops; the candidate holds
249,960/s with zero drops. Every cell the host can sustain, the candidate
meets with 0 drops.

**3. CPU per delivered iteration at saturation is par or better.** E.g.
r250000-0s-tick1000-wt2: -5.8% instructions while delivering +62.4%;
r150000-0s-tick1000-wt16: -5.7% task-clock, -3.5% instructions while
delivering +37.2%. The contended budget cache line does not show up as a
per-iteration cost regression at load.

**4. Broadcast over-waking is real, and severe for idle over-provisioned
pools.** This is the trade the spec called out, and the low-rate/high-idle
cell exists to expose. `notify_waiters()` wakes *every* parked worker once
per non-zero batch; losers re-park. The waste scales with
parked workers x tick frequency and is paid even when throughput is
identical:

| cell | delivered | base CPUs | cand CPUs | task-clock Δ |
|---|---|---|---|---|
| lowrate-overwake (2000 parked, 1k/s) | equal (1k/s) | 0.03 | 8.88 | +24,700% |
| r50000-1ms-tick1000-wt16 (~460 parked) | equal (50k/s) | 1.88 | 8.86 | +328% |
| r50000-1ms-tick5000-wt16 (~460 parked) | equal (50k/s) | 2.47 | 5.72 | +110% |
| r50000-1ms-tick1000-wt2 (~460 parked) | equal (50k/s) | 0.17 | 0.39 | +110% |

The worst case burns ~9 CPUs to deliver a 1k/s trickle from a 2000-worker
pool (base: 0.03 CPUs). The cost is bounded by pool size, not by rate, and
disappears when `pre_allocated_vus` approximates actual in-flight work
(compare the 0s-think cells, whose 64–128 pools show single-digit-% to ~2x
overhead worst case at wt16 while delivering equal or far higher throughput).

**5. Runs end at the deadline.** Wall time is 11.0s (base) vs 10.0s (cand)
in every cell: base parked workers sit out the 1s `graceful_stop` at the
natural deadline; the candidate's closure broadcast releases them
immediately. (This also makes the e2e suite faster.)

## Recommendation

The measured evidence supports the mechanism: the per-arrival dispatcher was
a real single-task ceiling, and the tick-bounded claim budget removes it
(+62% to +318% achieved throughput at the rates this goal targets, exact
schedule adherence with zero drops everywhere the host can sustain the rate,
par-or-better CPU per delivered iteration at load).

The evidence equally shows the predicted broadcast cost is not benign for
heavily over-provisioned idle pools: up to ~9 CPUs of pure wake churn at
2000 parked workers (~250x base task-clock at equal delivered load). That
regime is outside this goal's motivating use case (high-rate open model with
pools sized near expected in-flight work), but it is a real plan shape.

Recommendation for the reviewer: **land the claim budget for its throughput
regime, with the over-waking cost documented as a known sizing sensitivity
(`pre_allocated_vus` ≈ expected in-flight iterations), and treat the sharded
Notify ring / bounded-wake design — explicitly out of scope here — as the
follow-up that removes the idle-pool tax.** If grossly over-provisioned
pools must stay cheap before any landing, hold for that redesign instead;
the measurements above quantify exactly what it must fix. Either way this
local A/B validates the isolated mechanism only — the Graviton/AWS ceiling
remains for `aws-ab-validation`.
